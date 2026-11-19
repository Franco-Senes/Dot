//! Cryptographically secure password and passphrase generator for Search Linux.
//!
//! Uses `ring::rand::SystemRandom` with unbiased rejection sampling and Fisher-Yates
//! shuffling, and zeroizes generated secret material on drop via `zeroize`.

use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize;

/// Minimum allowed password length.
pub const MIN_PASSWORD_LENGTH: usize = 4;
/// Maximum allowed password length.
pub const MAX_PASSWORD_LENGTH: usize = 1024;
/// Minimum allowed passphrase word count.
pub const MIN_PASSPHRASE_WORDS: usize = 3;
/// Maximum allowed passphrase word count.
pub const MAX_PASSPHRASE_WORDS: usize = 64;
/// Maximum batch size for bulk generation.
pub const MAX_BATCH_COUNT: usize = 256;

const UPPERCASE_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE_CHARS: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGIT_CHARS: &str = "0123456789";
const DEFAULT_SYMBOL_CHARS: &str = "!@#$%^&*()-_=+[]{}|;:,.<>?/~";
const AMBIGUOUS_CHARS: &[char] = &['0', 'O', 'o', '1', 'l', 'I', '|', '`', '\'', '"'];

/// Curated built-in wordlist for offline passphrase generation (128 distinct words).
const BUILTIN_WORDLIST: &[&str] = &[
    "acorn", "anchor", "apple", "apron", "arbor", "arrow", "aspen", "atlas", "aurora", "autumn",
    "badger", "bamboo", "banjo", "barrel", "basalt", "beacon", "beaver", "birch", "bison", "bloom",
    "boulder", "breeze", "bridge", "bronze", "brook", "canyon", "carbon", "cedar", "chalet",
    "cherry", "cinder", "clover", "cobalt", "comet", "compass", "copper", "coral", "cougar",
    "crater", "creek", "crimson", "crystal", "cypress", "dagger", "daisy", "delta", "desert",
    "dolphin", "dragon", "drift", "eagle", "echo", "ember", "emerald", "falcon", "fennel", "fern",
    "fjord", "flint", "forest", "fossil", "fox", "frost", "galaxy", "garden", "garnet", "geyser",
    "glacier", "granite", "grove", "harbor", "harvest", "hawk", "hazel", "heather", "helmet",
    "heron", "horizon", "indigo", "island", "ivory", "jackal", "jade", "jasper", "juniper",
    "kayak", "kelp", "kestrel", "kettle", "lantern", "larch", "laurel", "ledge", "lichen", "lilac",
    "linden", "lotus", "lunar", "lynx", "magnet", "maple", "marble", "meadow", "meteor", "mineral",
    "mirror", "monarch", "monolith", "moss", "nebula", "nectar", "nettle", "nimbus", "nutmeg",
    "oak", "obsidian", "ochre", "olive", "onyx", "opal", "orchid", "osprey", "otter", "summit",
    "timber", "topaz", "valley", "zephyr",
];

/// Errors produced during policy validation or CSPRNG password generation.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasswordGenError {
    #[error("password length {requested} is below minimum {min}")]
    LengthTooShort { min: usize, requested: usize },

    #[error("password length {requested} exceeds maximum {max}")]
    LengthTooLong { max: usize, requested: usize },

    #[error("effective character pool is empty; enable at least one character class")]
    EmptyCharset,

    #[error(
        "password length {length} is smaller than required distinct character classes ({required_classes})"
    )]
    InsufficientLengthForClasses {
        required_classes: usize,
        length: usize,
    },

    #[error("passphrase word count {requested} must be between {min} and {max}")]
    WordCountOutOfRange {
        min: usize,
        max: usize,
        requested: usize,
    },

    #[error("wordlist is empty or contains only blank entries")]
    EmptyWordlist,

    #[error("custom symbol set must not be empty or whitespace-only when provided")]
    InvalidCustomSymbols,

    #[error("batch count {requested} must be between 1 and {max}")]
    InvalidBatchCount { max: usize, requested: usize },

    #[error(
        "minimum entropy requirement ({required_bits:.1} bits) not met (got {actual_bits:.1} bits)"
    )]
    InsufficientEntropy {
        required_bits: String,
        actual_bits: String,
    },

    #[error("system CSPRNG failed to produce random bytes")]
    CsprngFailure,
}

/// Qualitative strength classification based on estimated Shannon entropy bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntropyStrength {
    VeryWeak,
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

/// Configuration policy for character-based password generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PasswordPolicy {
    pub length: usize,
    pub include_uppercase: bool,
    pub include_lowercase: bool,
    pub include_digits: bool,
    pub include_symbols: bool,
    pub exclude_ambiguous: bool,
    pub custom_symbols: Option<String>,
    pub require_each_enabled_class: bool,
    pub min_entropy_bits: Option<f64>,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            length: 20,
            include_uppercase: true,
            include_lowercase: true,
            include_digits: true,
            include_symbols: true,
            exclude_ambiguous: false,
            custom_symbols: None,
            require_each_enabled_class: true,
            min_entropy_bits: None,
        }
    }
}

/// Configuration policy for wordlist-based passphrase generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassphrasePolicy {
    pub word_count: usize,
    pub separator: String,
    pub capitalize: bool,
    pub append_digit: bool,
}

impl Default for PassphrasePolicy {
    fn default() -> Self {
        Self {
            word_count: 6,
            separator: "-".to_string(),
            capitalize: false,
            append_digit: false,
        }
    }
}

/// Non-secret metadata describing a generated password or passphrase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PasswordMetadata {
    pub length: usize,
    pub pool_size: usize,
    pub entropy_bits: f64,
    pub strength: EntropyStrength,
    pub is_passphrase: bool,
}

/// Generated secret container that zeroizes its internal buffer on drop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSecret {
    value: String,
    pub metadata: PasswordMetadata,
}

impl GeneratedSecret {
    /// Returns the generated secret string slice.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Consumes the container and returns the inner `String` without zeroizing it.
    /// The caller assumes responsibility for clearing the returned string.
    pub fn into_inner(mut self) -> String {
        std::mem::take(&mut self.value)
    }
}

impl Drop for GeneratedSecret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

/// Abstraction over a cryptographically secure random byte source.
pub trait CsprngSource {
    fn fill_bytes(&self, dest: &mut [u8]) -> Result<(), PasswordGenError>;
}

/// Production CSPRNG backed by `ring::rand::SystemRandom`.
#[derive(Debug, Clone)]
pub struct SystemRandomCsprng {
    rng: SystemRandom,
}

impl SystemRandomCsprng {
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
        }
    }
}

impl Default for SystemRandomCsprng {
    fn default() -> Self {
        Self::new()
    }
}

impl CsprngSource for SystemRandomCsprng {
    fn fill_bytes(&self, dest: &mut [u8]) -> Result<(), PasswordGenError> {
        self.rng
            .fill(dest)
            .map_err(|_| PasswordGenError::CsprngFailure)
    }
}

/// Resolved character classes and combined deduplicated pool.
#[derive(Debug, Clone)]
struct CharsetPool {
    classes: Vec<Vec<char>>,
    combined: Vec<char>,
}

/// Cryptographically secure password and passphrase generator.
pub struct PasswordGenerator<R: CsprngSource = SystemRandomCsprng> {
    rng: R,
}

impl Default for PasswordGenerator<SystemRandomCsprng> {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordGenerator<SystemRandomCsprng> {
    /// Creates a new generator backed by `ring::rand::SystemRandom`.
    pub fn new() -> Self {
        Self {
            rng: SystemRandomCsprng::new(),
        }
    }
}

impl<R: CsprngSource> PasswordGenerator<R> {
    /// Creates a generator with a custom `CsprngSource` implementation.
    pub fn with_rng(rng: R) -> Self {
        Self { rng }
    }

    /// Validates a `PasswordPolicy` without generating a secret.
    pub fn validate_policy(&self, policy: &PasswordPolicy) -> Result<(), PasswordGenError> {
        if policy.length < MIN_PASSWORD_LENGTH {
            return Err(PasswordGenError::LengthTooShort {
                min: MIN_PASSWORD_LENGTH,
                requested: policy.length,
            });
        }
        if policy.length > MAX_PASSWORD_LENGTH {
            return Err(PasswordGenError::LengthTooLong {
                max: MAX_PASSWORD_LENGTH,
                requested: policy.length,
            });
        }

        let pool = build_charset_pool(policy)?;
        if policy.require_each_enabled_class && policy.length < pool.classes.len() {
            return Err(PasswordGenError::InsufficientLengthForClasses {
                required_classes: pool.classes.len(),
                length: policy.length,
            });
        }

        if let Some(min_bits) = policy.min_entropy_bits {
            let actual_bits = estimate_entropy_bits(policy.length, pool.combined.len());
            if actual_bits < min_bits {
                return Err(PasswordGenError::InsufficientEntropy {
                    required_bits: format!("{min_bits:.1}"),
                    actual_bits: format!("{actual_bits:.1}"),
                });
            }
        }

        Ok(())
    }

    /// Generates a single password matching `policy`.
    pub fn generate_password(
        &self,
        policy: &PasswordPolicy,
    ) -> Result<GeneratedSecret, PasswordGenError> {
        self.validate_policy(policy)?;
        let pool = build_charset_pool(policy)?;

        let mut chars: Vec<char> = Vec::with_capacity(policy.length);

        if policy.require_each_enabled_class {
            for class in &pool.classes {
                let idx = random_index(&self.rng, class.len())?;
                let ch = class
                    .get(idx)
                    .copied()
                    .ok_or(PasswordGenError::EmptyCharset)?;
                chars.push(ch);
            }
        }

        while chars.len() < policy.length {
            let idx = random_index(&self.rng, pool.combined.len())?;
            let ch = pool
                .combined
                .get(idx)
                .copied()
                .ok_or(PasswordGenError::EmptyCharset)?;
            chars.push(ch);
        }

        shuffle_slice(&self.rng, &mut chars)?;

        let value: String = chars.iter().collect();
        chars.zeroize();

        let entropy_bits = estimate_entropy_bits(policy.length, pool.combined.len());
        let metadata = PasswordMetadata {
            length: policy.length,
            pool_size: pool.combined.len(),
            entropy_bits,
            strength: classify_strength(entropy_bits),
            is_passphrase: false,
        };

        Ok(GeneratedSecret { value, metadata })
    }

    /// Generates `count` independent passwords using the same `policy`.
    pub fn generate_batch(
        &self,
        policy: &PasswordPolicy,
        count: usize,
    ) -> Result<Vec<GeneratedSecret>, PasswordGenError> {
        if count == 0 || count > MAX_BATCH_COUNT {
            return Err(PasswordGenError::InvalidBatchCount {
                max: MAX_BATCH_COUNT,
                requested: count,
            });
        }
        self.validate_policy(policy)?;

        let mut results = Vec::with_capacity(count);
        for _ in 0..count {
            results.push(self.generate_password(policy)?);
        }
        Ok(results)
    }

    /// Generates a wordlist-based passphrase using either `custom_wordlist` or the built-in list.
    pub fn generate_passphrase(
        &self,
        policy: &PassphrasePolicy,
        custom_wordlist: Option<&[&str]>,
    ) -> Result<GeneratedSecret, PasswordGenError> {
        if policy.word_count < MIN_PASSPHRASE_WORDS || policy.word_count > MAX_PASSPHRASE_WORDS {
            return Err(PasswordGenError::WordCountOutOfRange {
                min: MIN_PASSPHRASE_WORDS,
                max: MAX_PASSPHRASE_WORDS,
                requested: policy.word_count,
            });
        }

        let raw_words = custom_wordlist.unwrap_or(BUILTIN_WORDLIST);
        let words: Vec<&str> = raw_words
            .iter()
            .map(|w| w.trim())
            .filter(|w| !w.is_empty())
            .collect();

        if words.is_empty() {
            return Err(PasswordGenError::EmptyWordlist);
        }

        let mut chosen: Vec<String> = Vec::with_capacity(policy.word_count);
        for _ in 0..policy.word_count {
            let idx = random_index(&self.rng, words.len())?;
            let word = words
                .get(idx)
                .copied()
                .ok_or(PasswordGenError::EmptyWordlist)?;
            if policy.capitalize {
                chosen.push(capitalize_ascii_word(word));
            } else {
                chosen.push(word.to_string());
            }
        }

        let mut value = chosen.join(&policy.separator);
        for w in &mut chosen {
            w.zeroize();
        }

        let mut extra_entropy = 0.0_f64;
        if policy.append_digit {
            let digit = random_index(&self.rng, 10)?;
            value.push_str(&digit.to_string());
            extra_entropy = 10.0_f64.log2();
        }

        let entropy_bits = estimate_entropy_bits(policy.word_count, words.len()) + extra_entropy;
        let metadata = PasswordMetadata {
            length: value.chars().count(),
            pool_size: words.len(),
            entropy_bits,
            strength: classify_strength(entropy_bits),
            is_passphrase: true,
        };

        Ok(GeneratedSecret { value, metadata })
    }
}

/// Estimates Shannon entropy in bits for `length` uniform independent draws from `pool_size`.
pub fn estimate_entropy_bits(length: usize, pool_size: usize) -> f64 {
    if length == 0 || pool_size <= 1 {
        return 0.0;
    }
    (length as f64) * (pool_size as f64).log2()
}

/// Classifies entropy bits into a human-readable strength tier.
pub fn classify_strength(entropy_bits: f64) -> EntropyStrength {
    if entropy_bits < 40.0 {
        EntropyStrength::VeryWeak
    } else if entropy_bits < 60.0 {
        EntropyStrength::Weak
    } else if entropy_bits < 80.0 {
        EntropyStrength::Fair
    } else if entropy_bits < 112.0 {
        EntropyStrength::Strong
    } else {
        EntropyStrength::VeryStrong
    }
}

fn build_charset_pool(policy: &PasswordPolicy) -> Result<CharsetPool, PasswordGenError> {
    let mut classes: Vec<Vec<char>> = Vec::new();

    if policy.include_uppercase {
        push_filtered_class(&mut classes, UPPERCASE_CHARS, policy.exclude_ambiguous);
    }
    if policy.include_lowercase {
        push_filtered_class(&mut classes, LOWERCASE_CHARS, policy.exclude_ambiguous);
    }
    if policy.include_digits {
        push_filtered_class(&mut classes, DIGIT_CHARS, policy.exclude_ambiguous);
    }
    if policy.include_symbols {
        let symbols_src = match &policy.custom_symbols {
            Some(custom) => {
                if custom.trim().is_empty() {
                    return Err(PasswordGenError::InvalidCustomSymbols);
                }
                custom.as_str()
            }
            None => DEFAULT_SYMBOL_CHARS,
        };
        push_filtered_class(&mut classes, symbols_src, policy.exclude_ambiguous);
    }

    let mut combined: Vec<char> = Vec::new();
    for class in &classes {
        for &ch in class {
            if !combined.contains(&ch) {
                combined.push(ch);
            }
        }
    }

    if combined.is_empty() {
        return Err(PasswordGenError::EmptyCharset);
    }

    Ok(CharsetPool { classes, combined })
}

fn push_filtered_class(classes: &mut Vec<Vec<char>>, source: &str, exclude_ambiguous: bool) {
    let mut filtered: Vec<char> = Vec::new();
    for ch in source.chars() {
        if exclude_ambiguous && AMBIGUOUS_CHARS.contains(&ch) {
            continue;
        }
        if !filtered.contains(&ch) {
            filtered.push(ch);
        }
    }
    if !filtered.is_empty() {
        classes.push(filtered);
    }
}

/// Unbiased uniform random index in `[0, bound)` using rejection sampling.
fn random_index<R: CsprngSource>(rng: &R, bound: usize) -> Result<usize, PasswordGenError> {
    if bound == 0 {
        return Err(PasswordGenError::EmptyCharset);
    }
    if bound == 1 {
        return Ok(0);
    }

    let bound_u64 = bound as u64;
    let zone = (u64::MAX / bound_u64) * bound_u64;
    let mut buf = [0u8; 8];

    loop {
        rng.fill_bytes(&mut buf)?;
        let candidate = u64::from_le_bytes(buf);
        if candidate < zone {
            return Ok((candidate % bound_u64) as usize);
        }
    }
}

/// Unbiased Fisher-Yates shuffle over a mutable slice.
fn shuffle_slice<R: CsprngSource, T>(rng: &R, slice: &mut [T]) -> Result<(), PasswordGenError> {
    let len = slice.len();
    if len <= 1 {
        return Ok(());
    }
    for i in (1..len).rev() {
        let j = random_index(rng, i + 1)?;
        slice.swap(i, j);
    }
    Ok(())
}

fn capitalize_ascii_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(word.len());
            for up in first.to_uppercase() {
                out.push(up);
            }
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct DeterministicTestRng {
        state: AtomicU64,
    }

    impl DeterministicTestRng {
        fn new(seed: u64) -> Self {
            Self {
                state: AtomicU64::new(seed),
            }
        }
    }

    impl CsprngSource for DeterministicTestRng {
        fn fill_bytes(&self, dest: &mut [u8]) -> Result<(), PasswordGenError> {
            for chunk in dest.chunks_mut(8) {
                let prev = self.state.fetch_add(1, Ordering::Relaxed);
                let mixed = prev
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let bytes = mixed.to_le_bytes();
                for (dst, src) in chunk.iter_mut().zip(bytes.iter()) {
                    *dst = *src;
                }
            }
            Ok(())
        }
    }

    #[test]
    fn happy_path_default_policy_with_system_csprng() {
        let generator = PasswordGenerator::new();
        let policy = PasswordPolicy::default();
        let secret = generator.generate_password(&policy).unwrap();

        assert_eq!(secret.as_str().chars().count(), 20);
        assert_eq!(secret.metadata.length, 20);
        assert!(!secret.metadata.is_passphrase);
        assert!(secret.metadata.entropy_bits > 100.0);
        assert_eq!(secret.metadata.strength, EntropyStrength::VeryStrong);

        // Verify at least one character from every enabled class is present.
        assert!(secret.as_str().chars().any(|c| c.is_ascii_uppercase()));
        assert!(secret.as_str().chars().any(|c| c.is_ascii_lowercase()));
        assert!(secret.as_str().chars().any(|c| c.is_ascii_digit()));
        assert!(secret
            .as_str()
            .chars()
            .any(|c| DEFAULT_SYMBOL_CHARS.contains(c)));
    }

    #[test]
    fn exclude_ambiguous_and_custom_symbols() {
        let generator = PasswordGenerator::with_rng(DeterministicTestRng::new(42));
        let policy = PasswordPolicy {
            length: 32,
            include_uppercase: true,
            include_lowercase: true,
            include_digits: true,
            include_symbols: true,
            exclude_ambiguous: true,
            custom_symbols: Some("@#$|".to_string()),
            require_each_enabled_class: true,
            min_entropy_bits: Some(128.0),
        };

        let secret = generator.generate_password(&policy).unwrap();
        assert_eq!(secret.as_str().len(), 32);
        for ch in secret.as_str().chars() {
            assert!(
                !AMBIGUOUS_CHARS.contains(&ch),
                "unexpected ambiguous char: {ch}"
            );
        }
    }

    #[test]
    fn validation_edge_cases() {
        let generator = PasswordGenerator::new();

        // Too short
        let too_short = PasswordPolicy {
            length: 2,
            ..PasswordPolicy::default()
        };
        assert!(matches!(
            generator.validate_policy(&too_short),
            Err(PasswordGenError::LengthTooShort {
                min: 4,
                requested: 2
            })
        ));

        // Too long
        let too_long = PasswordPolicy {
            length: 2048,
            ..PasswordPolicy::default()
        };
        assert!(matches!(
            generator.validate_policy(&too_long),
            Err(PasswordGenError::LengthTooLong {
                max: 1024,
                requested: 2048
            })
        ));

        // Empty charset
        let empty = PasswordPolicy {
            length: 12,
            include_uppercase: false,
            include_lowercase: false,
            include_digits: false,
            include_symbols: false,
            ..PasswordPolicy::default()
        };
        assert_eq!(
            generator.validate_policy(&empty),
            Err(PasswordGenError::EmptyCharset)
        );

        // Blank custom symbols
        let blank_symbols = PasswordPolicy {
            length: 12,
            custom_symbols: Some("   ".to_string()),
            ..PasswordPolicy::default()
        };
        assert_eq!(
            generator.validate_policy(&blank_symbols),
            Err(PasswordGenError::InvalidCustomSymbols)
        );

        // Insufficient entropy
        let low_entropy = PasswordPolicy {
            length: 4,
            include_uppercase: false,
            include_lowercase: false,
            include_digits: true,
            include_symbols: false,
            min_entropy_bits: Some(80.0),
            ..PasswordPolicy::default()
        };
        assert!(matches!(
            generator.validate_policy(&low_entropy),
            Err(PasswordGenError::InsufficientEntropy { .. })
        ));
    }

    #[test]
    fn passphrase_generation_and_serialization() {
        let generator = PasswordGenerator::new();
        let policy = PassphrasePolicy {
            word_count: 5,
            separator: "_".to_string(),
            capitalize: true,
            append_digit: true,
        };

        let secret = generator.generate_passphrase(&policy, None).unwrap();
        assert!(secret.metadata.is_passphrase);
        let parts: Vec<&str> = secret.as_str().split('_').collect();
        assert_eq!(parts.len(), 5);
        assert!(parts[0].chars().next().unwrap().is_ascii_uppercase());
        assert!(secret.as_str().chars().last().unwrap().is_ascii_digit());

        // Round-trip serialization of policy and metadata
        let policy_json = serde_json::to_string(&policy).unwrap();
        let decoded_policy: PassphrasePolicy = serde_json::from_str(&policy_json).unwrap();
        assert_eq!(policy, decoded_policy);

        let meta_json = serde_json::to_string(&secret.metadata).unwrap();
        let decoded_meta: PasswordMetadata = serde_json::from_str(&meta_json).unwrap();
        assert_eq!(secret.metadata, decoded_meta);
    }

    #[test]
    fn batch_generation_bounds() {
        let generator = PasswordGenerator::new();
        let policy = PasswordPolicy::default();

        assert!(matches!(
            generator.generate_batch(&policy, 0),
            Err(PasswordGenError::InvalidBatchCount { .. })
        ));

        let batch = generator.generate_batch(&policy, 5).unwrap();
        assert_eq!(batch.len(), 5);
    }
}
