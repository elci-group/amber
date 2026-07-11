//! amber_toml — Minimal, dependency-free TOML subset parser/serializer.
//!
//! Supports the subset used by typical configuration files: tables, dotted
//! table headers, basic/literal strings, integers, floats, booleans, arrays,
//! and inline tables. It is intentionally small; for full TOML 1.0 semantics
//! (datetime, arrays-of-tables, multiline strings) use the upstream `toml`
//! crate.

use std::collections::HashMap;

/// A TOML value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<Value>),
    Table(HashMap<String, Value>),
}

impl Value {
    /// Get a value from a table by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Table(map) => map.get(key),
            _ => None,
        }
    }

    /// Mutably get a value from a table by key.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Self::Table(map) => map.get_mut(key),
            _ => None,
        }
    }

    /// Look up a value by a dotted path (e.g. `"policy.strict"`).
    #[must_use]
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let mut current = self;
        for segment in path.split('.') {
            current = current.get(segment)?;
        }
        Some(current)
    }

    /// Borrow as a string, if this is a `Value::String`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Borrow as an integer, if this is a `Value::Integer`.
    #[must_use]
    pub const fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Borrow as a bool, if this is a `Value::Boolean`.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

/// Error returned by [`parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse a TOML document into a [`Value`].
///
/// # Errors
/// Returns an error if the document contains a malformed key/value pair or an
/// unterminated string/array/table.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    let mut root: HashMap<String, Value> = HashMap::new();
    let mut current_path: Vec<String> = Vec::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let inner = &line[1..line.len() - 1];
            // Arrays of tables (`[[...]]`) are not supported; treat as a table.
            let inner = inner.trim_matches(['[', ']']).trim();
            if inner.is_empty() {
                return Err(ParseError::new(format!("empty table header on line {line_no}")));
            }
            current_path = parse_dotted_key(inner)?;
            ensure_table(&mut root, &current_path)?;
            continue;
        }

        let (key, value_text) = line.split_once('=').ok_or_else(|| {
            ParseError::new(format!("expected `key = value` on line {line_no}"))
        })?;
        let key = key.trim();
        if key.is_empty() {
            return Err(ParseError::new(format!("empty key on line {line_no}")));
        }
        let value = parse_value(value_text.trim())?;
        insert_value(&mut root, &current_path, key, value)?;
    }

    Ok(Value::Table(root))
}

/// Parse a TOML document from a string slice (alias for [`parse`]).
///
/// # Errors
/// Returns an error if the document is malformed.
pub fn from_str(input: &str) -> Result<Value, ParseError> {
    parse(input)
}

fn ensure_table(root: &mut HashMap<String, Value>, path: &[String]) -> Result<(), ParseError> {
    let mut map = root;
    for segment in path {
        map = match map
            .entry(segment.clone())
            .or_insert_with(|| Value::Table(HashMap::new()))
        {
            Value::Table(m) => m,
            _ => return Err(ParseError::new(format!("`{segment}` is not a table"))),
        };
    }
    Ok(())
}

fn insert_value(
    root: &mut HashMap<String, Value>,
    path: &[String],
    key: &str,
    value: Value,
) -> Result<(), ParseError> {
    let mut map = root;
    for segment in path {
        map = match map.get_mut(segment) {
            Some(Value::Table(m)) => m,
            _ => return Err(ParseError::new(format!("`{segment}` is not a table"))),
        };
    }
    map.insert(key.to_string(), value);
    Ok(())
}

fn parse_dotted_key(text: &str) -> Result<Vec<String>, ParseError> {
    let mut parts = Vec::new();
    for segment in text.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
            return Err(ParseError::new("empty table path segment"));
        }
        parts.push(parse_key(segment)?);
    }
    Ok(parts)
}

fn parse_key(text: &str) -> Result<String, ParseError> {
    if text.starts_with('"') || text.starts_with('\'') {
        let (s, rest) = parse_quoted(text)?;
        if !rest.trim().is_empty() {
            return Err(ParseError::new("unexpected characters after key"));
        }
        Ok(s)
    } else {
        Ok(text.to_string())
    }
}

fn parse_value(text: &str) -> Result<Value, ParseError> {
    if text.is_empty() {
        return Err(ParseError::new("empty value"));
    }
    let first = text.as_bytes()[0];
    match first {
        b'"' | b'\'' => {
            let (s, rest) = parse_quoted(text)?;
            if !rest.trim().is_empty() {
                return Err(ParseError::new("unexpected characters after string"));
            }
            Ok(Value::String(s))
        }
        b'[' => parse_array(text),
        b'{' => parse_inline_table(text),
        b't' | b'f' => match text {
            "true" => Ok(Value::Boolean(true)),
            "false" => Ok(Value::Boolean(false)),
            _ => Err(ParseError::new(format!("invalid value `{text}`"))),
        },
        _ => parse_number(text),
    }
}

fn parse_number(text: &str) -> Result<Value, ParseError> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() {
        return Err(ParseError::new("empty number"));
    }
    if cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E') {
        cleaned
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| ParseError::new(format!("invalid float `{text}`")))
    } else {
        cleaned
            .parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| ParseError::new(format!("invalid integer `{text}`")))
    }
}

fn parse_quoted(text: &str) -> Result<(String, &str), ParseError> {
    let quote = text.as_bytes()[0];
    let bytes = text.as_bytes();
    let mut i = 1;
    let mut out = String::new();
    while i < bytes.len() {
        let c = bytes[i];
        if c == quote {
            return Ok((out, &text[i + 1..]));
        }
        if quote == b'"' && c == b'\\' {
            i += 1;
            if i >= bytes.len() {
                return Err(ParseError::new("unterminated escape"));
            }
            let esc = match bytes[i] {
                b'n' => '\n',
                b't' => '\t',
                b'r' => '\r',
                b'"' => '"',
                b'\\' => '\\',
                b'0' => '\0',
                other => {
                    return Err(ParseError::new(format!(
                        "unsupported escape `\\{}`",
                        other as char
                    )))
                }
            };
            out.push(esc);
        } else {
            out.push(c as char);
        }
        i += 1;
    }
    Err(ParseError::new("unterminated string"))
}

fn parse_array(text: &str) -> Result<Value, ParseError> {
    let inner = text
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| ParseError::new("malformed array"))?;
    let mut items = Vec::new();
    for element in split_top_level(inner) {
        let element = element.trim();
        if element.is_empty() {
            continue;
        }
        items.push(parse_value(element)?);
    }
    Ok(Value::Array(items))
}

fn parse_inline_table(text: &str) -> Result<Value, ParseError> {
    let inner = text
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| ParseError::new("malformed inline table"))?;
    let mut map = HashMap::new();
    for pair in split_top_level(inner) {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (key, value_text) = pair
            .split_once('=')
            .ok_or_else(|| ParseError::new("expected `key = value` in inline table"))?;
        map.insert(parse_key(key.trim())?, parse_value(value_text.trim())?);
    }
    Ok(Value::Table(map))
}

/// Split a comma-separated list, ignoring commas inside strings, arrays, or
/// inline tables.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut in_string: Option<u8> = None;
    let mut escape = false;
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        if let Some(q) = in_string {
            if q == b'"' && c == b'\\' && !escape {
                escape = true;
                continue;
            }
            if c == q && !escape {
                in_string = None;
            }
            escape = false;
            continue;
        }
        match c {
            b'"' | b'\'' => in_string = Some(c),
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Strip a trailing `#` comment, respecting `#` inside quoted strings.
fn strip_comment(line: &str) -> &str {
    let mut in_string: Option<u8> = None;
    let mut escape = false;
    for (i, c) in line.bytes().enumerate() {
        if let Some(q) = in_string {
            if q == b'"' && c == b'\\' && !escape {
                escape = true;
                continue;
            }
            if c == q && !escape {
                in_string = None;
            }
            escape = false;
            continue;
        }
        match c {
            b'"' | b'\'' => in_string = Some(c),
            b'#' => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Serialize a [`Value`] to a TOML string.
#[must_use]
pub fn to_string(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, &[]);
    out
}

fn write_value(out: &mut String, value: &Value, path: &[String]) {
    match value {
        Value::Table(map) => {
            // Scalars/arrays first.
            for (k, v) in map {
                if !matches!(v, Value::Table(_)) {
                    out.push_str(k);
                    out.push_str(" = ");
                    write_scalar(out, v);
                    out.push('\n');
                }
            }
            // Then nested tables.
            for (k, v) in map {
                if let Value::Table(_) = v {
                    let mut new_path = path.to_vec();
                    new_path.push(k.clone());
                    out.push('\n');
                    out.push('[');
                    out.push_str(&new_path.join("."));
                    out.push_str("]\n");
                    write_value(out, v, &new_path);
                }
            }
        }
        other => write_scalar(out, other),
    }
}

fn write_scalar(out: &mut String, value: &Value) {
    match value {
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    other => out.push(other),
                }
            }
            out.push('"');
        }
        Value::Integer(i) => out.push_str(&i.to_string()),
        Value::Float(f) => out.push_str(&f.to_string()),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_scalar(out, item);
            }
            out.push(']');
        }
        Value::Table(_) => write_value(out, value, &[]),
    }
}
