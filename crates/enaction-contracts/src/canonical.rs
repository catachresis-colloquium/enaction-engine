// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canonical bytes: RFC 8785 JCS over integer-only I-JSON (ADR-0024; owner
//! ruling Q1, arm A).
//!
//! The profile is narrower than RFC 8785, on purpose:
//!
//! * **Integers only.** A number with a fraction or an exponent is refused
//!   as `NonIntegerNumber`, so no float can reach a hash, an identity or an
//!   ordering (ADR-0015). The CAC wire has no float field.
//! * **Bounded integers.** Every integer outside `±(2^53 − 1)` is refused as
//!   `IntegerOutOfRange`. There is no decimal-string escape: I-JSON integers
//!   beyond that bound round in JavaScript consumers, and an escape would give
//!   one value two spellings.
//! * **Strict I-JSON.** Duplicate object keys, lone surrogates, raw control
//!   characters, a byte-order mark, trailing input and every RFC 8259 grammar
//!   error are refused.
//!
//! Within that profile the output is exactly RFC 8785: object members sorted
//! by UTF-16 code units, arrays in order, minimal string escapes, no
//! insignificant whitespace, and integers in shortest decimal form (so `-0`
//! is `0`).

use std::fmt;

use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

/// The largest integer the wire admits: `2^53 − 1`.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// The maximum nesting depth the parser admits. Deep enough for every
/// contract document, shallow enough that hostile input cannot exhaust the
/// stack.
pub const MAX_DEPTH: usize = 128;

/// A parsed integer-only I-JSON value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Always within `±MAX_SAFE_INTEGER`.
    Int(i64),
    String(String),
    Array(Vec<Value>),
    /// Members in input order. Keys are unique; canonical output sorts them.
    Object(Vec<(String, Value)>),
}

/// Why a document is not canonicalisable, with the stable field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalFault {
    path: String,
    kind: FaultKind,
}

/// The kind of a [`CanonicalFault`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultKind {
    /// Not RFC 8259 JSON, or not I-JSON (lone surrogate, raw control, BOM).
    MalformedJson(String),
    /// An object names the same member twice.
    DuplicateKey(String),
    /// A number with a fraction or an exponent.
    NonIntegerNumber,
    /// An integer outside `±(2^53 − 1)`.
    IntegerOutOfRange,
    /// A member the target type does not declare.
    UnknownField(String),
    /// A document that is canonical but not the target type's shape: a
    /// missing member, a wrong type, a value outside its type's bound.
    BadShape(String),
}

impl CanonicalFault {
    fn new(path: impl Into<String>, kind: FaultKind) -> Self {
        CanonicalFault {
            path: path.into(),
            kind,
        }
    }

    /// The stable field path, e.g. `events[0].tick`. The root is `$`.
    #[must_use]
    pub fn path(&self) -> &str {
        if self.path.is_empty() {
            "$"
        } else {
            &self.path
        }
    }

    #[must_use]
    pub fn kind(&self) -> &FaultKind {
        &self.kind
    }

    /// The rule that failed, by stable name.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        match self.kind {
            FaultKind::MalformedJson(_) => "MalformedJson",
            FaultKind::DuplicateKey(_) => "DuplicateKey",
            FaultKind::NonIntegerNumber => "NonIntegerNumber",
            FaultKind::IntegerOutOfRange => "IntegerOutOfRange",
            FaultKind::UnknownField(_) => "UnknownField",
            FaultKind::BadShape(_) => "BadShape",
        }
    }
}

impl fmt::Display for CanonicalFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path(), self.rule())?;
        match &self.kind {
            FaultKind::MalformedJson(m) | FaultKind::BadShape(m) => write!(f, " ({m})"),
            FaultKind::DuplicateKey(k) | FaultKind::UnknownField(k) => write!(f, " ({k:?})"),
            FaultKind::NonIntegerNumber | FaultKind::IntegerOutOfRange => Ok(()),
        }
    }
}

impl std::error::Error for CanonicalFault {}

fn push_key(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn push_index(path: &str, index: usize) -> String {
    format!("{path}[{index}]")
}

// ---------------------------------------------------------------- parsing

/// Parse integer-only I-JSON strictly.
///
/// # Errors
///
/// The first fault found, with its field path.
pub fn parse(bytes: &[u8]) -> Result<Value, CanonicalFault> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| CanonicalFault::new("", FaultKind::MalformedJson(e.to_string())))?;
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.ws();
    let value = parser.value("", 0)?;
    parser.ws();
    if parser.at != parser.bytes.len() {
        return Err(parser.malformed("", "trailing input after the document"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn malformed(&self, path: &str, what: &str) -> CanonicalFault {
        CanonicalFault::new(
            path,
            FaultKind::MalformedJson(format!("{what} at byte {}", self.at)),
        )
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, path: &str, word: &str, value: Value) -> Result<Value, CanonicalFault> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.malformed(path, "unexpected token"))
        }
    }

    fn value(&mut self, path: &str, depth: usize) -> Result<Value, CanonicalFault> {
        if depth > MAX_DEPTH {
            return Err(self.malformed(path, "nesting deeper than MAX_DEPTH"));
        }
        match self.peek() {
            Some(b'{') => self.object(path, depth),
            Some(b'[') => self.array(path, depth),
            Some(b'"') => self.string(path).map(Value::String),
            Some(b't') => self.literal(path, "true", Value::Bool(true)),
            Some(b'f') => self.literal(path, "false", Value::Bool(false)),
            Some(b'n') => self.literal(path, "null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(path),
            Some(_) => Err(self.malformed(path, "unexpected character")),
            None => Err(self.malformed(path, "unexpected end of input")),
        }
    }

    fn object(&mut self, path: &str, depth: usize) -> Result<Value, CanonicalFault> {
        self.at += 1; // '{'
        let mut members: Vec<(String, Value)> = Vec::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Value::Object(members));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(self.malformed(path, "expected a member name"));
            }
            let key = self.string(path)?;
            let member_path = push_key(path, &key);
            if members.iter().any(|(k, _)| *k == key) {
                return Err(CanonicalFault::new(
                    member_path,
                    FaultKind::DuplicateKey(key),
                ));
            }
            self.ws();
            if !self.eat(b':') {
                return Err(self.malformed(&member_path, "expected ':'"));
            }
            self.ws();
            let value = self.value(&member_path, depth + 1)?;
            members.push((key, value));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value::Object(members));
            }
            return Err(self.malformed(path, "expected ',' or '}'"));
        }
    }

    fn array(&mut self, path: &str, depth: usize) -> Result<Value, CanonicalFault> {
        self.at += 1; // '['
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            self.ws();
            let item_path = push_index(path, items.len());
            items.push(self.value(&item_path, depth + 1)?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            return Err(self.malformed(path, "expected ',' or ']'"));
        }
    }

    fn hex4(&mut self, path: &str) -> Result<u16, CanonicalFault> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.malformed(path, "truncated \\u escape"))?;
        let text = std::str::from_utf8(digits).unwrap_or("");
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(self.malformed(path, "bad \\u escape"));
        }
        self.at += 4;
        u16::from_str_radix(text, 16).map_err(|_| self.malformed(path, "bad \\u escape"))
    }

    fn string(&mut self, path: &str) -> Result<String, CanonicalFault> {
        self.at += 1; // '"'
        let mut out = String::new();
        loop {
            let start = self.at;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.at += 1;
            }
            // The input is valid UTF-8 and we stopped on an ASCII byte, so
            // this slice is on character boundaries.
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default());
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.peek();
                    self.at += 1;
                    match escape {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let unit = self.hex4(path)?;
                            let scalar = match unit {
                                0xD800..=0xDBFF => {
                                    if !(self.eat(b'\\') && self.eat(b'u')) {
                                        return Err(self.malformed(path, "lone high surrogate"));
                                    }
                                    let low = self.hex4(path)?;
                                    if !(0xDC00..=0xDFFF).contains(&low) {
                                        return Err(self.malformed(path, "lone high surrogate"));
                                    }
                                    0x10000
                                        + ((u32::from(unit) - 0xD800) << 10)
                                        + (u32::from(low) - 0xDC00)
                                }
                                0xDC00..=0xDFFF => {
                                    return Err(self.malformed(path, "lone low surrogate"));
                                }
                                _ => u32::from(unit),
                            };
                            out.push(
                                char::from_u32(scalar)
                                    .ok_or_else(|| self.malformed(path, "bad scalar"))?,
                            );
                        }
                        _ => return Err(self.malformed(path, "bad escape")),
                    }
                }
                Some(_) => return Err(self.malformed(path, "raw control character in string")),
                None => return Err(self.malformed(path, "unterminated string")),
            }
        }
    }

    fn number(&mut self, path: &str) -> Result<Value, CanonicalFault> {
        let negative = self.eat(b'-');
        let int_start = self.at;
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(self.malformed(path, "bad number")),
        }
        let int_end = self.at;
        if matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err(self.malformed(path, "leading zero"));
        }
        let mut integral = true;
        if self.eat(b'.') {
            integral = false;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.malformed(path, "bad fraction"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            integral = false;
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.malformed(path, "bad exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if !integral {
            return Err(CanonicalFault::new(path, FaultKind::NonIntegerNumber));
        }
        let digits = std::str::from_utf8(&self.bytes[int_start..int_end]).unwrap_or_default();
        let magnitude = match digits.parse::<u64>() {
            Ok(m) if m <= MAX_SAFE_INTEGER => m,
            _ => return Err(CanonicalFault::new(path, FaultKind::IntegerOutOfRange)),
        };
        let magnitude = i64::try_from(magnitude).unwrap_or(i64::MAX);
        Ok(Value::Int(if negative { -magnitude } else { magnitude }))
    }
}

// --------------------------------------------------------------- emitting

/// The RFC 8785 bytes of a value.
#[must_use]
pub fn to_jcs(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    emit(value, &mut out);
    out.into_bytes()
}

fn emit(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::String(s) => emit_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                emit(item, out);
            }
            out.push(']');
        }
        Value::Object(members) => {
            let mut sorted: Vec<&(String, Value)> = members.iter().collect();
            sorted.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            out.push('{');
            for (i, (key, item)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                emit_string(key, out);
                out.push(':');
                emit(item, out);
            }
            out.push('}');
        }
    }
}

/// RFC 8785 section 3.2.2.2: ECMAScript `JSON.stringify` string escaping.
fn emit_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse and canonicalise in one step.
///
/// # Errors
///
/// Any fault [`parse`] reports.
pub fn canonicalize(bytes: &[u8]) -> Result<Vec<u8>, CanonicalFault> {
    parse(bytes).map(|v| to_jcs(&v))
}

// ------------------------------------------------------ typed conversion

/// A typed value as an integer-only I-JSON [`Value`].
///
/// # Errors
///
/// `IntegerOutOfRange` for an integer beyond the bound, `NonIntegerNumber`
/// for a float, `BadShape` for anything serde cannot represent.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Value, CanonicalFault> {
    let json = serde_json::to_value(value)
        .map_err(|e| CanonicalFault::new("", FaultKind::BadShape(e.to_string())))?;
    from_json(&json, "")
}

fn from_json(json: &serde_json::Value, path: &str) -> Result<Value, CanonicalFault> {
    Ok(match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                if u > MAX_SAFE_INTEGER {
                    return Err(CanonicalFault::new(path, FaultKind::IntegerOutOfRange));
                }
                Value::Int(i64::try_from(u).unwrap_or(i64::MAX))
            } else if let Some(i) = n.as_i64() {
                if i.unsigned_abs() > MAX_SAFE_INTEGER {
                    return Err(CanonicalFault::new(path, FaultKind::IntegerOutOfRange));
                }
                Value::Int(i)
            } else {
                return Err(CanonicalFault::new(path, FaultKind::NonIntegerNumber));
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| from_json(item, &push_index(path, i)))
                .collect::<Result<_, _>>()?,
        ),
        serde_json::Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), from_json(v, &push_key(path, k))?)))
                .collect::<Result<_, CanonicalFault>>()?,
        ),
    })
}

fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::from(*i),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
        Value::Object(members) => serde_json::Value::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
    }
}

/// The canonical bytes of a typed value.
///
/// # Errors
///
/// As [`to_value`].
pub fn canonical_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, CanonicalFault> {
    to_value(value).map(|v| to_jcs(&v))
}

/// Decode a typed value from bytes, strictly: the bytes must be
/// integer-only I-JSON (checked first, by [`parse`]), and then match the
/// type's shape exactly. Contract types deny unknown fields, so an unknown
/// member is refused as `UnknownField` with its path before anything is
/// canonicalised or hashed.
///
/// # Errors
///
/// The first fault, with its field path.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CanonicalFault> {
    decode_value(&parse(bytes)?)
}

/// As [`decode`], from an already-parsed value.
///
/// # Errors
///
/// The first shape fault, with its field path.
pub fn decode_value<T: DeserializeOwned>(value: &Value) -> Result<T, CanonicalFault> {
    serde_path_to_error::deserialize(to_json(value)).map_err(|e| {
        let mut path = String::new();
        for segment in e.path().iter() {
            match segment {
                serde_path_to_error::Segment::Seq { index } => {
                    path = push_index(&path, *index);
                }
                serde_path_to_error::Segment::Map { key } => path = push_key(&path, key),
                serde_path_to_error::Segment::Enum { variant } => {
                    path = push_key(&path, variant);
                }
                serde_path_to_error::Segment::Unknown => path = push_key(&path, "?"),
            }
        }
        let message = e.into_inner().to_string();
        // The path already ends at the unknown member's name.
        let kind = match unknown_field_name(&message) {
            Some(name) => FaultKind::UnknownField(name),
            None => FaultKind::BadShape(message),
        };
        CanonicalFault::new(path, kind)
    })
}

/// serde's derive reports an unknown member as "unknown field `NAME`, ...".
fn unknown_field_name(message: &str) -> Option<String> {
    let rest = message.strip_prefix("unknown field `")?;
    Some(rest[..rest.find('`')?].to_string())
}

/// Lowercase hex SHA-256 of some bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
