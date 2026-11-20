//! Password entropy scoring, pattern penalty detection, and policy validation.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize;

/// Errors returned when validating inputs or scoring a password.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum EntropyError {
    #[error("password must not be empty")]
    EmptyPassword,
    #[error("password length {len} exceeds maximum allowed length {max}")]
    TooLong { len: usize, max: usize },
    #[error("invalid entropy policy: {0}")]
    InvalidPolicy(String),
}

/// Qualitative password strength tier derived from effective entropy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthRating {
    VeryWeak,
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

/// Breakdown of character classes and pool size observed in a password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterPool {
    pub length_chars: usize,
    pub unique_chars: usize,
    pub lowercase_count: usize,
    pub uppercase_count: usize,
    pub digit_count: usize,
    pub symbol_count: usize,
    pub whitespace_count: usize,
    pub unicode_other_count: usize,
    pub pool_size: u32,
}

/// Category of structural weakness detected in a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternPenaltyKind {
    RepeatedChars,
    SequentialRun,
    KeyboardWalk,
    CommonPassword,
    YearPattern,
    LowUniqueRatio,
}

/// Individual entropy deduction applied for a detected weak pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternPenalty {
    pub kind: PatternPenaltyKind,
    pub description: String,
    pub entropy_deduction_bits: f64,
}

/// Estimated time to exhaustively guess the password at a given guess rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrackTimeEstimate {
    pub guesses_per_second: f64,
    pub estimated_seconds: f64,
    pub human_readable: String,
}

/// Configurable policy for password strength evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntropyPolicy {
    pub min_length: usize,
    pub max_length: usize,
    pub min_entropy_bits: f64,
    pub require_lowercase: bool,
    pub require_uppercase: bool,
    pub require_digits: bool,
    pub require_symbols: bool,
    pub disallow_common_passwords: bool,
    pub offline_guesses_per_sec: f64,
    pub online_guesses_per_sec: f64,
}

impl Default for EntropyPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            max_length: 1024,
            min_entropy_bits: 50.0,
            require_lowercase: false,
            require_uppercase: false,
            require_digits: false,
            require_symbols: false,
            disallow_common_passwords: true,
            offline_guesses_per_sec: 1.0e10,
            online_guesses_per_sec: 10.0,
        }
    }
}

impl EntropyPolicy {
    /// Validates that policy bounds and guess rates are well-formed.
    pub fn validate(&self) -> Result<(), EntropyError> {
        if self.max_length == 0 {
            return Err(EntropyError::InvalidPolicy(
                "max_length must be greater than 0".to_string(),
            ));
        }
        if self.min_length > self.max_length {
            return Err(EntropyError::InvalidPolicy(
                "min_length must not exceed max_length".to_string(),
            ));
        }
        if !self.min_entropy_bits.is_finite() || self.min_entropy_bits < 0.0 {
            return Err(EntropyError::InvalidPolicy(
                "min_entropy_bits must be a non-negative finite number".to_string(),
            ));
        }
        if !self.offline_guesses_per_sec.is_finite() || self.offline_guesses_per_sec <= 0.0 {
            return Err(EntropyError::InvalidPolicy(
                "offline_guesses_per_sec must be positive and finite".to_string(),
            ));
        }
        if !self.online_guesses_per_sec.is_finite() || self.online_guesses_per_sec <= 0.0 {
            return Err(EntropyError::InvalidPolicy(
                "online_guesses_per_sec must be positive and finite".to_string(),
            ));
        }
        Ok(())
    }
}

/// Complete entropy and policy assessment for a password.
///
/// By design, this struct never retains the plaintext password.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntropyReport {
    pub length_chars: usize,
    pub character_pool: CharacterPool,
    pub raw_entropy_bits: f64,
    pub shannon_entropy_bits: f64,
    pub effective_entropy_bits: f64,
    pub score_0_to_100: u8,
    pub rating: StrengthRating,
    pub penalties: Vec<PatternPenalty>,
    pub crack_time_offline_fast: CrackTimeEstimate,
    pub crack_time_online_throttled: CrackTimeEstimate,
    pub meets_policy: bool,
    pub policy_violations: Vec<String>,
    pub suggestions: Vec<String>,
}

/// Transient wrapper that zeroes its password buffer on drop.
#[derive(Debug, Clone)]
pub struct ZeroizingPassword {
    inner: String,
}

impl ZeroizingPassword {
    /// Creates a new zeroizing password wrapper.
    pub fn new(password: String) -> Self {
        Self { inner: password }
    }

    /// Returns a borrowed view of the password for scoring.
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl Drop for ZeroizingPassword {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

const COMMON_PASSWORDS: &[&str] = &[
    "password",
    "123456",
    "12345678",
    "123456789",
    "1234567890",
    "qwerty",
    "abc123",
    "letmein",
    "admin",
    "welcome",
    "iloveyou",
    "monkey",
    "dragon",
    "passw0rd",
    "password123",
    "root",
    "toor",
    "guest",
    "default",
    "changeme",
    "sunshine",
    "princess",
    "football",
    "baseball",
];

const KEYBOARD_ROWS: &[&str] = &["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"];

/// Scores a password against the given policy without retaining plaintext.
pub fn score_password(
    password: &str,
    policy: &EntropyPolicy,
) -> Result<EntropyReport, EntropyError> {
    policy.validate()?;

    let char_count = password.chars().count();
    if char_count == 0 {
        return Err(EntropyError::EmptyPassword);
    }
    if char_count > policy.max_length {
        return Err(EntropyError::TooLong {
            len: char_count,
            max: policy.max_length,
        });
    }

    let character_pool = analyze_character_pool(password);
    let raw_entropy_bits = compute_raw_entropy(&character_pool);
    let shannon_entropy_bits = compute_shannon_entropy(password, char_count);
    let penalties = detect_penalties(password, &character_pool);

    let effective_entropy_bits =
        compute_effective_entropy(raw_entropy_bits, &character_pool, &penalties);

    let score_0_to_100 = ((effective_entropy_bits / 100.0) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    let rating = classify_strength(effective_entropy_bits);

    let crack_time_offline_fast =
        estimate_crack_time(effective_entropy_bits, policy.offline_guesses_per_sec);
    let crack_time_online_throttled =
        estimate_crack_time(effective_entropy_bits, policy.online_guesses_per_sec);

    let is_common_exact = is_exact_common_password(password);
    let policy_violations = evaluate_policy_violations(
        &character_pool,
        effective_entropy_bits,
        is_common_exact,
        policy,
    );
    let meets_policy = policy_violations.is_empty();
    let suggestions = build_suggestions(&character_pool, &penalties, &policy_violations, policy);

    Ok(EntropyReport {
        length_chars: char_count,
        character_pool,
        raw_entropy_bits,
        shannon_entropy_bits,
        effective_entropy_bits,
        score_0_to_100,
        rating,
        penalties,
        crack_time_offline_fast,
        crack_time_online_throttled,
        meets_policy,
        policy_violations,
        suggestions,
    })
}

fn analyze_character_pool(password: &str) -> CharacterPool {
    let mut lowercase_count = 0usize;
    let mut uppercase_count = 0usize;
    let mut digit_count = 0usize;
    let mut symbol_count = 0usize;
    let mut whitespace_count = 0usize;
    let mut unicode_other_count = 0usize;
    let mut unique = HashSet::new();
    let mut length_chars = 0usize;

    for ch in password.chars() {
        length_chars += 1;
        unique.insert(ch);
        if ch.is_ascii_lowercase() {
            lowercase_count += 1;
        } else if ch.is_ascii_uppercase() {
            uppercase_count += 1;
        } else if ch.is_ascii_digit() {
            digit_count += 1;
        } else if ch.is_ascii_whitespace() {
            whitespace_count += 1;
        } else if ch.is_ascii_punctuation() {
            symbol_count += 1;
        } else {
            unicode_other_count += 1;
        }
    }

    let mut pool_size = 0u32;
    if lowercase_count > 0 {
        pool_size += 26;
    }
    if uppercase_count > 0 {
        pool_size += 26;
    }
    if digit_count > 0 {
        pool_size += 10;
    }
    if symbol_count > 0 {
        pool_size += 32;
    }
    if whitespace_count > 0 {
        pool_size += 1;
    }
    if unicode_other_count > 0 {
        pool_size += 64;
    }

    CharacterPool {
        length_chars,
        unique_chars: unique.len(),
        lowercase_count,
        uppercase_count,
        digit_count,
        symbol_count,
        whitespace_count,
        unicode_other_count,
        pool_size,
    }
}

fn compute_raw_entropy(pool: &CharacterPool) -> f64 {
    if pool.length_chars == 0 || pool.pool_size <= 1 {
        return 0.0;
    }
    (pool.length_chars as f64) * (pool.pool_size as f64).log2()
}

fn compute_shannon_entropy(password: &str, length_chars: usize) -> f64 {
    if length_chars <= 1 {
        return 0.0;
    }
    let mut counts: HashMap<char, usize> = HashMap::new();
    for ch in password.chars() {
        *counts.entry(ch).or_insert(0) += 1;
    }
    let total = length_chars as f64;
    let mut per_char_entropy = 0.0_f64;
    for count in counts.values() {
        let p = (*count as f64) / total;
        if p > 0.0 {
            per_char_entropy -= p * p.log2();
        }
    }
    per_char_entropy * total
}

fn detect_penalties(password: &str, pool: &CharacterPool) -> Vec<PatternPenalty> {
    let mut penalties = Vec::new();
    let chars: Vec<char> = password.chars().collect();

    // 1. Repeated identical characters (runs >= 3)
    let mut i = 0;
    while i < chars.len() {
        let mut j = i + 1;
        while j < chars.len() && chars[j] == chars[i] {
            j += 1;
        }
        let run_len = j - i;
        if run_len >= 3 {
            penalties.push(PatternPenalty {
                kind: PatternPenaltyKind::RepeatedChars,
                description: format!("repeated character run of length {run_len}"),
                entropy_deduction_bits: ((run_len - 2) as f64) * 4.0,
            });
        }
        i = j;
    }

    // 2. Ascending or descending ASCII sequential runs (length >= 3)
    let seq_deduction = detect_sequential_runs(&chars);
    if seq_deduction > 0.0 {
        penalties.push(PatternPenalty {
            kind: PatternPenaltyKind::SequentialRun,
            description: "sequential alphanumeric characters detected".to_string(),
            entropy_deduction_bits: seq_deduction,
        });
    }

    // 3. Keyboard walks (length >= 4)
    let lower = password.to_ascii_lowercase();
    if has_keyboard_walk(&lower) {
        penalties.push(PatternPenalty {
            kind: PatternPenaltyKind::KeyboardWalk,
            description: "keyboard walk pattern detected".to_string(),
            entropy_deduction_bits: 10.0,
        });
    }

    // 4. Common password exact or substring match
    if is_exact_common_password(&lower) {
        penalties.push(PatternPenalty {
            kind: PatternPenaltyKind::CommonPassword,
            description: "matches a commonly used password".to_string(),
            entropy_deduction_bits: 40.0,
        });
    } else if COMMON_PASSWORDS
        .iter()
        .any(|w| w.len() >= 5 && lower.contains(w))
    {
        penalties.push(PatternPenalty {
            kind: PatternPenaltyKind::CommonPassword,
            description: "contains a commonly used password substring".to_string(),
            entropy_deduction_bits: 15.0,
        });
    }

    // 5. Four-digit year pattern (1900..=2099)
    if contains_year_pattern(&chars) {
        penalties.push(PatternPenalty {
            kind: PatternPenaltyKind::YearPattern,
            description: "contains a 4-digit year pattern".to_string(),
            entropy_deduction_bits: 8.0,
        });
    }

    // 6. Low unique-character ratio on longer passwords
    if pool.length_chars >= 6 {
        let ratio = (pool.unique_chars as f64) / (pool.length_chars as f64);
        if ratio < 0.5 {
            penalties.push(PatternPenalty {
                kind: PatternPenaltyKind::LowUniqueRatio,
                description: "low ratio of unique characters".to_string(),
                entropy_deduction_bits: 10.0,
            });
        }
    }

    penalties
}

fn detect_sequential_runs(chars: &[char]) -> f64 {
    if chars.len() < 3 {
        return 0.0;
    }
    let mut total_deduction = 0.0_f64;
    let mut i = 0;
    while i + 2 < chars.len() {
        let a = chars[i];
        let b = chars[i + 1];
        if a.is_ascii_alphanumeric() && b.is_ascii_alphanumeric() {
            let diff = (b as i32) - (a as i32);
            if diff == 1 || diff == -1 {
                let mut run_len = 2usize;
                let mut k = i + 2;
                while k < chars.len() && chars[k].is_ascii_alphanumeric() {
                    let prev = chars[k - 1] as i32;
                    let curr = chars[k] as i32;
                    if curr - prev == diff {
                        run_len += 1;
                        k += 1;
                    } else {
                        break;
                    }
                }
                if run_len >= 3 {
                    total_deduction += ((run_len - 2) as f64) * 3.5;
                    i = k;
                    continue;
                }
            }
        }
        i += 1;
    }
    total_deduction
}

fn has_keyboard_walk(lower_password: &str) -> bool {
    for row in KEYBOARD_ROWS {
        let bytes = row.as_bytes();
        if bytes.len() < 4 {
            continue;
        }
        for win in bytes.windows(4) {
            if let Ok(sub) = std::str::from_utf8(win) {
                if lower_password.contains(sub) {
                    return true;
                }
                let rev: String = sub.chars().rev().collect();
                if lower_password.contains(&rev) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_exact_common_password(password: &str) -> bool {
    let lower = password.to_ascii_lowercase();
    COMMON_PASSWORDS.contains(&lower.as_str())
}

fn contains_year_pattern(chars: &[char]) -> bool {
    if chars.len() < 4 {
        return false;
    }
    for win in chars.windows(4) {
        if win.iter().all(|c| c.is_ascii_digit()) {
            let s: String = win.iter().collect();
            if let Ok(year) = s.parse::<u32>() {
                if (1900..=2099).contains(&year) {
                    return true;
                }
            }
        }
    }
    false
}

fn compute_effective_entropy(
    raw_entropy_bits: f64,
    pool: &CharacterPool,
    penalties: &[PatternPenalty],
) -> f64 {
    if pool.length_chars == 0 {
        return 0.0;
    }
    let unique_ratio = (pool.unique_chars as f64) / (pool.length_chars as f64);
    let base = if pool.unique_chars <= 1 {
        (pool.pool_size.max(1) as f64).log2()
    } else {
        raw_entropy_bits * (0.35 + 0.65 * unique_ratio.sqrt())
    };
    let total_deduction: f64 = penalties.iter().map(|p| p.entropy_deduction_bits).sum();
    (base - total_deduction).max(0.0)
}

fn classify_strength(effective_entropy_bits: f64) -> StrengthRating {
    if effective_entropy_bits < 28.0 {
        StrengthRating::VeryWeak
    } else if effective_entropy_bits < 45.0 {
        StrengthRating::Weak
    } else if effective_entropy_bits < 60.0 {
        StrengthRating::Fair
    } else if effective_entropy_bits < 80.0 {
        StrengthRating::Strong
    } else {
        StrengthRating::VeryStrong
    }
}

fn estimate_crack_time(effective_entropy_bits: f64, guesses_per_second: f64) -> CrackTimeEstimate {
    let clamped_bits = effective_entropy_bits.clamp(0.0, 128.0);
    // Average guesses is half the keyspace: 2^(H - 1), floored at 1.0
    let avg_guesses = (2.0_f64.powf(clamped_bits) * 0.5).max(1.0);
    let estimated_seconds = avg_guesses / guesses_per_second;
    CrackTimeEstimate {
        guesses_per_second,
        estimated_seconds,
        human_readable: format_duration(estimated_seconds),
    }
}

fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds >= 3.1536e11 {
        return "centuries+".to_string();
    }
    if seconds < 0.001 {
        return "instant".to_string();
    }
    if seconds < 1.0 {
        return "< 1 second".to_string();
    }
    if seconds < 60.0 {
        return format!("{} seconds", seconds.round() as u64);
    }
    let minutes = seconds / 60.0;
    if minutes < 60.0 {
        return format!("{} minutes", minutes.round() as u64);
    }
    let hours = minutes / 60.0;
    if hours < 24.0 {
        return format!("{} hours", hours.round() as u64);
    }
    let days = hours / 24.0;
    if days < 365.0 {
        return format!("{} days", days.round() as u64);
    }
    let years = days / 365.0;
    if years < 100.0 {
        return format!("{} years", years.round() as u64);
    }
    "centuries+".to_string()
}

fn evaluate_policy_violations(
    pool: &CharacterPool,
    effective_entropy_bits: f64,
    is_common_exact: bool,
    policy: &EntropyPolicy,
) -> Vec<String> {
    let mut violations = Vec::new();
    if pool.length_chars < policy.min_length {
        violations.push(format!(
            "password must be at least {} characters (got {})",
            policy.min_length, pool.length_chars
        ));
    }
    if effective_entropy_bits < policy.min_entropy_bits {
        violations.push(format!(
            "effective entropy {:.1} bits is below required {:.1} bits",
            effective_entropy_bits, policy.min_entropy_bits
        ));
    }
    if policy.require_lowercase && pool.lowercase_count == 0 {
        violations.push("must contain at least one lowercase letter".to_string());
    }
    if policy.require_uppercase && pool.uppercase_count == 0 {
        violations.push("must contain at least one uppercase letter".to_string());
    }
    if policy.require_digits && pool.digit_count == 0 {
        violations.push("must contain at least one digit".to_string());
    }
    if policy.require_symbols && pool.symbol_count == 0 {
        violations.push("must contain at least one symbol".to_string());
    }
    if policy.disallow_common_passwords && is_common_exact {
        violations.push("must not be a commonly used password".to_string());
    }
    violations
}

fn build_suggestions(
    pool: &CharacterPool,
    penalties: &[PatternPenalty],
    violations: &[String],
    policy: &EntropyPolicy,
) -> Vec<String> {
    let mut suggestions = Vec::new();
    if pool.length_chars < policy.min_length.max(12) {
        suggestions.push("Use a longer password or passphrase (12+ characters).".to_string());
    }
    if pool.lowercase_count == 0 || pool.uppercase_count == 0 {
        suggestions.push("Mix uppercase and lowercase letters.".to_string());
    }
    if pool.digit_count == 0 || pool.symbol_count == 0 {
        suggestions.push("Include digits and symbols to expand the character pool.".to_string());
    }
    for p in penalties {
        match p.kind {
            PatternPenaltyKind::RepeatedChars => {
                suggestions.push("Avoid repeating the same character multiple times.".to_string());
            }
            PatternPenaltyKind::SequentialRun => {
                suggestions
                    .push("Avoid sequential letters or numbers like abc or 123.".to_string());
            }
            PatternPenaltyKind::KeyboardWalk => {
                suggestions.push("Avoid keyboard patterns like qwerty or asdf.".to_string());
            }
            PatternPenaltyKind::CommonPassword => {
                suggestions
                    .push("Avoid common passwords or well-known dictionary words.".to_string());
            }
            PatternPenaltyKind::YearPattern => {
                suggestions.push("Avoid predictable 4-digit years or dates.".to_string());
            }
            PatternPenaltyKind::LowUniqueRatio => {
                suggestions
                    .push("Use more distinct characters instead of repeating a few.".to_string());
            }
        }
    }
    if suggestions.is_empty() && !violations.is_empty() {
        suggestions.push("Increase password complexity to satisfy all policy rules.".to_string());
    }
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_oversized_passwords_and_invalid_policies() {
        let policy = EntropyPolicy::default();
        assert_eq!(
            score_password("", &policy),
            Err(EntropyError::EmptyPassword)
        );

        let too_long = "a".repeat(1025);
        assert_eq!(
            score_password(&too_long, &policy),
            Err(EntropyError::TooLong {
                len: 1025,
                max: 1024
            })
        );

        let bad_policy = EntropyPolicy {
            min_length: 20,
            max_length: 10,
            ..EntropyPolicy::default()
        };
        assert!(matches!(
            score_password("ValidPass123!", &bad_policy),
            Err(EntropyError::InvalidPolicy(_))
        ));
    }

    #[test]
    fn penalizes_common_and_patterned_passwords() {
        let policy = EntropyPolicy::default();

        let common = score_password("password", &policy).unwrap();
        assert_eq!(common.rating, StrengthRating::VeryWeak);
        assert!(!common.meets_policy);
        assert!(common
            .penalties
            .iter()
            .any(|p| p.kind == PatternPenaltyKind::CommonPassword));

        let repeated = score_password("aaaaaaaaaaaa", &policy).unwrap();
        assert_eq!(repeated.rating, StrengthRating::VeryWeak);
        assert!(repeated
            .penalties
            .iter()
            .any(|p| p.kind == PatternPenaltyKind::RepeatedChars));

        let walk_and_year = score_password("qwerty1999abc", &policy).unwrap();
        assert!(walk_and_year
            .penalties
            .iter()
            .any(|p| p.kind == PatternPenaltyKind::KeyboardWalk));
        assert!(walk_and_year
            .penalties
            .iter()
            .any(|p| p.kind == PatternPenaltyKind::YearPattern));
        assert!(walk_and_year
            .penalties
            .iter()
            .any(|p| p.kind == PatternPenaltyKind::SequentialRun));
    }

    #[test]
    fn scores_strong_high_entropy_password_and_meets_strict_policy() {
        let strict_policy = EntropyPolicy {
            min_length: 12,
            require_lowercase: true,
            require_uppercase: true,
            require_digits: true,
            require_symbols: true,
            min_entropy_bits: 70.0,
            ..EntropyPolicy::default()
        };

        let secret = ZeroizingPassword::new("K9#mV2!xP7$qL4@z".to_string());
        let report = score_password(secret.as_str(), &strict_policy).unwrap();

        assert!(
            report.meets_policy,
            "violations: {:?}",
            report.policy_violations
        );
        assert!(report.effective_entropy_bits >= 80.0);
        assert_eq!(report.rating, StrengthRating::VeryStrong);
        assert!(report.score_0_to_100 >= 80);
        assert_eq!(report.crack_time_offline_fast.human_readable, "centuries+");
    }

    #[test]
    fn serializes_and_deserializes_report_without_plaintext() {
        let policy = EntropyPolicy::default();
        let report = score_password("T7!vR9#nB2@kQ5", &policy).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("T7!vR9#nB2@kQ5"));

        let decoded: EntropyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.length_chars, report.length_chars);
        assert_eq!(decoded.score_0_to_100, report.score_0_to_100);
        assert_eq!(decoded.rating, report.rating);
        assert!((decoded.raw_entropy_bits - report.raw_entropy_bits).abs() < 1e-12);
        assert!((decoded.effective_entropy_bits - report.effective_entropy_bits).abs() < 1e-12);
    }
}
