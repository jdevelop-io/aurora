//! Abstract Syntax Tree types.

#![allow(dead_code)]

use std::collections::HashMap;

/// Root AST node representing a Beamfile.
#[derive(Debug, Clone)]
pub struct AstBeamfile {
    pub items: Vec<AstItem>,
}

/// Top-level items in a Beamfile.
#[derive(Debug, Clone)]
pub enum AstItem {
    Variable(AstVariable),
    Beam(AstBeam),
    Default(String),
}

/// Variable definition.
#[derive(Debug, Clone)]
pub struct AstVariable {
    pub name: String,
    pub body: HashMap<String, AstValue>,
}

/// Beam (target) definition.
#[derive(Debug, Clone)]
pub struct AstBeam {
    pub name: String,
    pub body: Vec<AstBeamItem>,
}

/// Items within a beam block.
#[derive(Debug, Clone)]
pub enum AstBeamItem {
    Description(String),
    DependsOn(Vec<String>),
    Condition(AstCondition),
    Env(HashMap<String, String>),
    PreHook(AstHook),
    Run(AstRun),
    PostHook(AstHook),
    Inputs(Vec<String>),
    Outputs(Vec<String>),
}

/// Condition block.
#[derive(Debug, Clone)]
pub enum AstCondition {
    FileExists(String),
    EnvSet(String),
    EnvEquals { name: String, value: String },
    Command { run: String, expect_success: bool },
    And(Vec<AstCondition>),
    Or(Vec<AstCondition>),
    Not(Box<AstCondition>),
}

/// Hook block (pre_hook or post_hook).
#[derive(Debug, Clone)]
pub struct AstHook {
    pub commands: Vec<String>,
    pub shell: Option<String>,
    pub working_dir: Option<String>,
    pub fail_on_error: Option<bool>,
}

/// Run block.
#[derive(Debug, Clone)]
pub struct AstRun {
    pub commands: Vec<String>,
    pub shell: Option<String>,
    pub working_dir: Option<String>,
    pub fail_fast: Option<bool>,
}

/// Generic AST value.
#[derive(Debug, Clone)]
pub enum AstValue {
    String(String),
    Number(i64),
    Bool(bool),
    Array(Vec<AstValue>),
    Block(HashMap<String, AstValue>),
}

impl AstValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            AstValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            AstValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[AstValue]> {
        match self {
            AstValue::Array(arr) => Some(arr),
            _ => None,
        }
    }
}
