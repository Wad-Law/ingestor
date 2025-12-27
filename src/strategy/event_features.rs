//! event_features.rs
//!
//! Low-level feature extraction from TokenizedNews:
//!   - entities (via curated dictionaries / Aho-Corasick)
//!   - numbers (%, bps, years, generic numbers)
//!   - coarse time windows (year-end, next week, Q4, etc.)

use std::collections::HashMap;

use aho_corasick::AhoCorasick;
use chrono::{DateTime, Duration, Utc};
use lazy_static::lazy_static;
use regex::Regex;

use crate::strategy::tokenization::TokenizedNews;

/// Simple extracted entity.
#[derive(Debug, Clone)]
pub struct Entity {
    pub value: String, // canonical label, e.g. "ECB", "Eurozone"
}

/// Coarse time window extracted from text.
#[derive(Debug, Clone)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    #[allow(dead_code)]
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct EventFeatures {
    pub entities: Vec<Entity>,
    pub time_window: Option<TimeWindow>,
}

/// Dictionaries used in feature extraction.
/// Keep this minimal & composable – load from config/JSON later.
#[derive(Debug, Clone)]
pub struct FeatureDictionaries {
    /// Lowercased pattern -> canonical label
    pub entities: HashMap<String, String>,
}

impl FeatureDictionaries {
    pub fn default_minimal() -> Self {
        let mut entities = HashMap::new();

        // Central banks
        entities.insert("ecb".into(), "ECB".into());
        entities.insert("fed".into(), "Fed".into());
        entities.insert("fomc".into(), "Fed".into());
        entities.insert("bank of england".into(), "BoE".into());
        entities.insert("boj".into(), "BoJ".into());

        // Macro concepts
        entities.insert("inflation".into(), "inflation".into());
        entities.insert("cpi".into(), "CPI".into());
        entities.insert("gdp".into(), "GDP".into());

        // Countries (tiny sample)
        entities.insert("united states".into(), "US".into());
        entities.insert("u.s.".into(), "US".into());
        entities.insert("us".into(), "US".into());
        entities.insert("china".into(), "China".into());
        entities.insert("germany".into(), "Germany".into());

        // Crypto
        entities.insert("bitcoin".into(), "BTC".into());
        entities.insert("btc".into(), "BTC".into());
        entities.insert("ether".into(), "ETH".into());
        entities.insert("eth".into(), "ETH".into());

        Self { entities }
    }
}

/// Extracts low-level features from normalized text.
/// Extracts low-level features from normalized text.
pub struct EventFeatureExtractor {
    ac_entities: AhoCorasick,
    entity_labels: Vec<String>,
    re_date_phrase: Regex,
}

impl EventFeatureExtractor {
    pub fn new(dict: FeatureDictionaries) -> Self {
        let mut patterns = Vec::new();
        let mut labels = Vec::new();

        // Build AC over all entity patterns (keys must be lowercase).
        for (pat, label) in dict.entities.into_iter() {
            patterns.push(pat);
            labels.push(label);
        }

        let ac_entities = AhoCorasick::new(&patterns).expect("failed to build AC for entities");

        lazy_static! {
            static ref RE_DATE_PHRASE: Regex = Regex::new(
                r"\b(year[- ]end|year end|next week|this week|next month|this month|q[1-4])\b"
            )
            .unwrap();
        }

        Self {
            ac_entities,
            entity_labels: labels,
            re_date_phrase: RE_DATE_PHRASE.clone(),
        }
    }

    #[allow(dead_code)]
    pub fn with_default_dicts() -> Self {
        Self::new(FeatureDictionaries::default_minimal())
    }

    pub fn extract(&self, tok: &TokenizedNews, now: DateTime<Utc>) -> EventFeatures {
        let text = tok.normalized.as_str();

        let entities = self.extract_entities(text);
        let time_window = self.derive_time_window(text, now);

        EventFeatures {
            entities,
            time_window,
        }
    }

    fn extract_entities(&self, text: &str) -> Vec<Entity> {
        let mut entities = Vec::new();

        for m in self.ac_entities.find_iter(text) {
            let label = &self.entity_labels[m.pattern()];
            entities.push(Entity {
                value: label.clone(),
            });
        }

        entities
    }

    fn derive_time_window(&self, text: &str, now: DateTime<Utc>) -> Option<TimeWindow> {
        // Phrase-based first.
        for caps in self.re_date_phrase.captures_iter(text) {
            if let Some(m) = caps.get(1) {
                if let Some(tw) = map_phrase_to_window(m.as_str(), now) {
                    return Some(tw);
                }
            }
        }

        None
    }
}

// --- helpers ---

// --- helpers ---

fn map_phrase_to_window(phrase: &str, now: DateTime<Utc>) -> Option<TimeWindow> {
    use chrono::{Datelike, TimeZone};

    let lower = phrase.to_lowercase();

    if lower.contains("year-end") || lower.contains("year end") {
        let year = now.year();
        let start = now;
        let end = Utc.with_ymd_and_hms(year, 12, 31, 23, 59, 59).unwrap();
        return Some(TimeWindow { start, end });
    }

    if lower == "next week" {
        let weekday = now.weekday();
        let days_from_monday = weekday.num_days_from_monday() as i64;
        let days_to_next_monday = 7 - days_from_monday;
        let start_date = (now + Duration::days(days_to_next_monday)).date_naive();
        let end_date = start_date + Duration::days(6);

        let start = DateTime::<Utc>::from_naive_utc_and_offset(
            start_date.and_hms_opt(0, 0, 0).unwrap(),
            Utc,
        );
        let end = DateTime::<Utc>::from_naive_utc_and_offset(
            end_date.and_hms_opt(23, 59, 59).unwrap(),
            Utc,
        );
        return Some(TimeWindow { start, end });
    }

    if lower == "this week" {
        let weekday = now.weekday();
        let days_from_monday = weekday.num_days_from_monday() as i64;
        let start_date = (now - Duration::days(days_from_monday)).date_naive();
        let end_date = start_date + Duration::days(6);

        let start = DateTime::<Utc>::from_naive_utc_and_offset(
            start_date.and_hms_opt(0, 0, 0).unwrap(),
            Utc,
        );
        let end = DateTime::<Utc>::from_naive_utc_and_offset(
            end_date.and_hms_opt(23, 59, 59).unwrap(),
            Utc,
        );
        return Some(TimeWindow { start, end });
    }

    if lower == "this month" || lower == "next month" {
        let mut year = now.year();
        let mut month = now.month();

        if lower == "next month" {
            if month == 12 {
                year += 1;
                month = 1;
            } else {
                month += 1;
            }
        }

        let start = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).unwrap();
        // naive end of month: +32 days → clamp to 1st of following month -1 sec
        let approx_next_month = start + Duration::days(32);
        let ny = approx_next_month.year();
        let nm = approx_next_month.month();
        let next_month_start = Utc.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).unwrap();
        let end = next_month_start - Duration::seconds(1);
        return Some(TimeWindow { start, end });
    }

    if lower.starts_with('q') && lower.len() == 2 {
        if let Some(qch) = lower.chars().nth(1) {
            if let Some(q) = qch.to_digit(10) {
                let year = now.year();
                let (start_month, end_month) = match q {
                    1 => (1, 3),
                    2 => (4, 6),
                    3 => (7, 9),
                    4 => (10, 12),
                    _ => return None,
                };
                let start = Utc.with_ymd_and_hms(year, start_month, 1, 0, 0, 0).unwrap();
                let approx_end = Utc
                    .with_ymd_and_hms(year, end_month, 28, 23, 59, 59)
                    .unwrap();
                let end = approx_end + Duration::days(7); // approximate
                return Some(TimeWindow { start, end });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::RawNews;
    use crate::strategy::tokenization::{TokenizationConfig, TokenizedNews};

    fn make_tokenized(text: &str) -> TokenizedNews {
        let raw = RawNews {
            title: text.to_string(),
            url: "http://example.com".to_string(),
            description: "".to_string(),
            feed: "test".to_string(),
            published: Some(Utc::now()),
            labels: vec![],
        };
        let cfg = TokenizationConfig::default();
        TokenizedNews::from_raw(raw, &cfg)
    }

    #[test]
    fn test_entity_extraction() {
        let extractor = EventFeatureExtractor::with_default_dicts();
        let now = Utc::now();
        // "Fed" is in dict
        let tok = make_tokenized("Fed discuss inflation");
        let feat = extractor.extract(&tok, now);

        // Should find Fed, inflation
        let values: Vec<String> = feat.entities.iter().map(|e| e.value.clone()).collect();
        assert!(values.contains(&"Fed".to_string()));
        assert!(values.contains(&"inflation".to_string()));
    }

    #[test]
    fn test_time_window_phrase() {
        let extractor = EventFeatureExtractor::with_default_dicts();
        let now = Utc::now();
        let tok = make_tokenized("Outlook for next week");
        let feat = extractor.extract(&tok, now);

        assert!(feat.time_window.is_some());
        let tw = feat.time_window.unwrap();
        assert!(tw.end > tw.start);
        assert!(tw.start > now); // Next week starts in future
    }
}
