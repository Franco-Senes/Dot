use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_WORDS_PER_MINUTE: u32 = 200;
const DEFAULT_MAX_INPUT_BYTES: usize = 5 * 1024 * 1024;
const DEFAULT_TOP_KEYWORDS_LIMIT: usize = 10;

const DEFAULT_STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "has", "have", "in", "is",
    "it", "its", "of", "on", "or", "that", "the", "to", "was", "were", "will", "with",
];

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WordCounterError {
    #[error("words_per_minute must be greater than zero, got {0}")]
    InvalidWordsPerMinute(u32),
    #[error("max_input_bytes must be greater than zero")]
    InvalidMaxInputBytes,
    #[error("input size {actual} bytes exceeds maximum allowed {max} bytes")]
    InputTooLarge { actual: usize, max: usize },
    #[error("selection text is empty")]
    EmptySelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordCounterConfig {
    pub words_per_minute: u32,
    pub include_numbers: bool,
    pub count_hyphenated_as_one: bool,
    pub strip_html_tags: bool,
    pub exclude_stop_words: bool,
    pub max_input_bytes: usize,
    pub top_keywords_limit: usize,
    pub custom_stop_words: BTreeSet<String>,
}

impl Default for WordCounterConfig {
    fn default() -> Self {
        Self {
            words_per_minute: DEFAULT_WORDS_PER_MINUTE,
            include_numbers: true,
            count_hyphenated_as_one: true,
            strip_html_tags: true,
            exclude_stop_words: true,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            top_keywords_limit: DEFAULT_TOP_KEYWORDS_LIMIT,
            custom_stop_words: BTreeSet::new(),
        }
    }
}

impl WordCounterConfig {
    pub fn validate(&self) -> Result<(), WordCounterError> {
        if self.words_per_minute == 0 {
            return Err(WordCounterError::InvalidWordsPerMinute(
                self.words_per_minute,
            ));
        }
        if self.max_input_bytes == 0 {
            return Err(WordCounterError::InvalidMaxInputBytes);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordFrequency {
    pub word: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageWordStats {
    pub word_count: usize,
    pub unique_word_count: usize,
    pub character_count: usize,
    pub character_count_no_spaces: usize,
    pub sentence_count: usize,
    pub paragraph_count: usize,
    pub line_count: usize,
    pub estimated_reading_time_secs: u64,
    pub estimated_reading_time_display: String,
    pub average_word_length: f64,
    pub top_words: Vec<WordFrequency>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionComparison {
    pub selection: PageWordStats,
    pub page_total_words: usize,
    pub percentage_of_page: f64,
}

#[derive(Debug, Clone)]
pub struct PageWordCounter {
    config: WordCounterConfig,
}

impl PageWordCounter {
    pub fn new(config: WordCounterConfig) -> Result<Self, WordCounterError> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &WordCounterConfig {
        &self.config
    }

    pub fn count_page(&self, input: &str) -> Result<PageWordStats, WordCounterError> {
        if input.len() > self.config.max_input_bytes {
            return Err(WordCounterError::InputTooLarge {
                actual: input.len(),
                max: self.config.max_input_bytes,
            });
        }

        let cleaned = if self.config.strip_html_tags {
            extract_visible_text_from_html(input)
        } else {
            input.to_string()
        };

        Ok(self.analyze_plain_text(&cleaned))
    }

    pub fn count_selection(
        &self,
        page_input: &str,
        selection_input: &str,
    ) -> Result<SelectionComparison, WordCounterError> {
        if selection_input.trim().is_empty() {
            return Err(WordCounterError::EmptySelection);
        }

        let page_stats = self.count_page(page_input)?;
        let selection_stats = self.count_page(selection_input)?;

        let percentage_of_page = if page_stats.word_count == 0 {
            0.0
        } else {
            let pct = (selection_stats.word_count as f64 / page_stats.word_count as f64) * 100.0;
            (pct * 100.0).round() / 100.0
        };

        Ok(SelectionComparison {
            selection: selection_stats,
            page_total_words: page_stats.word_count,
            percentage_of_page,
        })
    }

    fn analyze_plain_text(&self, text: &str) -> PageWordStats {
        let character_count = text.chars().count();
        let character_count_no_spaces = text.chars().filter(|c| !c.is_whitespace()).count();
        let line_count = if text.is_empty() {
            0
        } else {
            text.lines().count()
        };
        let paragraph_count = count_paragraphs(text);

        let tokens = tokenize_words(text, self.config.count_hyphenated_as_one);
        let mut words = Vec::with_capacity(tokens.len());
        let mut total_word_chars = 0usize;
        let mut freq_map: HashMap<String, usize> = HashMap::new();

        for token in tokens {
            if !self.config.include_numbers && token.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let lower = token.to_lowercase();
            total_word_chars += token.chars().count();
            *freq_map.entry(lower).or_insert(0) += 1;
            words.push(token);
        }

        let word_count = words.len();
        let unique_word_count = freq_map.len();
        let sentence_count = count_sentences(text, word_count);

        let estimated_reading_time_secs = if word_count == 0 {
            0
        } else {
            (word_count as u64 * 60).div_ceil(self.config.words_per_minute as u64)
        };
        let estimated_reading_time_display = format_reading_time(estimated_reading_time_secs);

        let average_word_length = if word_count == 0 {
            0.0
        } else {
            let avg = total_word_chars as f64 / word_count as f64;
            (avg * 100.0).round() / 100.0
        };

        let top_words = self.build_top_words(&freq_map);

        PageWordStats {
            word_count,
            unique_word_count,
            character_count,
            character_count_no_spaces,
            sentence_count,
            paragraph_count,
            line_count,
            estimated_reading_time_secs,
            estimated_reading_time_display,
            average_word_length,
            top_words,
        }
    }

    fn build_top_words(&self, freq_map: &HashMap<String, usize>) -> Vec<WordFrequency> {
        if self.config.top_keywords_limit == 0 {
            return Vec::new();
        }

        let mut items: Vec<WordFrequency> = freq_map
            .iter()
            .filter(|(word, _)| !self.is_stop_word(word))
            .map(|(word, &count)| WordFrequency {
                word: word.clone(),
                count,
            })
            .collect();

        items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
        items.truncate(self.config.top_keywords_limit);
        items
    }

    fn is_stop_word(&self, lower_word: &str) -> bool {
        if self.config.custom_stop_words.contains(lower_word) {
            return true;
        }
        if self.config.exclude_stop_words {
            return DEFAULT_STOP_WORDS.contains(&lower_word);
        }
        false
    }
}

fn tokenize_words(text: &str, count_hyphenated_as_one: bool) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch.is_alphanumeric()
            || ((ch == '\'' || ch == '\u{2019}' || (count_hyphenated_as_one && ch == '-'))
                && !current.is_empty()
                && i + 1 < len
                && chars[i + 1].is_alphanumeric())
        {
            current.push(ch);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        i += 1;
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn count_sentences(text: &str, word_count: usize) -> usize {
    if word_count == 0 {
        return 0;
    }

    let mut count = 0usize;
    let mut in_terminator_run = false;

    for ch in text.chars() {
        if ch == '.' || ch == '!' || ch == '?' {
            if !in_terminator_run {
                count += 1;
                in_terminator_run = true;
            }
        } else if !ch.is_whitespace() {
            in_terminator_run = false;
        }
    }

    if count == 0 {
        1
    } else {
        count
    }
}

fn count_paragraphs(text: &str) -> usize {
    let mut count = 0usize;
    let mut in_paragraph = false;

    for line in text.lines() {
        if line.trim().is_empty() {
            in_paragraph = false;
        } else if !in_paragraph {
            count += 1;
            in_paragraph = true;
        }
    }

    count
}

fn format_reading_time(secs: u64) -> String {
    if secs == 0 {
        return "0 sec".to_string();
    }
    if secs < 60 {
        return "< 1 min".to_string();
    }
    let total_mins = secs.div_ceil(60);
    if total_mins < 60 {
        return format!("{total_mins} min");
    }
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    if mins == 0 {
        format!("{hours} hr")
    } else {
        format!("{hours} hr {mins} min")
    }
}

fn extract_visible_text_from_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let lower_bytes = lower.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if lower_bytes[i..].starts_with(b"<script") {
                if let Some(end_rel) = lower[i..].find("</script>") {
                    i += end_rel + "</script>".len();
                    out.push(' ');
                    continue;
                }
            } else if lower_bytes[i..].starts_with(b"<style") {
                if let Some(end_rel) = lower[i..].find("</style>") {
                    i += end_rel + "</style>".len();
                    out.push(' ');
                    continue;
                }
            } else if lower_bytes[i..].starts_with(b"<!--") {
                if let Some(end_rel) = html[i..].find("-->") {
                    i += end_rel + "-->".len();
                    continue;
                }
            }

            if let Some(close_rel) = html[i..].find('>') {
                let tag_inner = &lower[i + 1..i + close_rel];
                let tag_name = tag_inner.split_whitespace().next().unwrap_or("");
                if is_block_tag(tag_name) {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                i += close_rel + 1;
                continue;
            }
        }

        if let Some(ch) = html[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    decode_basic_entities(&out)
}

fn is_block_tag(tag_name: &str) -> bool {
    let clean = tag_name.trim_start_matches('/').trim_end_matches('/');
    matches!(
        clean,
        "p" | "div"
            | "br"
            | "li"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "section"
            | "article"
            | "tr"
    )
}

fn decode_basic_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_plain_text_stats_and_top_words() {
        let counter = PageWordCounter::new(WordCounterConfig {
            strip_html_tags: false,
            words_per_minute: 200,
            ..Default::default()
        })
        .unwrap();

        let text = "Rust is fast! Rust is safe.\n\nWell-known systems language, Rust 2024.";
        let stats = counter.count_page(text).unwrap();

        assert_eq!(stats.word_count, 11);
        assert_eq!(stats.sentence_count, 3);
        assert_eq!(stats.paragraph_count, 2);
        assert_eq!(stats.estimated_reading_time_secs, 4);
        assert_eq!(stats.estimated_reading_time_display, "< 1 min");
        assert_eq!(stats.top_words[0].word, "rust");
        assert_eq!(stats.top_words[0].count, 3);
    }

    #[test]
    fn strips_html_scripts_styles_and_entities() {
        let counter = PageWordCounter::new(WordCounterConfig::default()).unwrap();
        let html = r#"
            <html>
                <head>
                    <style>body { color: red; }</style>
                    <script>console.log("ignore me");</script>
                </head>
                <body>
                    <!-- hidden comment -->
                    <h1>Search &amp; Browse</h1>
                    <p>Fast, private web search.</p>
                </body>
            </html>
        "#;

        let stats = counter.count_page(html).unwrap();
        assert_eq!(stats.word_count, 6);
        assert_eq!(stats.sentence_count, 1);
        assert!(stats.top_words.iter().any(|w| w.word == "search"));
        assert!(!stats.top_words.iter().any(|w| w.word == "console"));
        assert!(!stats.top_words.iter().any(|w| w.word == "hidden"));
    }

    #[test]
    fn respects_hyphen_number_and_stop_word_options() {
        let counter = PageWordCounter::new(WordCounterConfig {
            strip_html_tags: false,
            include_numbers: false,
            count_hyphenated_as_one: false,
            exclude_stop_words: false,
            top_keywords_limit: 5,
            ..Default::default()
        })
        .unwrap();

        let stats = counter.count_page("The well-known 42 answers").unwrap();
        assert_eq!(stats.word_count, 4);
        assert!(stats.top_words.iter().any(|w| w.word == "the"));
        assert!(!stats.top_words.iter().any(|w| w.word == "42"));
    }

    #[test]
    fn validates_config_input_limits_and_selection_comparison() {
        let bad_wpm = PageWordCounter::new(WordCounterConfig {
            words_per_minute: 0,
            ..Default::default()
        });
        assert_eq!(
            bad_wpm.unwrap_err(),
            WordCounterError::InvalidWordsPerMinute(0)
        );

        let counter = PageWordCounter::new(WordCounterConfig {
            max_input_bytes: 20,
            strip_html_tags: false,
            ..Default::default()
        })
        .unwrap();

        let too_large = counter.count_page("This input is definitely longer than twenty bytes");
        assert!(matches!(
            too_large,
            Err(WordCounterError::InputTooLarge { .. })
        ));

        let full_counter = PageWordCounter::new(WordCounterConfig {
            strip_html_tags: false,
            ..Default::default()
        })
        .unwrap();
        let cmp = full_counter
            .count_selection("one two three four", "one two")
            .unwrap();
        assert_eq!(cmp.page_total_words, 4);
        assert_eq!(cmp.selection.word_count, 2);
        assert_eq!(cmp.percentage_of_page, 50.0);

        let empty_sel = full_counter.count_selection("one two", "   ");
        assert_eq!(empty_sel.unwrap_err(), WordCounterError::EmptySelection);
    }

    #[test]
    fn serializes_and_deserializes_stats_json() {
        let counter = PageWordCounter::new(WordCounterConfig::default()).unwrap();
        let stats = counter.count_page("<p>Hello world.</p>").unwrap();
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: PageWordStats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, decoded);
    }
}
