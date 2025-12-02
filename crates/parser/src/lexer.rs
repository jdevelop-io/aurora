//! Lexer/tokenizer types.

#![allow(dead_code)]

use nom_locate::LocatedSpan;

/// Input type with position tracking.
pub type Span<'a> = LocatedSpan<&'a str>;

/// Creates a new span from a string slice.
pub fn span(input: &str) -> Span<'_> {
    Span::new(input)
}

/// Token types for the Beamfile DSL.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Beam,
    Variable,
    Condition,
    Env,
    PreHook,
    PostHook,
    Run,

    // Literals
    Identifier(String),
    String(String),
    Number(i64),
    Bool(bool),

    // Operators and delimiters
    Equals,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    Comma,

    // Special
    Comment(String),
    Eof,
}
