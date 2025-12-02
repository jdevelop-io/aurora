//! Parser error types.

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// Error type for parsing failures.
#[derive(Debug, Error, Diagnostic)]
#[error("Parse error: {message}")]
#[diagnostic(code(aurora::parser::error))]
pub struct ParseError {
    pub message: String,

    #[source_code]
    pub src: String,

    #[label("here")]
    pub span: Option<SourceSpan>,
}

impl ParseError {
    pub fn new(message: impl Into<String>, src: &str, offset: usize) -> Self {
        Self {
            message: message.into(),
            src: src.to_string(),
            span: Some(SourceSpan::from(offset..offset + 1)),
        }
    }

    pub fn eof(src: &str) -> Self {
        Self {
            message: "Unexpected end of file".to_string(),
            src: src.to_string(),
            span: Some(SourceSpan::from(src.len().saturating_sub(1)..src.len())),
        }
    }
}
