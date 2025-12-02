//! Nom parser combinators for the Beamfile DSL.

use std::collections::HashMap;

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_while, take_while1},
    character::complete::{char, digit1, multispace1, none_of},
    combinator::{eof, map, map_res, opt, recognize, value},
    multi::{many0, separated_list0},
    sequence::{delimited, pair},
};

use crate::ast::*;
use crate::lexer::Span;

// ============================================================================
// Utility combinators
// ============================================================================

/// Parses whitespace and comments.
fn ws(input: Span) -> IResult<Span, ()> {
    value((), many0(alt((value((), multispace1), value((), comment))))).parse(input)
}

/// Parses a comment (# until end of line).
fn comment(input: Span) -> IResult<Span, Span> {
    recognize(pair(char('#'), take_while(|c| c != '\n'))).parse(input)
}

/// Wraps a parser with whitespace handling.
fn ws_wrap<'a, F, O>(mut inner: F) -> impl FnMut(Span<'a>) -> IResult<Span<'a>, O>
where
    F: Parser<Span<'a>, Output = O, Error = nom::error::Error<Span<'a>>>,
{
    move |input| {
        let (input, _) = ws(input)?;
        let (input, result) = inner.parse(input)?;
        let (input, _) = ws(input)?;
        Ok((input, result))
    }
}

// ============================================================================
// Basic value parsers
// ============================================================================

/// Parses an identifier: [a-zA-Z_][a-zA-Z0-9_]*
pub fn identifier(input: Span) -> IResult<Span, String> {
    map(
        recognize(pair(
            take_while1(|c: char| c.is_alphabetic() || c == '_'),
            take_while(|c: char| c.is_alphanumeric() || c == '_'),
        )),
        |s: Span| s.fragment().to_string(),
    )
    .parse(input)
}

/// Parses a string literal: "..."
pub fn string_literal(input: Span) -> IResult<Span, String> {
    delimited(
        char('"'),
        map(
            opt(escaped_transform(
                none_of("\\\""),
                '\\',
                alt((
                    value('\\', char('\\')),
                    value('"', char('"')),
                    value('\n', char('n')),
                    value('\r', char('r')),
                    value('\t', char('t')),
                )),
            )),
            |s| s.unwrap_or_default(),
        ),
        char('"'),
    )
    .parse(input)
}

/// Parses a number literal.
pub fn number_literal(input: Span) -> IResult<Span, i64> {
    map_res(recognize(pair(opt(char('-')), digit1)), |s: Span| {
        s.fragment().parse::<i64>()
    })
    .parse(input)
}

/// Parses a boolean literal.
pub fn bool_literal(input: Span) -> IResult<Span, bool> {
    alt((value(true, tag("true")), value(false, tag("false")))).parse(input)
}

/// Parses any value (string, number, bool, array, or block).
pub fn ast_value(input: Span) -> IResult<Span, AstValue> {
    alt((
        map(bool_literal, AstValue::Bool),
        map(number_literal, AstValue::Number),
        map(string_literal, AstValue::String),
        map(array_value, AstValue::Array),
        map(block_value, AstValue::Block),
    ))
    .parse(input)
}

/// Parses an array: [value, value, ...]
fn array_value(input: Span) -> IResult<Span, Vec<AstValue>> {
    let (input, _) = char('[')(input)?;
    let (input, _) = ws(input)?;
    let (input, items) = separated_list0(
        |i| {
            let (i, _) = ws(i)?;
            let (i, _) = opt(char(',')).parse(i)?;
            let (i, _) = ws(i)?;
            Ok((i, ()))
        },
        ast_value,
    )
    .parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = opt(char(',')).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(']')(input)?;
    Ok((input, items))
}

/// Parses a block: { key = value, ... }
fn block_value(input: Span) -> IResult<Span, HashMap<String, AstValue>> {
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, pairs) = many0(ws_wrap(key_value_pair)).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;
    Ok((input, pairs.into_iter().collect()))
}

/// Parses key = value.
fn key_value_pair(input: Span) -> IResult<Span, (String, AstValue)> {
    let (input, key) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    let (input, value) = ast_value(input)?;
    Ok((input, (key, value)))
}

// ============================================================================
// String array parser (common pattern)
// ============================================================================

/// Parses an array of strings: ["a", "b", "c"]
pub fn string_array(input: Span) -> IResult<Span, Vec<String>> {
    let (input, _) = char('[')(input)?;
    let (input, _) = ws(input)?;
    let (input, items) = separated_list0(
        |i| {
            let (i, _) = ws(i)?;
            let (i, _) = opt(char(',')).parse(i)?;
            let (i, _) = ws(i)?;
            Ok((i, ()))
        },
        string_literal,
    )
    .parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = opt(char(',')).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(']')(input)?;
    Ok((input, items))
}

// ============================================================================
// Variable parser
// ============================================================================

/// Parses a variable block: variable "name" { ... }
pub fn variable_block(input: Span) -> IResult<Span, AstVariable> {
    let (input, _) = tag("variable")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = string_literal(input)?;
    let (input, _) = ws(input)?;
    let (input, body) = block_value(input)?;

    Ok((input, AstVariable { name, body }))
}

// ============================================================================
// Beam parser
// ============================================================================

/// Parses a beam block: beam "name" { ... }
pub fn beam_block(input: Span) -> IResult<Span, AstBeam> {
    let (input, _) = tag("beam")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = string_literal(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, body) = many0(ws_wrap(beam_item)).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;

    Ok((input, AstBeam { name, body }))
}

/// Parses a single item within a beam block.
fn beam_item(input: Span) -> IResult<Span, AstBeamItem> {
    alt((
        map(description_field, AstBeamItem::Description),
        map(depends_on_field, AstBeamItem::DependsOn),
        map(condition_block, AstBeamItem::Condition),
        map(env_block, AstBeamItem::Env),
        map(pre_hook_block, AstBeamItem::PreHook),
        map(run_block, AstBeamItem::Run),
        map(post_hook_block, AstBeamItem::PostHook),
        map(inputs_field, AstBeamItem::Inputs),
        map(outputs_field, AstBeamItem::Outputs),
    ))
    .parse(input)
}

/// Parses: description = "..."
fn description_field(input: Span) -> IResult<Span, String> {
    let (input, _) = tag("description")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    string_literal(input)
}

/// Parses: depends_on = ["a", "b"]
fn depends_on_field(input: Span) -> IResult<Span, Vec<String>> {
    let (input, _) = tag("depends_on")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    string_array(input)
}

/// Parses: inputs = ["file1", "file2"]
fn inputs_field(input: Span) -> IResult<Span, Vec<String>> {
    let (input, _) = tag("inputs")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    string_array(input)
}

/// Parses: outputs = ["file1", "file2"]
fn outputs_field(input: Span) -> IResult<Span, Vec<String>> {
    let (input, _) = tag("outputs")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    string_array(input)
}

/// Parses: env { KEY = "value" }
fn env_block(input: Span) -> IResult<Span, HashMap<String, String>> {
    let (input, _) = tag("env")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, pairs) = many0(ws_wrap(env_pair)).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;

    Ok((input, pairs.into_iter().collect()))
}

/// Parses: KEY = "value"
fn env_pair(input: Span) -> IResult<Span, (String, String)> {
    let (input, key) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    let (input, value) = string_literal(input)?;
    Ok((input, (key, value)))
}

// ============================================================================
// Condition parser
// ============================================================================

/// Parses: condition { ... }
fn condition_block(input: Span) -> IResult<Span, AstCondition> {
    let (input, _) = tag("condition")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, condition) = condition_inner(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;

    Ok((input, condition))
}

/// Parses the inner condition.
fn condition_inner(input: Span) -> IResult<Span, AstCondition> {
    alt((
        condition_file_exists,
        condition_env_set,
        condition_env_equals,
    ))
    .parse(input)
}

/// Parses: file_exists = "path"
fn condition_file_exists(input: Span) -> IResult<Span, AstCondition> {
    let (input, _) = tag("file_exists")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    let (input, path) = string_literal(input)?;
    Ok((input, AstCondition::FileExists(path)))
}

/// Parses: env_set = "VAR"
fn condition_env_set(input: Span) -> IResult<Span, AstCondition> {
    let (input, _) = tag("env_set")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = string_literal(input)?;
    Ok((input, AstCondition::EnvSet(name)))
}

/// Parses: env_equals { name = "VAR", value = "val" }
fn condition_env_equals(input: Span) -> IResult<Span, AstCondition> {
    let (input, _) = tag("env_equals")(input)?;
    let (input, _) = ws(input)?;
    let (input, block) = block_value(input)?;

    let name = block
        .get("name")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string();
    let value = block
        .get("value")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string();

    Ok((input, AstCondition::EnvEquals { name, value }))
}

// ============================================================================
// Hook parsers
// ============================================================================

/// Parses: pre_hook { commands = [...] }
fn pre_hook_block(input: Span) -> IResult<Span, AstHook> {
    let (input, _) = tag("pre_hook")(input)?;
    let (input, _) = ws(input)?;
    hook_body(input)
}

/// Parses: post_hook { commands = [...] }
fn post_hook_block(input: Span) -> IResult<Span, AstHook> {
    let (input, _) = tag("post_hook")(input)?;
    let (input, _) = ws(input)?;
    hook_body(input)
}

/// Parses the body of a hook block.
fn hook_body(input: Span) -> IResult<Span, AstHook> {
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, fields) = many0(ws_wrap(key_value_pair)).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;

    let mut hook = AstHook {
        commands: Vec::new(),
        shell: None,
        working_dir: None,
        fail_on_error: None,
    };

    for (key, value) in fields {
        match key.as_str() {
            "commands" => {
                if let AstValue::Array(arr) = value {
                    hook.commands = arr
                        .into_iter()
                        .filter_map(|v| {
                            if let AstValue::String(s) = v {
                                Some(s)
                            } else {
                                None
                            }
                        })
                        .collect();
                }
            }
            "shell" => hook.shell = value.as_string().map(|s| s.to_string()),
            "working_dir" => hook.working_dir = value.as_string().map(|s| s.to_string()),
            "fail_on_error" => hook.fail_on_error = value.as_bool(),
            _ => {}
        }
    }

    Ok((input, hook))
}

// ============================================================================
// Run block parser
// ============================================================================

/// Parses: run { commands = [...] }
fn run_block(input: Span) -> IResult<Span, AstRun> {
    let (input, _) = tag("run")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, fields) = many0(ws_wrap(key_value_pair)).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;

    let mut run = AstRun {
        commands: Vec::new(),
        shell: None,
        working_dir: None,
        fail_fast: None,
    };

    for (key, value) in fields {
        match key.as_str() {
            "commands" => {
                if let AstValue::Array(arr) = value {
                    run.commands = arr
                        .into_iter()
                        .filter_map(|v| {
                            if let AstValue::String(s) = v {
                                Some(s)
                            } else {
                                None
                            }
                        })
                        .collect();
                }
            }
            "shell" => run.shell = value.as_string().map(|s| s.to_string()),
            "working_dir" => run.working_dir = value.as_string().map(|s| s.to_string()),
            "fail_fast" => run.fail_fast = value.as_bool(),
            _ => {}
        }
    }

    Ok((input, run))
}

// ============================================================================
// Default beam parser
// ============================================================================

/// Parses: default = "beam_name"
pub fn default_beam(input: Span) -> IResult<Span, String> {
    let (input, _) = tag("default")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = ws(input)?;
    string_literal(input)
}

// ============================================================================
// Root parser
// ============================================================================

/// Parses a complete Beamfile.
pub fn beamfile(input: Span) -> IResult<Span, AstBeamfile> {
    let (input, _) = ws(input)?;
    let (input, items) = many0(ws_wrap(beamfile_item)).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = eof(input)?;

    Ok((input, AstBeamfile { items }))
}

/// Parses a top-level item in a Beamfile.
fn beamfile_item(input: Span) -> IResult<Span, AstItem> {
    alt((
        map(variable_block, AstItem::Variable),
        map(beam_block, AstItem::Beam),
        map(default_beam, AstItem::Default),
    ))
    .parse(input)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn span(s: &str) -> Span<'_> {
        Span::new(s)
    }

    #[test]
    fn test_identifier() {
        let (_, result) = identifier(span("hello_world")).unwrap();
        assert_eq!(result, "hello_world");
    }

    #[test]
    fn test_string_literal() {
        let (_, result) = string_literal(span(r#""hello world""#)).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_string_literal_escaped() {
        let (_, result) = string_literal(span(r#""hello\nworld""#)).unwrap();
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn test_number_literal() {
        let (_, result) = number_literal(span("42")).unwrap();
        assert_eq!(result, 42);

        let (_, result) = number_literal(span("-10")).unwrap();
        assert_eq!(result, -10);
    }

    #[test]
    fn test_bool_literal() {
        let (_, result) = bool_literal(span("true")).unwrap();
        assert!(result);

        let (_, result) = bool_literal(span("false")).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_string_array() {
        let (_, result) = string_array(span(r#"["a", "b", "c"]"#)).unwrap();
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_variable_block() {
        let input = r#"variable "build_mode" {
            default = "release"
            description = "Build mode"
        }"#;
        let (_, result) = variable_block(span(input)).unwrap();
        assert_eq!(result.name, "build_mode");
    }

    #[test]
    fn test_beam_block() {
        let input = r#"beam "build" {
            description = "Build the project"
            depends_on = ["clean"]
            run {
                commands = ["cargo build"]
            }
        }"#;
        let (_, result) = beam_block(span(input)).unwrap();
        assert_eq!(result.name, "build");
        assert_eq!(result.body.len(), 3);
    }

    #[test]
    fn test_beamfile() {
        let input = r#"
            # This is a comment
            variable "mode" {
                default = "debug"
            }

            beam "build" {
                run {
                    commands = ["cargo build"]
                }
            }

            default = "build"
        "#;
        let (_, result) = beamfile(span(input)).unwrap();
        assert_eq!(result.items.len(), 3);
    }
}
