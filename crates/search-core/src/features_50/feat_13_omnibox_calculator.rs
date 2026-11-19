//! Omnibox inline calculator, percentage evaluator, and unit converter.
//!
//! Provides a safe, bounded recursive-descent expression parser and evaluator
//! for Search Linux's omnibox so users can compute arithmetic, percentages,
//! math functions, base-prefixed literals, and common unit conversions inline.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

const DEFAULT_MAX_DEPTH: usize = 32;
const DEFAULT_MAX_TOKENS: usize = 256;
const DEFAULT_PRECISION: usize = 10;

/// Angle unit used by trigonometric functions (`sin`, `cos`, `tan`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AngleMode {
    #[default]
    Radians,
    Degrees,
}

/// Configuration for the omnibox calculator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalcConfig {
    /// Maximum decimal places shown in formatted floating-point output (1..=15).
    pub precision: usize,
    /// Maximum AST / parenthesis nesting depth (1..=128).
    pub max_depth: usize,
    /// Maximum number of lexer tokens allowed per expression (8..=2048).
    pub max_tokens: usize,
    /// Angle unit for trigonometric functions.
    pub angle_mode: AngleMode,
    /// Whether `<value> <unit> in|to <unit>` conversions are enabled.
    pub allow_unit_conversion: bool,
    /// Whether to include alternate representations (hex/bin/oct) for integer results.
    pub include_base_alternates: bool,
}

impl Default for CalcConfig {
    fn default() -> Self {
        Self {
            precision: DEFAULT_PRECISION,
            max_depth: DEFAULT_MAX_DEPTH,
            max_tokens: DEFAULT_MAX_TOKENS,
            angle_mode: AngleMode::Radians,
            allow_unit_conversion: true,
            include_base_alternates: true,
        }
    }
}

impl CalcConfig {
    /// Validates configuration bounds.
    pub fn validate(&self) -> Result<(), CalcError> {
        if self.precision == 0 || self.precision > 15 {
            return Err(CalcError::InvalidConfig(
                "precision must be between 1 and 15".to_string(),
            ));
        }
        if self.max_depth == 0 || self.max_depth > 128 {
            return Err(CalcError::InvalidConfig(
                "max_depth must be between 1 and 128".to_string(),
            ));
        }
        if self.max_tokens < 8 || self.max_tokens > 2048 {
            return Err(CalcError::InvalidConfig(
                "max_tokens must be between 8 and 2048".to_string(),
            ));
        }
        Ok(())
    }
}

/// Classification of the evaluated omnibox calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalcResultKind {
    Arithmetic,
    Percentage,
    UnitConversion,
}

/// Alternate representation of a numeric result (e.g., hex, binary, octal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalcAlternate {
    pub label: String,
    pub value: String,
}

/// Evaluated omnibox calculation result ready for UI presentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcResult {
    pub raw_input: String,
    pub normalized_expression: String,
    pub value: f64,
    pub formatted: String,
    pub unit: Option<String>,
    pub kind: CalcResultKind,
    pub alternates: Vec<CalcAlternate>,
    pub copy_value: String,
}

/// Errors produced during configuration validation, lexing, parsing, or evaluation.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum CalcError {
    #[error("invalid calculator configuration: {0}")]
    InvalidConfig(String),
    #[error("empty expression")]
    EmptyInput,
    #[error("input is not a calculable expression")]
    NotAnExpression,
    #[error("syntax error: {0}")]
    InvalidSyntax(String),
    #[error("division by zero")]
    DivisionByZero,
    #[error("math domain error: {0}")]
    DomainError(String),
    #[error("numeric overflow or non-finite result")]
    Overflow,
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    #[error("unknown identifier: {0}")]
    UnknownIdentifier(String),
    #[error("incompatible units: cannot convert {from} to {to}")]
    IncompatibleUnits { from: String, to: String },
    #[error("unknown unit: {0}")]
    UnknownUnit(String),
    #[error("expression nesting exceeds max depth ({0})")]
    DepthLimitExceeded(usize),
    #[error("expression exceeds max token count ({0})")]
    TokenLimitExceeded(usize),
}

/// Safe, stateless omnibox expression evaluator.
#[derive(Debug, Clone, Default)]
pub struct OmniboxCalculator {
    config: CalcConfig,
}

impl OmniboxCalculator {
    /// Creates a new calculator after validating the provided configuration.
    pub fn new(config: CalcConfig) -> Result<Self, CalcError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &CalcConfig {
        &self.config
    }

    /// Evaluates an omnibox query, returning `None` if the query is not a math
    /// expression or if evaluation fails.
    ///
    /// Plain single numbers (e.g., `"42"`) and plain words (e.g., `"rust"`)
    /// return `None` so the omnibox does not show redundant calculator suggestions.
    pub fn try_suggest(&self, input: &str) -> Option<CalcResult> {
        let raw_trimmed = input.trim();
        let trimmed = strip_leading_equals(raw_trimmed).trim();
        if trimmed.is_empty() {
            return None;
        }
        if !raw_trimmed.starts_with('=') && !looks_like_calculation(trimmed) {
            return None;
        }
        self.evaluate(input).ok()
    }

    /// Evaluates an expression or unit conversion and returns a structured result.
    pub fn evaluate(&self, input: &str) -> Result<CalcResult, CalcError> {
        self.config.validate()?;

        let trimmed = strip_leading_equals(input).trim();
        if trimmed.is_empty() {
            return Err(CalcError::EmptyInput);
        }

        if self.config.allow_unit_conversion {
            if let Some(conv_result) = self.try_evaluate_unit_conversion(input, trimmed)? {
                return Ok(conv_result);
            }
        }

        if let Some(pct_result) = self.try_evaluate_percentage_phrase(input, trimmed)? {
            return Ok(pct_result);
        }

        let tokens = tokenize(trimmed, self.config.max_tokens)?;
        if tokens.is_empty() {
            return Err(CalcError::EmptyInput);
        }

        let mut parser = Parser::new(&tokens, &self.config);
        let value = parser.parse_expression(0)?;
        parser.expect_end()?;

        if !value.is_finite() {
            return Err(CalcError::Overflow);
        }

        let clean_value = normalize_zero(value);
        let formatted = format_number(clean_value, self.config.precision);
        let alternates = if self.config.include_base_alternates {
            build_base_alternates(clean_value)
        } else {
            Vec::new()
        };

        Ok(CalcResult {
            raw_input: input.to_string(),
            normalized_expression: trimmed.to_string(),
            value: clean_value,
            formatted: formatted.clone(),
            unit: None,
            kind: CalcResultKind::Arithmetic,
            alternates,
            copy_value: formatted,
        })
    }

    fn try_evaluate_percentage_phrase(
        &self,
        raw_input: &str,
        trimmed: &str,
    ) -> Result<Option<CalcResult>, CalcError> {
        let lower = trimmed.to_ascii_lowercase();
        if let Some((left, right)) = split_once_word(&lower, "of") {
            let left_trim = left.trim();
            if let Some(pct_str) = left_trim.strip_suffix('%') {
                let pct = self.eval_subexpression(pct_str.trim())?;
                let base = self.eval_subexpression(right.trim())?;
                let result = normalize_zero((pct / 100.0) * base);
                if !result.is_finite() {
                    return Err(CalcError::Overflow);
                }
                let formatted = format_number(result, self.config.precision);
                return Ok(Some(CalcResult {
                    raw_input: raw_input.to_string(),
                    normalized_expression: format!("({pct_str}% of {})", right.trim()),
                    value: result,
                    formatted: formatted.clone(),
                    unit: None,
                    kind: CalcResultKind::Percentage,
                    alternates: Vec::new(),
                    copy_value: formatted,
                }));
            }
        }
        Ok(None)
    }

    fn try_evaluate_unit_conversion(
        &self,
        raw_input: &str,
        trimmed: &str,
    ) -> Result<Option<CalcResult>, CalcError> {
        let lower = trimmed.to_ascii_lowercase();
        let split = split_once_word(&lower, "in").or_else(|| split_once_word(&lower, "to"));
        let Some((lhs, rhs)) = split else {
            return Ok(None);
        };

        let rhs_unit_str = rhs.trim();
        let Some(to_unit) = Unit::parse(rhs_unit_str) else {
            return Err(CalcError::UnknownUnit(rhs_unit_str.to_string()));
        };

        let lhs_trim = lhs.trim();
        let (expr_part, from_unit_str) = split_trailing_unit(lhs_trim).ok_or_else(|| {
            CalcError::InvalidSyntax("missing source unit before conversion keyword".to_string())
        })?;

        let Some(from_unit) = Unit::parse(from_unit_str) else {
            return Err(CalcError::UnknownUnit(from_unit_str.to_string()));
        };

        let source_value = self.eval_subexpression(expr_part)?;
        let converted = convert_unit(source_value, from_unit, to_unit)?;
        if !converted.is_finite() {
            return Err(CalcError::Overflow);
        }

        let clean = normalize_zero(converted);
        let num_str = format_number(clean, self.config.precision);
        let unit_label = to_unit.canonical_symbol().to_string();
        let formatted = format!("{num_str} {unit_label}");

        Ok(Some(CalcResult {
            raw_input: raw_input.to_string(),
            normalized_expression: format!(
                "{} {} -> {}",
                format_number(source_value, self.config.precision),
                from_unit.canonical_symbol(),
                unit_label
            ),
            value: clean,
            formatted,
            unit: Some(unit_label),
            kind: CalcResultKind::UnitConversion,
            alternates: Vec::new(),
            copy_value: num_str,
        }))
    }

    fn eval_subexpression(&self, expr: &str) -> Result<f64, CalcError> {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return Err(CalcError::InvalidSyntax("empty subexpression".to_string()));
        }
        let tokens = tokenize(trimmed, self.config.max_tokens)?;
        let mut parser = Parser::new(&tokens, &self.config);
        let val = parser.parse_expression(0)?;
        parser.expect_end()?;
        if !val.is_finite() {
            return Err(CalcError::Overflow);
        }
        Ok(val)
    }
}

fn strip_leading_equals(input: &str) -> &str {
    let trimmed = input.trim_start();
    trimmed.strip_prefix('=').unwrap_or(trimmed)
}

fn looks_like_calculation(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.starts_with('=') {
        return true;
    }
    if lower.contains(" in ") || lower.contains(" to ") || lower.contains(" of ") {
        return true;
    }
    let has_operator = trimmed
        .bytes()
        .any(|b| matches!(b, b'+' | b'-' | b'*' | b'/' | b'^' | b'%' | b'(' | b')'));
    if has_operator {
        return true;
    }
    if lower.starts_with("0x") || lower.starts_with("0b") || lower.starts_with("0o") {
        return true;
    }
    false
}

fn split_once_word<'a>(haystack: &'a str, word: &str) -> Option<(&'a str, &'a str)> {
    let bytes = haystack.as_bytes();
    let wbytes = word.as_bytes();
    if bytes.len() < wbytes.len() + 2 {
        return None;
    }
    for i in 1..=(bytes.len() - wbytes.len() - 1) {
        if &bytes[i..i + wbytes.len()] == wbytes
            && bytes[i - 1].is_ascii_whitespace()
            && bytes[i + wbytes.len()].is_ascii_whitespace()
        {
            let left = &haystack[..i];
            let right = &haystack[i + wbytes.len()..];
            return Some((left, right));
        }
    }
    None
}

fn split_trailing_unit(lhs: &str) -> Option<(&str, &str)> {
    let trimmed = lhs.trim();
    let mut split_idx = None;
    for (idx, ch) in trimmed.char_indices().rev() {
        if ch.is_ascii_alphabetic() {
            split_idx = Some(idx);
        } else {
            break;
        }
    }
    let idx = split_idx?;
    let expr = trimmed[..idx].trim();
    let unit = trimmed[idx..].trim();
    if expr.is_empty() || unit.is_empty() {
        return None;
    }
    Some((expr, unit))
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Comma,
}

fn tokenize(input: &str, max_tokens: usize) -> Result<Vec<Token>, CalcError> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if tokens.len() >= max_tokens {
            return Err(CalcError::TokenLimitExceeded(max_tokens));
        }

        match b {
            b'+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            b'-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            b'*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            b'/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            b'%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            b'^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            b'(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            b')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            b',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            b'0'..=b'9' | b'.' => {
                let (num, next_i) = parse_number_token(input, i)?;
                tokens.push(Token::Number(num));
                i = next_i;
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push(Token::Ident(input[start..i].to_ascii_lowercase()));
            }
            other => {
                return Err(CalcError::InvalidSyntax(format!(
                    "unexpected character '{}'",
                    other as char
                )));
            }
        }
    }

    Ok(tokens)
}

fn parse_number_token(input: &str, start: usize) -> Result<(f64, usize), CalcError> {
    let bytes = input.as_bytes();
    if start + 2 <= bytes.len() && bytes[start] == b'0' {
        let radix = match bytes[start + 1] {
            b'x' | b'X' => Some(16),
            b'b' | b'B' => Some(2),
            b'o' | b'O' => Some(8),
            _ => None,
        };
        if let Some(radix) = radix {
            let mut i = start + 2;
            let mut digits = String::new();
            while i < bytes.len() {
                let c = bytes[i];
                if c == b'_' {
                    i += 1;
                    continue;
                }
                if (c as char).is_digit(radix) {
                    digits.push(c as char);
                    i += 1;
                } else {
                    break;
                }
            }
            if digits.is_empty() {
                return Err(CalcError::InvalidSyntax(
                    "expected digits after base prefix".to_string(),
                ));
            }
            let val = u64::from_str_radix(&digits, radix).map_err(|_| CalcError::Overflow)?;
            return Ok((val as f64, i));
        }
    }

    let mut i = start;
    let mut seen_dot = false;
    let mut seen_exp = false;
    let mut cleaned = String::new();

    while i < bytes.len() {
        let c = bytes[i];
        if c == b'_' {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            cleaned.push(c as char);
            i += 1;
        } else if c == b'.' && !seen_dot && !seen_exp {
            seen_dot = true;
            cleaned.push('.');
            i += 1;
        } else if (c == b'e' || c == b'E') && !seen_exp && !cleaned.is_empty() {
            seen_exp = true;
            cleaned.push('e');
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                cleaned.push(bytes[i] as char);
                i += 1;
            }
        } else {
            break;
        }
    }

    let val: f64 = cleaned
        .parse()
        .map_err(|_| CalcError::InvalidSyntax(format!("invalid number '{cleaned}'")))?;
    if !val.is_finite() {
        return Err(CalcError::Overflow);
    }
    Ok((val, i))
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    config: &'a CalcConfig,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], config: &'a CalcConfig) -> Self {
        Self {
            tokens,
            pos: 0,
            config,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect_end(&self) -> Result<(), CalcError> {
        if let Some(tok) = self.peek() {
            return Err(CalcError::InvalidSyntax(format!(
                "unexpected token {tok:?}"
            )));
        }
        Ok(())
    }

    fn parse_expression(&mut self, depth: usize) -> Result<f64, CalcError> {
        if depth > self.config.max_depth {
            return Err(CalcError::DepthLimitExceeded(self.config.max_depth));
        }
        self.parse_additive(depth)
    }

    fn parse_additive(&mut self, depth: usize) -> Result<f64, CalcError> {
        let mut left = self.parse_multiplicative(depth)?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Plus => {
                    self.next();
                    let right = self.parse_multiplicative(depth)?;
                    // Support "200 + 15%" -> 200 + (200 * 0.15) when right came from a trailing '%'
                    left += right;
                }
                Token::Minus => {
                    self.next();
                    let right = self.parse_multiplicative(depth)?;
                    left -= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self, depth: usize) -> Result<f64, CalcError> {
        let mut left = self.parse_power(depth)?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Star => {
                    self.next();
                    let right = self.parse_power(depth)?;
                    left *= right;
                }
                Token::Slash => {
                    self.next();
                    let right = self.parse_power(depth)?;
                    if right == 0.0 {
                        return Err(CalcError::DivisionByZero);
                    }
                    left /= right;
                }
                Token::Percent => {
                    self.next();
                    // If followed by another primary/expression, treat as modulo; otherwise percentage (/ 100).
                    if self.starts_primary() {
                        let right = self.parse_power(depth)?;
                        if right == 0.0 {
                            return Err(CalcError::DivisionByZero);
                        }
                        left %= right;
                    } else {
                        left /= 100.0;
                    }
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn starts_primary(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::Number(_) | Token::Ident(_) | Token::LParen | Token::Plus | Token::Minus)
        )
    }

    fn parse_power(&mut self, depth: usize) -> Result<f64, CalcError> {
        let base = self.parse_unary(depth)?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.next();
            let exp = self.parse_power(depth + 1)?;
            let res = base.powf(exp);
            if !res.is_finite() {
                return Err(CalcError::Overflow);
            }
            return Ok(res);
        }
        Ok(base)
    }

    fn parse_unary(&mut self, depth: usize) -> Result<f64, CalcError> {
        if depth > self.config.max_depth {
            return Err(CalcError::DepthLimitExceeded(self.config.max_depth));
        }
        match self.peek() {
            Some(Token::Plus) => {
                self.next();
                self.parse_unary(depth + 1)
            }
            Some(Token::Minus) => {
                self.next();
                let val = self.parse_unary(depth + 1)?;
                Ok(-val)
            }
            _ => self.parse_primary(depth),
        }
    }

    fn parse_primary(&mut self, depth: usize) -> Result<f64, CalcError> {
        if depth > self.config.max_depth {
            return Err(CalcError::DepthLimitExceeded(self.config.max_depth));
        }
        let tok = self
            .next()
            .cloned()
            .ok_or_else(|| CalcError::InvalidSyntax("unexpected end of expression".to_string()))?;

        match tok {
            Token::Number(n) => Ok(n),
            Token::LParen => {
                let val = self.parse_expression(depth + 1)?;
                match self.next() {
                    Some(Token::RParen) => Ok(val),
                    _ => Err(CalcError::InvalidSyntax("missing closing ')'".to_string())),
                }
            }
            Token::Ident(name) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_expression(depth + 1)?);
                            match self.peek() {
                                Some(Token::Comma) => {
                                    self.next();
                                }
                                Some(Token::RParen) => break,
                                _ => {
                                    return Err(CalcError::InvalidSyntax(
                                        "expected ',' or ')' in function call".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    self.next(); // consume ')'
                    eval_function(&name, &args, self.config.angle_mode)
                } else {
                    eval_constant(&name)
                }
            }
            other => Err(CalcError::InvalidSyntax(format!(
                "unexpected token {other:?}"
            ))),
        }
    }
}

fn eval_constant(name: &str) -> Result<f64, CalcError> {
    match name {
        "pi" => Ok(std::f64::consts::PI),
        "e" => Ok(std::f64::consts::E),
        "tau" => Ok(std::f64::consts::TAU),
        _ => Err(CalcError::UnknownIdentifier(name.to_string())),
    }
}

fn eval_function(name: &str, args: &[f64], angle_mode: AngleMode) -> Result<f64, CalcError> {
    let to_rad = |v: f64| match angle_mode {
        AngleMode::Radians => v,
        AngleMode::Degrees => v.to_radians(),
    };
    let from_rad = |v: f64| match angle_mode {
        AngleMode::Radians => v,
        AngleMode::Degrees => v.to_degrees(),
    };

    let require_arity = |expected: usize| -> Result<(), CalcError> {
        if args.len() != expected {
            return Err(CalcError::InvalidSyntax(format!(
                "function '{name}' expects {expected} argument(s), got {}",
                args.len()
            )));
        }
        Ok(())
    };

    let res = match name {
        "sqrt" => {
            require_arity(1)?;
            if args[0] < 0.0 {
                return Err(CalcError::DomainError(
                    "sqrt of negative number".to_string(),
                ));
            }
            args[0].sqrt()
        }
        "abs" => {
            require_arity(1)?;
            args[0].abs()
        }
        "floor" => {
            require_arity(1)?;
            args[0].floor()
        }
        "ceil" => {
            require_arity(1)?;
            args[0].ceil()
        }
        "round" => {
            require_arity(1)?;
            args[0].round()
        }
        "sin" => {
            require_arity(1)?;
            to_rad(args[0]).sin()
        }
        "cos" => {
            require_arity(1)?;
            to_rad(args[0]).cos()
        }
        "tan" => {
            require_arity(1)?;
            to_rad(args[0]).tan()
        }
        "asin" => {
            require_arity(1)?;
            if !(-1.0..=1.0).contains(&args[0]) {
                return Err(CalcError::DomainError(
                    "asin argument must be in [-1, 1]".to_string(),
                ));
            }
            from_rad(args[0].asin())
        }
        "acos" => {
            require_arity(1)?;
            if !(-1.0..=1.0).contains(&args[0]) {
                return Err(CalcError::DomainError(
                    "acos argument must be in [-1, 1]".to_string(),
                ));
            }
            from_rad(args[0].acos())
        }
        "atan" => {
            require_arity(1)?;
            from_rad(args[0].atan())
        }
        "ln" => {
            require_arity(1)?;
            if args[0] <= 0.0 {
                return Err(CalcError::DomainError(
                    "ln argument must be positive".to_string(),
                ));
            }
            args[0].ln()
        }
        "log" | "log10" => {
            require_arity(1)?;
            if args[0] <= 0.0 {
                return Err(CalcError::DomainError(
                    "log10 argument must be positive".to_string(),
                ));
            }
            args[0].log10()
        }
        "log2" => {
            require_arity(1)?;
            if args[0] <= 0.0 {
                return Err(CalcError::DomainError(
                    "log2 argument must be positive".to_string(),
                ));
            }
            args[0].log2()
        }
        "exp" => {
            require_arity(1)?;
            args[0].exp()
        }
        "min" => {
            if args.is_empty() {
                return Err(CalcError::InvalidSyntax(
                    "min requires at least 1 argument".to_string(),
                ));
            }
            args.iter().copied().fold(f64::INFINITY, f64::min)
        }
        "max" => {
            if args.is_empty() {
                return Err(CalcError::InvalidSyntax(
                    "max requires at least 1 argument".to_string(),
                ));
            }
            args.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        }
        "pow" => {
            require_arity(2)?;
            args[0].powf(args[1])
        }
        _ => return Err(CalcError::UnknownFunction(name.to_string())),
    };

    if !res.is_finite() {
        return Err(CalcError::Overflow);
    }
    Ok(res)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitCategory {
    Length,
    Mass,
    Data,
    Time,
    Temperature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    // Length (base: meter)
    Millimeter,
    Centimeter,
    Meter,
    Kilometer,
    Inch,
    Foot,
    Yard,
    Mile,
    // Mass (base: kilogram)
    Milligram,
    Gram,
    Kilogram,
    Ounce,
    Pound,
    // Data (base: byte)
    Byte,
    Kilobyte,
    Megabyte,
    Gigabyte,
    Terabyte,
    Kibibyte,
    Mebibyte,
    Gibibyte,
    Tebibyte,
    // Time (base: second)
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    // Temperature
    Celsius,
    Fahrenheit,
    Kelvin,
}

impl Unit {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mm" | "millimeter" | "millimeters" => Some(Self::Millimeter),
            "cm" | "centimeter" | "centimeters" => Some(Self::Centimeter),
            "m" | "meter" | "meters" => Some(Self::Meter),
            "km" | "kilometer" | "kilometers" => Some(Self::Kilometer),
            "in" | "inch" | "inches" => Some(Self::Inch),
            "ft" | "foot" | "feet" => Some(Self::Foot),
            "yd" | "yard" | "yards" => Some(Self::Yard),
            "mi" | "mile" | "miles" => Some(Self::Mile),

            "mg" | "milligram" | "milligrams" => Some(Self::Milligram),
            "g" | "gram" | "grams" => Some(Self::Gram),
            "kg" | "kilogram" | "kilograms" => Some(Self::Kilogram),
            "oz" | "ounce" | "ounces" => Some(Self::Ounce),
            "lb" | "lbs" | "pound" | "pounds" => Some(Self::Pound),

            "b" | "byte" | "bytes" => Some(Self::Byte),
            "kb" | "kilobyte" | "kilobytes" => Some(Self::Kilobyte),
            "mb" | "megabyte" | "megabytes" => Some(Self::Megabyte),
            "gb" | "gigabyte" | "gigabytes" => Some(Self::Gigabyte),
            "tb" | "terabyte" | "terabytes" => Some(Self::Terabyte),
            "kib" => Some(Self::Kibibyte),
            "mib" => Some(Self::Mebibyte),
            "gib" => Some(Self::Gibibyte),
            "tib" => Some(Self::Tebibyte),

            "ms" | "millisecond" | "milliseconds" => Some(Self::Millisecond),
            "s" | "sec" | "second" | "seconds" => Some(Self::Second),
            "min" | "mins" | "minute" | "minutes" => Some(Self::Minute),
            "h" | "hr" | "hrs" | "hour" | "hours" => Some(Self::Hour),
            "d" | "day" | "days" => Some(Self::Day),

            "c" | "celsius" => Some(Self::Celsius),
            "f" | "fahrenheit" => Some(Self::Fahrenheit),
            "k" | "kelvin" => Some(Self::Kelvin),
            _ => None,
        }
    }

    fn category(self) -> UnitCategory {
        match self {
            Self::Millimeter
            | Self::Centimeter
            | Self::Meter
            | Self::Kilometer
            | Self::Inch
            | Self::Foot
            | Self::Yard
            | Self::Mile => UnitCategory::Length,
            Self::Milligram | Self::Gram | Self::Kilogram | Self::Ounce | Self::Pound => {
                UnitCategory::Mass
            }
            Self::Byte
            | Self::Kilobyte
            | Self::Megabyte
            | Self::Gigabyte
            | Self::Terabyte
            | Self::Kibibyte
            | Self::Mebibyte
            | Self::Gibibyte
            | Self::Tebibyte => UnitCategory::Data,
            Self::Millisecond | Self::Second | Self::Minute | Self::Hour | Self::Day => {
                UnitCategory::Time
            }
            Self::Celsius | Self::Fahrenheit | Self::Kelvin => UnitCategory::Temperature,
        }
    }

    fn canonical_symbol(self) -> &'static str {
        match self {
            Self::Millimeter => "mm",
            Self::Centimeter => "cm",
            Self::Meter => "m",
            Self::Kilometer => "km",
            Self::Inch => "in",
            Self::Foot => "ft",
            Self::Yard => "yd",
            Self::Mile => "mi",
            Self::Milligram => "mg",
            Self::Gram => "g",
            Self::Kilogram => "kg",
            Self::Ounce => "oz",
            Self::Pound => "lb",
            Self::Byte => "B",
            Self::Kilobyte => "KB",
            Self::Megabyte => "MB",
            Self::Gigabyte => "GB",
            Self::Terabyte => "TB",
            Self::Kibibyte => "KiB",
            Self::Mebibyte => "MiB",
            Self::Gibibyte => "GiB",
            Self::Tebibyte => "TiB",
            Self::Millisecond => "ms",
            Self::Second => "s",
            Self::Minute => "min",
            Self::Hour => "h",
            Self::Day => "d",
            Self::Celsius => "C",
            Self::Fahrenheit => "F",
            Self::Kelvin => "K",
        }
    }

    fn linear_factor(self) -> Option<f64> {
        match self {
            Self::Millimeter => Some(0.001),
            Self::Centimeter => Some(0.01),
            Self::Meter => Some(1.0),
            Self::Kilometer => Some(1000.0),
            Self::Inch => Some(0.0254),
            Self::Foot => Some(0.3048),
            Self::Yard => Some(0.9144),
            Self::Mile => Some(1609.344),

            Self::Milligram => Some(1e-6),
            Self::Gram => Some(0.001),
            Self::Kilogram => Some(1.0),
            Self::Ounce => Some(0.028_349_523_125),
            Self::Pound => Some(0.453_592_37),

            Self::Byte => Some(1.0),
            Self::Kilobyte => Some(1_000.0),
            Self::Megabyte => Some(1_000_000.0),
            Self::Gigabyte => Some(1_000_000_000.0),
            Self::Terabyte => Some(1_000_000_000_000.0),
            Self::Kibibyte => Some(1_024.0),
            Self::Mebibyte => Some(1_048_576.0),
            Self::Gibibyte => Some(1_073_741_824.0),
            Self::Tebibyte => Some(1_099_511_627_776.0),

            Self::Millisecond => Some(0.001),
            Self::Second => Some(1.0),
            Self::Minute => Some(60.0),
            Self::Hour => Some(3600.0),
            Self::Day => Some(86_400.0),

            Self::Celsius | Self::Fahrenheit | Self::Kelvin => None,
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_symbol())
    }
}

fn convert_unit(value: f64, from: Unit, to: Unit) -> Result<f64, CalcError> {
    if from.category() != to.category() {
        return Err(CalcError::IncompatibleUnits {
            from: from.canonical_symbol().to_string(),
            to: to.canonical_symbol().to_string(),
        });
    }
    if from == to {
        return Ok(value);
    }

    if from.category() == UnitCategory::Temperature {
        let kelvin = match from {
            Unit::Celsius => value + 273.15,
            Unit::Fahrenheit => (value - 32.0) * (5.0 / 9.0) + 273.15,
            Unit::Kelvin => value,
            _ => unreachable!(),
        };
        let target = match to {
            Unit::Celsius => kelvin - 273.15,
            Unit::Fahrenheit => (kelvin - 273.15) * (9.0 / 5.0) + 32.0,
            Unit::Kelvin => kelvin,
            _ => unreachable!(),
        };
        return Ok(target);
    }

    let (Some(from_factor), Some(to_factor)) = (from.linear_factor(), to.linear_factor()) else {
        return Err(CalcError::IncompatibleUnits {
            from: from.canonical_symbol().to_string(),
            to: to.canonical_symbol().to_string(),
        });
    };

    Ok(value * from_factor / to_factor)
}

fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

fn format_number(value: f64, precision: usize) -> String {
    let clean = normalize_zero(value);
    if clean.abs() >= 1e15 || (clean != 0.0 && clean.abs() < 1e-9) {
        return format!("{clean:.precision$e}")
            .trim_end_matches('0')
            .to_string();
    }

    let rounded = {
        let factor = 10_f64.powi(precision as i32);
        (clean * factor).round() / factor
    };
    let s = format!("{rounded:.precision$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn build_base_alternates(value: f64) -> Vec<CalcAlternate> {
    if value.fract() != 0.0 || value < 0.0 || value > (u64::MAX as f64) {
        return Vec::new();
    }
    let int_val = value as u64;
    vec![
        CalcAlternate {
            label: "hex".to_string(),
            value: format!("0x{int_val:X}"),
        },
        CalcAlternate {
            label: "bin".to_string(),
            value: format!("0b{int_val:b}"),
        },
        CalcAlternate {
            label: "oct".to_string(),
            value: format!("0o{int_val:o}"),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_precedence_functions_and_bases() {
        let calc = OmniboxCalculator::default();

        let res = calc.evaluate("2 + 3 * 4^2 - (10 / 2)").unwrap();
        assert_eq!(res.value, 45.0);
        assert_eq!(res.formatted, "45");
        assert_eq!(res.kind, CalcResultKind::Arithmetic);
        assert!(res
            .alternates
            .iter()
            .any(|a| a.label == "hex" && a.value == "0x2D"));

        let func_res = calc
            .evaluate("sqrt(144) + log10(1000) + max(2, 9, 4)")
            .unwrap();
        assert_eq!(func_res.value, 24.0);

        let base_res = calc.evaluate("0xff + 0b1010").unwrap();
        assert_eq!(base_res.value, 265.0);
    }

    #[test]
    fn percentage_and_unit_conversions() {
        let calc = OmniboxCalculator::default();

        let pct = calc.evaluate("15% of 200").unwrap();
        assert_eq!(pct.value, 30.0);
        assert_eq!(pct.kind, CalcResultKind::Percentage);

        let temp = calc.evaluate("212 f to c").unwrap();
        assert!((temp.value - 100.0).abs() < 1e-9);
        assert_eq!(temp.formatted, "100 C");
        assert_eq!(temp.unit.as_deref(), Some("C"));
        assert_eq!(temp.kind, CalcResultKind::UnitConversion);

        let data = calc.evaluate("2 gib to mib").unwrap();
        assert_eq!(data.value, 2048.0);
        assert_eq!(data.formatted, "2048 MiB");
    }

    #[test]
    fn validation_errors_and_try_suggest_filtering() {
        let calc = OmniboxCalculator::default();

        // Plain search queries or single plain numbers should not trigger omnibox suggestions.
        assert!(calc.try_suggest("rust async programming").is_none());
        assert!(calc.try_suggest("42").is_none());
        assert!(calc.try_suggest("=42").is_some());

        // Division by zero & domain errors.
        assert_eq!(calc.evaluate("10 / 0"), Err(CalcError::DivisionByZero));
        assert!(matches!(
            calc.evaluate("sqrt(-1)"),
            Err(CalcError::DomainError(_))
        ));
        assert!(matches!(
            calc.evaluate("10 km to kg"),
            Err(CalcError::IncompatibleUnits { .. })
        ));

        // Depth limit & invalid config.
        let strict = OmniboxCalculator::new(CalcConfig {
            max_depth: 2,
            ..CalcConfig::default()
        })
        .unwrap();
        assert!(matches!(
            strict.evaluate("(((1 + 2)))"),
            Err(CalcError::DepthLimitExceeded(2))
        ));
        assert!(OmniboxCalculator::new(CalcConfig {
            precision: 0,
            ..CalcConfig::default()
        })
        .is_err());

        // Serialization roundtrip.
        let res = calc.evaluate("1 + 2").unwrap();
        let json = serde_json::to_string(&res).unwrap();
        let decoded: CalcResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, res);
    }
}
