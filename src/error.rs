use thiserror::Error;

use crate::parser::Rule;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Pest(Box<pest::error::Error<Rule>>),

    #[error("unexpected: {0}")]
    Unexpected(String),

    #[error("invalid integer literal: {0}")]
    InvalidInt(String),

    #[error("invalid float literal: {0}")]
    InvalidFloat(String),
}

impl From<pest::error::Error<Rule>> for ParseError {
    fn from(value: pest::error::Error<Rule>) -> Self {
        ParseError::Pest(Box::new(value))
    }
}
