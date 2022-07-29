//  Copyright 2022 Datafuse Labs.
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

use std::sync::Arc;

use common_arrow::arrow::io::parquet::read::column_iter_to_arrays;
use common_arrow::arrow::io::parquet::read::infer_schema;
use common_arrow::arrow::io::parquet::read::read_metadata_async;
use common_arrow::arrow::io::parquet::read::RowGroupDeserializer;
use common_arrow::parquet::compression::Compression;
use common_arrow::parquet::metadata::ColumnChunkMetaData;
use common_arrow::parquet::metadata::FileMetaData;
use common_arrow::parquet::read::BasicDecompressor;
use common_arrow::parquet::read::PageMetaData;
use common_arrow::parquet::read::PageReader;
use common_cache::Cache;
use common_catalog::table_context::TableContext;
use common_datablocks::DataBlock;
use common_datavalues::DataField;
use common_datavalues::DataSchema;
use common_datavalues::ToDataType;
use common_datavalues::Vu8;
use common_exception::ErrorCode;
use common_exception::Result;
use common_tracing::tracing;
use futures_util::future::try_join_all;
use opendal::Operator;

#[tracing::instrument(level = "debug", skip_all)]
pub async fn load_bloom_filter_by_columns(
    ctx: &Arc<dyn TableContext>,
    dal: Operator,
    columns: &[String],
    path: &str,
) -> Result<DataBlock> {
    let file_meta = read_index_meta(ctx, path, &dal).await?;
    let row_group = &file_meta.row_groups[0];

    let fields = columns
        .iter()
        .map(|name| DataField::new(name, Vu8::to_data_type()))
        .collect::<Vec<_>>();

    let schema = Arc::new(DataSchema::new(fields));

    let futs = columns
        .iter()
        .map(|col_name| load_column_bytes(ctx, &file_meta, col_name, path, &dal))
        .collect::<Vec<_>>();
    let cols_data = try_join_all(futs).await?;

    let column_descriptors = file_meta.schema_descr.columns();
    let arrow_schema = infer_schema(&file_meta)?;
    let mut columns_array_iter = vec![];
    let num_values = row_group.num_rows();
    for (bytes, col_idx) in cols_data {
        let page_meta_data = PageMetaData {
            column_start: 0, // PageReader does not care about this
            num_values: num_values as i64,
            compression: Compression::Lz4Raw, // compression for bloom filter might not be sensible
            descriptor: file_meta.schema_descr.columns()[col_idx].descriptor.clone(),
        };
        let pages = PageReader::new_with_page_meta(
            std::io::Cursor::new(bytes.as_ref().clone()), // heart breaking
            page_meta_data,
            Arc::new(|_, _| true),
            vec![],
        );
        let decompressor = BasicDecompressor::new(pages, vec![]);
        let decompressors = vec![decompressor];
        let types = vec![&column_descriptors[col_idx].descriptor.primitive_type];
        let field = arrow_schema.fields[col_idx].clone();
        columns_array_iter.push(column_iter_to_arrays(
            decompressors,
            types,
            field,
            Some(num_values),
        )?);
    }

    let mut deserializer = RowGroupDeserializer::new(columns_array_iter, num_values, None);

    match deserializer.next() {
        None => Err(ErrorCode::ParquetError("fail to get a chunk")),
        Some(Err(cause)) => Err(ErrorCode::from(cause)),
        Some(Ok(chunk)) => DataBlock::from_chunk(&schema, &chunk),
    }
}

/// return bytes and index of the given column
async fn load_column_bytes(
    ctx: &Arc<dyn TableContext>,
    file_meta: &FileMetaData,
    col_name: &str,
    path: &str,
    dal: &Operator,
) -> Result<(Arc<Vec<u8>>, usize)> {
    let cols = file_meta.row_groups[0].columns();
    if let Some((idx, col_meta)) = cols
        .iter()
        .enumerate()
        .find(|(_, c)| c.descriptor().path_in_schema[0] == col_name)
    {
        let cache_key = format!("{path}-{idx}");
        if let Some(bloom_index_cache) = ctx.get_storage_cache_manager().get_bloom_index_cache() {
            let cache = &mut bloom_index_cache.write().await;
            if let Some(bytes) = cache.get(&cache_key) {
                Ok((bytes.clone(), idx))
            } else {
                let bytes = load_data(col_meta, dal, path).await?;
                let bytes = Arc::new(bytes);
                cache.put(cache_key, bytes.clone());
                Ok((bytes, idx))
            }
        } else {
            let bytes = load_data(col_meta, dal, path).await?;
            Ok((Arc::new(bytes), idx))
        }
    } else {
        Err(ErrorCode::LogicalError(format!(
            "no such column {col_name}"
        )))
    }
}

async fn read_index_meta(
    ctx: &Arc<dyn TableContext>,
    path: &str,
    dal: &Operator,
) -> Result<Arc<FileMetaData>> {
    let cache_key = format!("{path}-bf");
    if let Some(bloom_index_meta_cache) =
        ctx.get_storage_cache_manager().get_bloom_index_meta_cache()
    {
        let cache = &mut bloom_index_meta_cache.write().await;
        if let Some(file_meta) = cache.get(&cache_key) {
            Ok(file_meta.clone())
        } else {
            let file_meta = Arc::new(load_index_meta(dal, path).await?);
            cache.put(cache_key, file_meta.clone());
            Ok(file_meta)
        }
    } else {
        let file_meta = Arc::new(load_index_meta(dal, path).await?);
        Ok(file_meta)
    }
}

async fn load_index_meta(dal: &Operator, path: &str) -> Result<FileMetaData> {
    let object = dal.object(path);
    let mut reader = object.seekable_reader(0..);
    let file_meta = read_metadata_async(&mut reader).await?;
    Ok(file_meta)
}

async fn load_data(col_meta: &ColumnChunkMetaData, dal: &Operator, path: &str) -> Result<Vec<u8>> {
    let chunk_meta = col_meta.metadata();
    let chunk_offset = chunk_meta.data_page_offset as u64;
    let col_len = chunk_meta.total_compressed_size as u64;
    let column_reader = dal.object(path);
    let bytes = column_reader
        .range_read(chunk_offset..chunk_offset + col_len)
        .await?;
    Ok(bytes)
}
