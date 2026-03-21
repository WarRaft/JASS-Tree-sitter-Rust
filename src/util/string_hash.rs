//! Blizzard `StringHash` implementation (Bob Jenkins' lookup2 hash).
//!
//! This is a faithful Rust port of the C `SStrHash2` function used by the
//! Warcraft III engine for the `StringHash` JASS native.  It is used at
//! build time to fold `StringHash(expr)` calls into integer constants when
//! the argument can be fully evaluated at compile time.

use std::collections::HashMap;

// ─── Jenkins hash ────────────────────────────────────────────────────────────

/// Bob Jenkins' lookup2 `hash` — the core mixing function.
fn jenkins_hash(k: &[u8], initval: u32) -> u32 {
    let length = k.len() as u32;
    let mut a: u32 = 0x9e3779b9;
    let mut b: u32 = 0x9e3779b9;
    let mut c: u32 = initval;

    let mut i = 0usize;
    let mut len = k.len();

    // Handle most of the key (12-byte chunks).
    while len >= 12 {
        a = a.wrapping_add(
            k[i] as u32
                | (k[i + 1] as u32) << 8
                | (k[i + 2] as u32) << 16
                | (k[i + 3] as u32) << 24,
        );
        b = b.wrapping_add(
            k[i + 4] as u32
                | (k[i + 5] as u32) << 8
                | (k[i + 6] as u32) << 16
                | (k[i + 7] as u32) << 24,
        );
        c = c.wrapping_add(
            k[i + 8] as u32
                | (k[i + 9] as u32) << 8
                | (k[i + 10] as u32) << 16
                | (k[i + 11] as u32) << 24,
        );
        mix(&mut a, &mut b, &mut c);
        i += 12;
        len -= 12;
    }

    // Handle the last 11 bytes.
    c = c.wrapping_add(length);
    #[allow(clippy::identity_op)]
    match len {
        11 => {
            c = c.wrapping_add((k[i + 10] as u32) << 24);
            c = c.wrapping_add((k[i + 9] as u32) << 16);
            c = c.wrapping_add((k[i + 8] as u32) << 8);
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        10 => {
            c = c.wrapping_add((k[i + 9] as u32) << 16);
            c = c.wrapping_add((k[i + 8] as u32) << 8);
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        9 => {
            c = c.wrapping_add((k[i + 8] as u32) << 8);
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        8 => {
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        7 => {
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        6 => {
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        5 => {
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        4 => {
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        3 => {
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        2 => {
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        1 => {
            a = a.wrapping_add(k[i + 0] as u32);
        }
        _ => {} // case 0: nothing left to add
    }
    mix(&mut a, &mut b, &mut c);
    c
}

/// The Jenkins lookup2 mix macro, translated to Rust.
#[inline]
fn mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 13);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 8);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 13);
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 12);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 16);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 5);
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 3);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 10);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 15);
}

/// Compute the Blizzard `StringHash` value for a string.
///
/// Mirrors the C `SStrHash2` function:
/// - lowercase `a`–`z` → uppercase (subtract `0x20`)
/// - `/` → `\`
/// - everything else unchanged
///
/// Returns the hash as an `i32` (matching JASS `integer` semantics).
pub fn blizzard_string_hash(key: &str) -> i32 {
    let mut buf: Vec<u8> = Vec::with_capacity(key.len());
    for &byte in key.as_bytes() {
        if byte >= b'a' && byte <= b'z' {
            buf.push(byte - 0x20);
        } else if byte == b'/' {
            buf.push(b'\\');
        } else {
            buf.push(byte);
        }
    }
    jenkins_hash(&buf, 0) as i32
}

// ─── Constant value ──────────────────────────────────────────────────────────

/// A compile-time constant value extracted from JASS source.
#[derive(Debug, Clone)]
pub enum ConstValue {
    Str(String),
    Int(i32),
}

// ─── Collect constants from globals ──────────────────────────────────────────

/// Parse `globals_out` lines and collect compile-time constant values.
///
/// Processes lines in order so that later constants can reference earlier ones:
/// ```text
/// constant string  A = "hello"
/// constant string  B = A + " world"
/// constant integer C = 2 * 3
/// ```
pub fn collect_constants(globals: &[String]) -> HashMap<String, ConstValue> {
    let mut map = HashMap::<String, ConstValue>::new();
    for line in globals {
        let t = line.trim();
        let t = match t.strip_prefix("constant ") {
            Some(rest) => rest.trim(),
            None => continue,
        };
        // Determine the type.
        let (ty, rest) = if let Some(r) = t.strip_prefix("string ") {
            ("string", r.trim())
        } else if let Some(r) = t.strip_prefix("integer ") {
            ("integer", r.trim())
        } else {
            continue;
        };
        // `NAME = EXPR`
        let eq_pos = match rest.find('=') {
            Some(p) => p,
            None => continue,
        };
        let name = rest[..eq_pos].trim();
        let expr_text = rest[eq_pos + 1..].trim();
        if name.is_empty() || expr_text.is_empty() {
            continue;
        }
        if let Some(val) = eval_const_expr(expr_text, &map) {
            match (&val, ty) {
                (ConstValue::Str(_), "string") | (ConstValue::Int(_), "integer") => {
                    map.insert(name.to_string(), val);
                }
                _ => {}
            }
        }
    }
    map
}

// ─── Expression evaluator ────────────────────────────────────────────────────
//
// Recursive-descent parser for JASS constant expressions.
//
// Grammar (simplified):
//   expr       → add_expr
//   add_expr   → mul_expr (('+' | '-') mul_expr)*
//   mul_expr   → unary_expr (('*' | '/' | '%') unary_expr)*
//   unary_expr → '-' unary_expr | atom
//   atom       → STRING_LITERAL | INTEGER_LITERAL | IDENTIFIER
//                | '(' expr ')' | I2S '(' expr ')'

/// Token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Str(String),
    Int(i32),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

/// Tokenise a JASS expression fragment.
fn tokenize(input: &str) -> Option<Vec<Token>> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => { i += 1; }
            b'+' => { tokens.push(Token::Plus); i += 1; }
            b'-' => { tokens.push(Token::Minus); i += 1; }
            b'*' => { tokens.push(Token::Star); i += 1; }
            b'/' => { tokens.push(Token::Slash); i += 1; }
            b'%' => { tokens.push(Token::Percent); i += 1; }
            b'(' => { tokens.push(Token::LParen); i += 1; }
            b')' => { tokens.push(Token::RParen); i += 1; }
            b'"' => {
                // String literal.
                i += 1;
                let mut s = String::new();
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        match bytes[i + 1] {
                            b'\\' => s.push('\\'),
                            b'"'  => s.push('"'),
                            b'n'  => s.push('\n'),
                            b'r'  => s.push('\r'),
                            b't'  => s.push('\t'),
                            _     => return None,
                        }
                        i += 2;
                    } else {
                        s.push(bytes[i] as char);
                        i += 1;
                    }
                }
                if i >= bytes.len() { return None; } // unterminated
                i += 1; // skip closing '"'
                tokens.push(Token::Str(s));
            }
            b'\'' => {
                // FourCC literal: 'ABCD' → i32
                i += 1;
                let mut val: i32 = 0;
                while i < bytes.len() && bytes[i] != b'\'' {
                    val = (val << 8) | (bytes[i] as i32);
                    i += 1;
                }
                if i >= bytes.len() { return None; }
                i += 1;
                tokens.push(Token::Int(val));
            }
            b'$' => {
                // JASS hex literal: $FF
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() { i += 1; }
                if i == start { return None; }
                let val = i32::from_str_radix(&input[start..i], 16).ok()?;
                tokens.push(Token::Int(val));
            }
            b'0' if i + 1 < bytes.len() && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') => {
                // 0x hex literal
                i += 2;
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() { i += 1; }
                if i == start { return None; }
                let val = i32::from_str_radix(&input[start..i], 16).ok()?;
                tokens.push(Token::Int(val));
            }
            b'0' if i + 1 < bytes.len() && bytes[i + 1] >= b'0' && bytes[i + 1] <= b'7' => {
                // Octal literal: 012 == 10
                let start = i;
                i += 1; // skip leading '0'
                while i < bytes.len() && bytes[i] >= b'0' && bytes[i] <= b'7' { i += 1; }
                let val = i32::from_str_radix(&input[start + 1..i], 8).ok()?;
                tokens.push(Token::Int(val));
            }
            b if b.is_ascii_digit() => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
                let val: i32 = input[start..i].parse().ok()?;
                tokens.push(Token::Int(val));
            }
            b if b.is_ascii_alphabetic() || b == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push(Token::Ident(input[start..i].to_string()));
            }
            _ => return None, // unexpected character
        }
    }
    Some(tokens)
}

/// Parser state: a token stream with a cursor.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self { Self { tokens, pos: 0 } }
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }
    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(t)
    }
    fn at_end(&self) -> bool { self.pos >= self.tokens.len() }
}

/// Evaluate a constant expression string given a map of known constants.
pub fn eval_const_expr(input: &str, constants: &HashMap<String, ConstValue>) -> Option<ConstValue> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() { return None; }
    let mut parser = Parser::new(tokens);
    let val = parse_add(&mut parser, constants)?;
    if parser.at_end() { Some(val) } else { None }
}

fn parse_add(p: &mut Parser, c: &HashMap<String, ConstValue>) -> Option<ConstValue> {
    let mut left = parse_mul(p, c)?;
    loop {
        match p.peek() {
            Some(Token::Plus) => {
                p.next();
                let right = parse_mul(p, c)?;
                left = apply_add(left, right)?;
            }
            Some(Token::Minus) => {
                p.next();
                let right = parse_mul(p, c)?;
                left = apply_sub(left, right)?;
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_mul(p: &mut Parser, c: &HashMap<String, ConstValue>) -> Option<ConstValue> {
    let mut left = parse_unary(p, c)?;
    loop {
        match p.peek() {
            Some(Token::Star) => {
                p.next();
                let right = parse_unary(p, c)?;
                match (&left, &right) {
                    (ConstValue::Int(a), ConstValue::Int(b)) => left = ConstValue::Int(a.wrapping_mul(*b)),
                    _ => return None,
                }
            }
            Some(Token::Slash) => {
                p.next();
                let right = parse_unary(p, c)?;
                match (&left, &right) {
                    (ConstValue::Int(a), ConstValue::Int(b)) if *b != 0 => left = ConstValue::Int(a.wrapping_div(*b)),
                    _ => return None,
                }
            }
            Some(Token::Percent) => {
                p.next();
                let right = parse_unary(p, c)?;
                match (&left, &right) {
                    (ConstValue::Int(a), ConstValue::Int(b)) if *b != 0 => left = ConstValue::Int(a.wrapping_rem(*b)),
                    _ => return None,
                }
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_unary(p: &mut Parser, c: &HashMap<String, ConstValue>) -> Option<ConstValue> {
    if let Some(Token::Minus) = p.peek() {
        p.next();
        let val = parse_unary(p, c)?;
        return match val {
            ConstValue::Int(n) => Some(ConstValue::Int(n.wrapping_neg())),
            _ => None,
        };
    }
    parse_atom(p, c)
}

fn parse_atom(p: &mut Parser, c: &HashMap<String, ConstValue>) -> Option<ConstValue> {
    match p.next()? {
        Token::Str(s) => Some(ConstValue::Str(s)),
        Token::Int(n) => Some(ConstValue::Int(n)),
        Token::Ident(name) => {
            // I2S(expr) — integer → string conversion
            if name == "I2S" {
                if p.next() != Some(Token::LParen) { return None; }
                let val = parse_add(p, c)?;
                if p.next() != Some(Token::RParen) { return None; }
                return match val {
                    ConstValue::Int(n) => Some(ConstValue::Str(n.to_string())),
                    _ => None,
                };
            }
            // Look up in constants map.
            c.get(&name).cloned()
        }
        Token::LParen => {
            let val = parse_add(p, c)?;
            if p.next() != Some(Token::RParen) { return None; }
            Some(val)
        }
        _ => None,
    }
}

/// `+` : string concatenation or integer addition.
///
/// Mixed operands (string + integer, integer + string) are supported:
/// the integer is converted to its decimal string representation (like `I2S`).
fn apply_add(left: ConstValue, right: ConstValue) -> Option<ConstValue> {
    match (left, right) {
        (ConstValue::Int(a), ConstValue::Int(b)) => Some(ConstValue::Int(a.wrapping_add(b))),
        (ConstValue::Str(a), ConstValue::Str(b)) => Some(ConstValue::Str(a + &b)),
        (ConstValue::Str(a), ConstValue::Int(b)) => Some(ConstValue::Str(format!("{}{}", a, b))),
        (ConstValue::Int(a), ConstValue::Str(b)) => Some(ConstValue::Str(format!("{}{}", a, b))),
    }
}

/// `-` : integer subtraction only.
fn apply_sub(left: ConstValue, right: ConstValue) -> Option<ConstValue> {
    match (left, right) {
        (ConstValue::Int(a), ConstValue::Int(b)) => Some(ConstValue::Int(a.wrapping_sub(b))),
        _ => None,
    }
}

// ─── Fold StringHash calls ───────────────────────────────────────────────────

/// Fold `StringHash(expr)` calls in a source string.
///
/// For each `StringHash(...)` call, extracts the argument expression,
/// evaluates it using the provided constants map, and — if it resolves to a
/// string — replaces the call with the precomputed integer hash.
pub fn fold_string_hash(source: &str, constants: &HashMap<String, ConstValue>) -> String {
    const PREFIX: &str = "StringHash(";
    let mut result = String::with_capacity(source.len());
    let mut search_from = 0;

    while let Some(start) = source[search_from..].find(PREFIX) {
        let abs_start = search_from + start;

        // Verify word boundary.
        if abs_start > 0 {
            let prev = source.as_bytes()[abs_start - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                result.push_str(&source[search_from..abs_start + PREFIX.len()]);
                search_from = abs_start + PREFIX.len();
                continue;
            }
        }

        let args_start = abs_start + PREFIX.len();

        // Find the matching closing `)`.
        if let Some(close) = find_matching_paren(source, args_start) {
            let expr_text = &source[args_start..close];
            if let Some(ConstValue::Str(s)) = eval_const_expr(expr_text, constants) {
                result.push_str(&source[search_from..abs_start]);
                let hash = blizzard_string_hash(&s);
                result.push_str(&hash.to_string());
                search_from = close + 1; // skip past ')'
                continue;
            }
        }

        // Could not evaluate — skip past the prefix.
        result.push_str(&source[search_from..args_start]);
        search_from = args_start;
    }

    result.push_str(&source[search_from..]);
    result
}

/// Find the position of the closing `)` that matches an already-consumed `(`.
///
/// `start` points to the first character **inside** the parentheses.
/// Returns the byte offset of the matching `)`, or `None`.
pub fn find_matching_paren(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth: usize = 1;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' => {
                // Skip over string literal.
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' { i += 2; continue; }
                    if bytes[i] == b'"' { break; }
                    i += 1;
                }
            }
            b'\'' => {
                // Skip over FourCC literal.
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' { i += 1; }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split a comma-separated argument list into `(start_byte, end_byte)` ranges.
///
/// `args_start` is the byte offset right after the opening `(`.
/// `args_end` is the byte offset of the closing `)`.
/// Handles nested parentheses, strings, and FourCC literals.
pub fn split_call_args(source: &str, args_start: usize, args_end: usize) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut args = Vec::new();
    let mut depth: usize = 0;
    let mut arg_start = args_start;
    let mut i = args_start;

    while i < args_end {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                if depth > 0 { depth -= 1; }
            }
            b',' if depth == 0 => {
                args.push((arg_start, i));
                arg_start = i + 1;
            }
            b'"' => {
                i += 1;
                while i < args_end {
                    if bytes[i] == b'\\' { i += 2; continue; }
                    if bytes[i] == b'"' { break; }
                    i += 1;
                }
            }
            b'\'' => {
                i += 1;
                while i < args_end && bytes[i] != b'\'' { i += 1; }
            }
            _ => {}
        }
        i += 1;
    }
    if arg_start < args_end {
        args.push((arg_start, args_end));
    }
    args
}

/// Fold string arguments in integer parameter positions.
///
/// For each known function call in `source`, checks whether any argument
/// evaluates to a string but the corresponding parameter expects `integer`.
/// If so, computes `StringHash(argument)` and replaces the argument text.
///
/// `signatures`: `func_name → [param_type, …]`
pub fn fold_string_integer_args(
    source: &str,
    constants: &HashMap<String, ConstValue>,
    signatures: &HashMap<String, Vec<String>>,
) -> String {
    // Collect replacement spans first, then apply in reverse order.
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    // Scan for identifier followed by `(`.
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip string literals.
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' { i += 2; continue; }
                if bytes[i] == b'"' { break; }
                i += 1;
            }
            i += 1;
            continue;
        }
        // Skip FourCC.
        if bytes[i] == b'\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'\'' { i += 1; }
            i += 1;
            continue;
        }

        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let id_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let name = &source[id_start..i];

            // Skip whitespace between name and `(`.
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') { j += 1; }

            if j < bytes.len() && bytes[j] == b'(' {
                let args_start = j + 1;
                if let Some(close) = find_matching_paren(source, args_start) {
                    if let Some(param_types) = signatures.get(name) {
                        let arg_ranges = split_call_args(source, args_start, close);
                        for (idx, &(a_start, a_end)) in arg_ranges.iter().enumerate() {
                            if idx >= param_types.len() { break; }
                            if param_types[idx] != "integer" { continue; }

                            let arg_text = source[a_start..a_end].trim();
                            if let Some(ConstValue::Str(s)) = eval_const_expr(arg_text, constants) {
                                let hash = blizzard_string_hash(&s);
                                // Trim whitespace to get exact positions.
                                let trimmed_start = a_start + source[a_start..a_end].len()
                                    - source[a_start..a_end].trim_start().len();
                                let trimmed_end = a_end - (source[a_start..a_end].len()
                                    - source[a_start..a_end].trim_end().len());
                                replacements.push((trimmed_start, trimmed_end, hash.to_string()));
                            }
                        }
                    }
                    i = close + 1;
                    continue;
                }
            }
            continue;
        }
        i += 1;
    }

    if replacements.is_empty() {
        return source.to_string();
    }

    // Apply replacements in reverse order so offsets stay valid.
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.to_string();
    for (start, end, replacement) in replacements {
        result.replace_range(start..end, &replacement);
    }
    result
}

