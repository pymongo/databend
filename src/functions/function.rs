// Copyright 2020 The FuseQuery Authors.
//
// Code is licensed under AGPL License, Version 3.0.

use std::fmt;

use crate::datablocks::DataBlock;
use crate::datavalues::{DataColumnarValue, DataSchema, DataType};
use crate::error::FuseQueryResult;
use crate::functions::function_logic::LogicFunction;
use crate::functions::{
    AggregatorFunction, AliasFunction, ArithmeticFunction, ComparisonFunction, ConstantFunction,
    VariableFunction,
};

#[derive(Clone)]
pub enum Function {
    Alias(AliasFunction),
    Constant(ConstantFunction),
    Variable(VariableFunction),
    Arithmetic(ArithmeticFunction),
    Comparison(ComparisonFunction),
    Logic(LogicFunction),
    Aggregator(AggregatorFunction),
}

impl Function {
    pub fn return_type(&self, input_schema: &DataSchema) -> FuseQueryResult<DataType> {
        match self {
            Function::Alias(v) => v.return_type(input_schema),
            Function::Constant(v) => v.return_type(input_schema),
            Function::Variable(v) => v.return_type(input_schema),
            Function::Arithmetic(v) => v.return_type(input_schema),
            Function::Comparison(v) => v.return_type(input_schema),
            Function::Logic(v) => v.return_type(input_schema),
            Function::Aggregator(v) => v.return_type(),
        }
    }

    pub fn nullable(&self, input_schema: &DataSchema) -> FuseQueryResult<bool> {
        match self {
            Function::Alias(v) => v.nullable(input_schema),
            Function::Constant(v) => v.nullable(input_schema),
            Function::Variable(v) => v.nullable(input_schema),
            Function::Arithmetic(v) => v.nullable(input_schema),
            Function::Comparison(v) => v.nullable(input_schema),
            Function::Logic(v) => v.nullable(input_schema),
            Function::Aggregator(v) => v.nullable(input_schema),
        }
    }

    pub fn eval(&mut self, block: &DataBlock) -> FuseQueryResult<()> {
        match self {
            Function::Alias(v) => v.eval(block),
            Function::Constant(v) => v.eval(block),
            Function::Variable(v) => v.eval(block),
            Function::Arithmetic(v) => v.eval(block),
            Function::Comparison(v) => v.eval(block),
            Function::Logic(v) => v.eval(block),
            Function::Aggregator(v) => v.eval(block),
        }
    }

    pub fn result(&self) -> FuseQueryResult<DataColumnarValue> {
        match self {
            Function::Alias(v) => v.result(),
            Function::Constant(v) => v.result(),
            Function::Variable(v) => v.result(),
            Function::Arithmetic(v) => v.result(),
            Function::Comparison(v) => v.result(),
            Function::Logic(v) => v.result(),
            Function::Aggregator(v) => v.result(),
        }
    }
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Function::Alias(v) => write!(f, "{}", v),
            Function::Constant(v) => write!(f, "{}", v),
            Function::Variable(v) => write!(f, "{}", v),
            Function::Arithmetic(v) => write!(f, "{}", v),
            Function::Comparison(v) => write!(f, "{}", v),
            Function::Logic(v) => write!(f, "{}", v),
            Function::Aggregator(v) => write!(f, "{}", v),
        }
    }
}
