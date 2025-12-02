//! Variable interpolation for Beamfile strings.
//!
//! Supports the following syntax:
//! - `${var.name}` - Reference a Beamfile variable
//! - `${env.NAME}` - Reference an environment variable
//! - `${beam.name}` - Reference the current beam name (in context)
//! - `$$` - Escaped literal `$`

use std::collections::HashMap;

use crate::error::{AuroraError, Result};

/// Context for variable interpolation.
#[derive(Debug, Clone, Default)]
pub struct InterpolationContext {
    /// Beamfile variables (var.name)
    variables: HashMap<String, String>,
    /// Current beam name (beam.name)
    beam_name: Option<String>,
    /// Additional context values
    extra: HashMap<String, String>,
}

impl InterpolationContext {
    /// Creates a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a variable to the context.
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(name.into(), value.into());
        self
    }

    /// Adds multiple variables to the context.
    pub fn with_variables(mut self, vars: HashMap<String, String>) -> Self {
        self.variables.extend(vars);
        self
    }

    /// Sets the current beam name.
    pub fn with_beam_name(mut self, name: impl Into<String>) -> Self {
        self.beam_name = Some(name.into());
        self
    }

    /// Adds an extra context value.
    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// Gets a variable value.
    pub fn get_variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|s| s.as_str())
    }

    /// Gets an environment variable value.
    pub fn get_env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// Gets the current beam name.
    pub fn get_beam_name(&self) -> Option<&str> {
        self.beam_name.as_deref()
    }

    /// Gets an extra context value.
    pub fn get_extra(&self, key: &str) -> Option<&str> {
        self.extra.get(key).map(|s| s.as_str())
    }
}

/// Interpolates variables in a string.
///
/// # Syntax
/// - `${var.name}` - Beamfile variable
/// - `${env.NAME}` - Environment variable
/// - `${beam.name}` - Current beam name
/// - `$$` - Literal `$`
///
/// # Examples
/// ```
/// use aurora_core::interpolation::{interpolate, InterpolationContext};
///
/// let ctx = InterpolationContext::new()
///     .with_variable("version", "1.0.0")
///     .with_beam_name("build");
///
/// let result = interpolate("Building v${var.version}", &ctx).unwrap();
/// assert_eq!(result, "Building v1.0.0");
/// ```
pub fn interpolate(input: &str, ctx: &InterpolationContext) -> Result<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('$') => {
                    // Escaped dollar sign
                    chars.next();
                    result.push('$');
                }
                Some('{') => {
                    // Variable reference
                    chars.next(); // consume '{'
                    let var_ref = parse_variable_ref(&mut chars)?;
                    let value = resolve_variable(&var_ref, ctx)?;
                    result.push_str(&value);
                }
                _ => {
                    // Just a dollar sign
                    result.push('$');
                }
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// Parses a variable reference from the input.
/// Expects the opening `{` to have already been consumed.
fn parse_variable_ref(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<VariableRef> {
    let mut name = String::new();

    while let Some(&c) = chars.peek() {
        if c == '}' {
            chars.next(); // consume '}'
            break;
        } else if c.is_alphanumeric() || c == '_' || c == '.' {
            name.push(c);
            chars.next();
        } else {
            return Err(AuroraError::Interpolation {
                message: format!("Invalid character '{}' in variable reference", c),
            });
        }
    }

    if name.is_empty() {
        return Err(AuroraError::Interpolation {
            message: "Empty variable reference".to_string(),
        });
    }

    // Parse the variable reference type
    if let Some(var_name) = name.strip_prefix("var.") {
        Ok(VariableRef::Variable(var_name.to_string()))
    } else if let Some(env_name) = name.strip_prefix("env.") {
        Ok(VariableRef::Environment(env_name.to_string()))
    } else if name == "beam.name" {
        Ok(VariableRef::BeamName)
    } else if let Some(extra_key) = name.strip_prefix("ctx.") {
        Ok(VariableRef::Extra(extra_key.to_string()))
    } else {
        // Assume it's a shorthand for var.name
        Ok(VariableRef::Variable(name))
    }
}

/// Resolves a variable reference to its value.
fn resolve_variable(var_ref: &VariableRef, ctx: &InterpolationContext) -> Result<String> {
    match var_ref {
        VariableRef::Variable(name) => {
            ctx.get_variable(name)
                .map(|s| s.to_string())
                .ok_or_else(|| AuroraError::Interpolation {
                    message: format!("Undefined variable: {}", name),
                })
        }

        VariableRef::Environment(name) => {
            ctx.get_env(name).ok_or_else(|| AuroraError::Interpolation {
                message: format!("Undefined environment variable: {}", name),
            })
        }

        VariableRef::BeamName => {
            ctx.get_beam_name()
                .map(|s| s.to_string())
                .ok_or_else(|| AuroraError::Interpolation {
                    message: "Beam name not available in this context".to_string(),
                })
        }

        VariableRef::Extra(key) => {
            ctx.get_extra(key)
                .map(|s| s.to_string())
                .ok_or_else(|| AuroraError::Interpolation {
                    message: format!("Undefined context key: {}", key),
                })
        }
    }
}

/// Types of variable references.
#[derive(Debug, Clone, PartialEq)]
enum VariableRef {
    /// Reference to a Beamfile variable: ${var.name}
    Variable(String),
    /// Reference to an environment variable: ${env.NAME}
    Environment(String),
    /// Reference to the current beam name: ${beam.name}
    BeamName,
    /// Reference to extra context: ${ctx.key}
    Extra(String),
}

/// Interpolates all strings in a HashMap.
pub fn interpolate_map(
    map: &HashMap<String, String>,
    ctx: &InterpolationContext,
) -> Result<HashMap<String, String>> {
    let mut result = HashMap::with_capacity(map.len());
    for (key, value) in map {
        result.insert(key.clone(), interpolate(value, ctx)?);
    }
    Ok(result)
}

/// Interpolates all strings in a Vec.
pub fn interpolate_vec(vec: &[String], ctx: &InterpolationContext) -> Result<Vec<String>> {
    vec.iter().map(|s| interpolate(s, ctx)).collect()
}

/// Checks if a string contains any variable references.
pub fn contains_variables(input: &str) -> bool {
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            if let Some('{') = chars.peek() {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_variable() {
        let ctx = InterpolationContext::new().with_variable("name", "world");
        let result = interpolate("Hello, ${var.name}!", &ctx).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_shorthand_variable() {
        let ctx = InterpolationContext::new().with_variable("name", "world");
        let result = interpolate("Hello, ${name}!", &ctx).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_multiple_variables() {
        let ctx = InterpolationContext::new()
            .with_variable("first", "Hello")
            .with_variable("second", "World");
        let result = interpolate("${var.first}, ${var.second}!", &ctx).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_environment_variable() {
        // SAFETY: This is a test, we control the environment
        unsafe {
            std::env::set_var("AURORA_TEST_VAR", "test_value");
        }
        let ctx = InterpolationContext::new();
        let result = interpolate("Value: ${env.AURORA_TEST_VAR}", &ctx).unwrap();
        assert_eq!(result, "Value: test_value");
        // SAFETY: This is a test, we control the environment
        unsafe {
            std::env::remove_var("AURORA_TEST_VAR");
        }
    }

    #[test]
    fn test_beam_name() {
        let ctx = InterpolationContext::new().with_beam_name("build");
        let result = interpolate("Running ${beam.name}", &ctx).unwrap();
        assert_eq!(result, "Running build");
    }

    #[test]
    fn test_escaped_dollar() {
        let ctx = InterpolationContext::new();
        let result = interpolate("Price: $$100", &ctx).unwrap();
        assert_eq!(result, "Price: $100");
    }

    #[test]
    fn test_no_interpolation() {
        let ctx = InterpolationContext::new();
        let result = interpolate("No variables here", &ctx).unwrap();
        assert_eq!(result, "No variables here");
    }

    #[test]
    fn test_undefined_variable() {
        let ctx = InterpolationContext::new();
        let result = interpolate("${var.undefined}", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_undefined_env_variable() {
        let ctx = InterpolationContext::new();
        let result = interpolate("${env.AURORA_DEFINITELY_NOT_SET_12345}", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_contains_variables() {
        assert!(contains_variables("${var.name}"));
        assert!(contains_variables("prefix ${var.name} suffix"));
        assert!(!contains_variables("no variables"));
        assert!(!contains_variables("just a $ sign"));
        assert!(!contains_variables("$$escaped"));
    }

    #[test]
    fn test_interpolate_vec() {
        let ctx = InterpolationContext::new().with_variable("name", "test");
        let vec = vec!["${var.name}".to_string(), "static".to_string()];
        let result = interpolate_vec(&vec, &ctx).unwrap();
        assert_eq!(result, vec!["test".to_string(), "static".to_string()]);
    }

    #[test]
    fn test_interpolate_map() {
        let ctx = InterpolationContext::new().with_variable("val", "replaced");
        let mut map = HashMap::new();
        map.insert("key".to_string(), "${var.val}".to_string());
        let result = interpolate_map(&map, &ctx).unwrap();
        assert_eq!(result.get("key").unwrap(), "replaced");
    }

    #[test]
    fn test_extra_context() {
        let ctx = InterpolationContext::new().with_extra("custom", "value");
        let result = interpolate("${ctx.custom}", &ctx).unwrap();
        assert_eq!(result, "value");
    }

    #[test]
    fn test_mixed_interpolation() {
        // SAFETY: This is a test, we control the environment
        unsafe {
            std::env::set_var("AURORA_MIX_TEST", "env_val");
        }
        let ctx = InterpolationContext::new()
            .with_variable("var_val", "beamfile_val")
            .with_beam_name("my_beam");

        let result = interpolate(
            "var=${var.var_val}, env=${env.AURORA_MIX_TEST}, beam=${beam.name}",
            &ctx,
        )
        .unwrap();

        assert_eq!(result, "var=beamfile_val, env=env_val, beam=my_beam");
        // SAFETY: This is a test, we control the environment
        unsafe {
            std::env::remove_var("AURORA_MIX_TEST");
        }
    }
}
