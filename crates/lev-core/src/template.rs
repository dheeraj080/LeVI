// SPDX-License-Identifier: MIT OR Apache-2.0
//! Templates: parse once, render many times.
//!
//! A template is a string with embedded expressions:
//!
//! - `{{ name }}` — a variable from the active scope
//! - `{{$uuid}}` — a dynamic function (note the leading `$`)
//!
//! Parsing happens once (and is cached by callers); rendering is a linear walk.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{CoreError, Result};

const OPEN: &str = "{{";
const CLOSE: &str = "}}";

/// A context that can resolve variables and dynamic functions for a template.
pub trait RenderContext {
    fn var(&self, name: &str) -> Result<String>;
    fn func(&mut self, name: &str) -> Result<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Var(String),
    Func(String),
}

/// A parsed template. Cheap to clone, and safe to render concurrently.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    source: String,
    segments: Vec<Segment>,
}

impl Template {
    /// Parse a template, reporting the byte offset of the first syntax error.
    pub fn parse(source: &str) -> Result<Self> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut literal = String::new();
        let mut rest = source;

        while let Some(start) = rest.find(OPEN) {
            literal.push_str(&rest[..start]);
            let after_open = &rest[start + OPEN.len()..];
            let Some(end) = after_open.find(CLOSE) else {
                return Err(CoreError::TemplateParse {
                    pos: start,
                    msg: format!("unclosed `{OPEN}`"),
                });
            };

            let inner = after_open[..end].trim();
            if inner.is_empty() {
                return Err(CoreError::TemplateParse {
                    pos: start,
                    msg: "empty expression".to_string(),
                });
            }

            let segment = match inner.strip_prefix('$') {
                Some(func) => {
                    let func = func.trim();
                    if func.is_empty() {
                        return Err(CoreError::TemplateParse {
                            pos: start,
                            msg: "empty function name after `$`".to_string(),
                        });
                    }
                    Segment::Func(func.to_string())
                }
                None => Segment::Var(inner.to_string()),
            };

            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(segment);
            rest = &after_open[end + CLOSE.len()..];
        }

        literal.push_str(rest);
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Self {
            source: source.to_string(),
            segments,
        })
    }

    /// A template that is plain text (no expressions).
    pub fn literal(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            segments: vec![Segment::Literal(value.clone())],
            source: value,
        }
    }

    /// The original source text.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// True when the template contains no expressions.
    pub fn is_literal(&self) -> bool {
        self.segments
            .iter()
            .all(|segment| matches!(segment, Segment::Literal(_)))
    }

    /// Render against a context. Missing variables and unknown functions are errors,
    /// never silently dropped.
    pub fn render(&self, ctx: &mut dyn RenderContext) -> Result<String> {
        let mut out = String::with_capacity(self.source.len());
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Var(name) => out.push_str(&ctx.var(name)?),
                Segment::Func(name) => out.push_str(&ctx.func(name)?),
            }
        }
        Ok(out)
    }
}

/// The built-in dynamic functions: `{{$uuid}}`, `{{$timestamp}}`, `{{$randomInt}}`.
///
/// Backed by a small seeded PRNG so runs are reproducible (`--seed`).
#[derive(Debug, Clone)]
pub struct DynamicFunctions {
    state: u64,
}

impl DynamicFunctions {
    pub fn new(seed: u64) -> Self {
        // Mix the seed (splitmix64) so low seeds don't produce low-entropy output.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self {
            state: if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Resolve a dynamic function by name (without the leading `$`).
    pub fn call(&mut self, name: &str) -> Option<String> {
        match name {
            "uuid" => Some(self.uuid()),
            "timestamp" => Some(now_unix_millis().to_string()),
            "randomInt" => Some((self.next_u64() % 1_000_000).to_string()),
            _ => None,
        }
    }

    fn uuid(&mut self) -> String {
        let hi = self.next_u64();
        let lo = self.next_u64();
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&hi.to_be_bytes());
        bytes[8..].copy_from_slice(&lo.to_be_bytes());
        // Set the version (4) and variant (RFC 4122) bits.
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        )
    }
}

fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct MapCtx {
        vars: BTreeMap<String, String>,
        dynamic: DynamicFunctions,
    }

    impl MapCtx {
        fn new() -> Self {
            Self {
                vars: BTreeMap::new(),
                dynamic: DynamicFunctions::new(42),
            }
        }
    }

    impl RenderContext for MapCtx {
        fn var(&self, name: &str) -> Result<String> {
            self.vars
                .get(name)
                .cloned()
                .ok_or_else(|| CoreError::UnresolvedVariable(name.to_string()))
        }

        fn func(&mut self, name: &str) -> Result<String> {
            self.dynamic
                .call(name)
                .ok_or_else(|| CoreError::UnknownFunction(name.to_string()))
        }
    }

    #[test]
    fn literal_only_template() {
        let template = Template::parse("https://example.com/health").unwrap();
        assert!(template.is_literal());
        let mut ctx = MapCtx::new();
        assert_eq!(
            template.render(&mut ctx).unwrap(),
            "https://example.com/health"
        );
    }

    #[test]
    fn renders_variables_and_trims_whitespace() {
        let template = Template::parse("Hello {{ name }}!").unwrap();
        let mut ctx = MapCtx::new();
        ctx.vars.insert("name".into(), "world".into());
        assert_eq!(template.render(&mut ctx).unwrap(), "Hello world!");
    }

    #[test]
    fn missing_variable_is_an_error() {
        let template = Template::parse("{{ nope }}").unwrap();
        let mut ctx = MapCtx::new();
        assert!(matches!(
            template.render(&mut ctx),
            Err(CoreError::UnresolvedVariable(_))
        ));
    }

    #[test]
    fn unclosed_expression_is_an_error() {
        assert!(matches!(
            Template::parse("a {{ b"),
            Err(CoreError::TemplateParse { .. })
        ));
    }

    #[test]
    fn unknown_function_is_an_error() {
        let template = Template::parse("{{$nope}}").unwrap();
        let mut ctx = MapCtx::new();
        assert!(matches!(
            template.render(&mut ctx),
            Err(CoreError::UnknownFunction(_))
        ));
    }

    #[test]
    fn dynamic_functions_are_seeded_and_repeatable() {
        let template = Template::parse("{{$uuid}} {{$randomInt}}").unwrap();
        let mut first_ctx = MapCtx::new();
        let first = template.render(&mut first_ctx).unwrap();
        let mut second_ctx = MapCtx::new();
        let second = template.render(&mut second_ctx).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn uuid_has_the_right_shape() {
        let mut functions = DynamicFunctions::new(7);
        let id = functions.call("uuid").unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[8], b'-');
        assert_eq!(id.as_bytes()[13], b'-');
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }
}
