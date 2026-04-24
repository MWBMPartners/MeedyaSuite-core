use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::types::{ProviderResult, SearchQuery};

/// Weights for each scoring component.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub title: f64,
    pub artist: f64,
    pub album: f64,
    pub year: f64,
    pub isrc: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            title: 0.35,
            artist: 0.30,
            album: 0.20,
            year: 0.10,
            isrc: 0.05,
        }
    }
}

/// Fuzzy match scorer for ranking provider results against a search query.
pub struct MatchScorer {
    matcher: SkimMatcherV2,
    weights: ScoringWeights,
}

impl MatchScorer {
    pub fn new(weights: ScoringWeights) -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
            weights,
        }
    }

    /// Compute a 0.0–1.0 confidence score for how well a result matches the query.
    pub fn score(&self, query: &SearchQuery, result: &ProviderResult) -> f64 {
        let mut total = 0.0;
        let mut total_weight = 0.0;

        // Title
        if let Some(ref q_title) = query.title {
            if let Some(ref r_title) = result.title {
                total += self.weights.title * fuzzy_ratio(&self.matcher, q_title, r_title);
                total_weight += self.weights.title;
            }
        }

        // Artist
        if let Some(ref q_artist) = query.artist {
            if let Some(ref r_artist) = result.artist {
                total += self.weights.artist * fuzzy_ratio(&self.matcher, q_artist, r_artist);
                total_weight += self.weights.artist;
            }
        }

        // Album
        if let Some(ref q_album) = query.album {
            if let Some(ref r_album) = result.album {
                total += self.weights.album * fuzzy_ratio(&self.matcher, q_album, r_album);
                total_weight += self.weights.album;
            }
        }

        // Year
        if let Some(q_year) = query.year {
            if let Some(r_year) = result.year {
                total += self.weights.year * year_proximity(q_year, r_year);
                total_weight += self.weights.year;
            }
        }

        // ISRC (exact match)
        if let Some(ref q_isrc) = query.isrc {
            if let Some(ref r_isrc) = result.isrc {
                let score = if normalize_isrc(q_isrc) == normalize_isrc(r_isrc) {
                    1.0
                } else {
                    0.0
                };
                total += self.weights.isrc * score;
                total_weight += self.weights.isrc;
            }
        }

        if total_weight > 0.0 {
            (total / total_weight).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

impl Default for MatchScorer {
    fn default() -> Self {
        Self::new(ScoringWeights::default())
    }
}

/// Compute a normalized fuzzy ratio between two strings (0.0–1.0).
fn fuzzy_ratio(matcher: &SkimMatcherV2, query: &str, target: &str) -> f64 {
    let q = normalize_string(query);
    let t = normalize_string(target);

    if q.is_empty() || t.is_empty() {
        return 0.0;
    }

    // Score the match and normalize against self-match of the longer string
    let match_score = matcher.fuzzy_match(&t, &q).unwrap_or(0) as f64;
    let longer = if q.len() >= t.len() { &q } else { &t };
    let self_score = matcher.fuzzy_match(longer, longer).unwrap_or(1) as f64;

    if self_score > 0.0 {
        (match_score / self_score).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Year proximity: 1.0 for exact match, -0.1 per year of difference, clamped to 0.0.
fn year_proximity(query_year: u32, result_year: u32) -> f64 {
    let diff = (query_year as f64 - result_year as f64).abs();
    (1.0 - diff * 0.1).max(0.0)
}

/// Normalize a string for fuzzy matching: lowercase, strip punctuation, collapse whitespace.
fn normalize_string(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalize an ISRC for comparison.
fn normalize_isrc(isrc: &str) -> String {
    isrc.to_uppercase().replace('-', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_scores_high() {
        let scorer = MatchScorer::default();
        let query = SearchQuery {
            title: Some("Bohemian Rhapsody".to_string()),
            artist: Some("Queen".to_string()),
            ..Default::default()
        };
        let mut result = ProviderResult::new("test");
        result.title = Some("Bohemian Rhapsody".to_string());
        result.artist = Some("Queen".to_string());

        let score = scorer.score(&query, &result);
        assert!(score > 0.9, "exact match should score > 0.9, got {score}");
    }

    #[test]
    fn partial_match_scores_lower() {
        let scorer = MatchScorer::default();
        let query = SearchQuery {
            title: Some("Bohemian Rhapsody".to_string()),
            artist: Some("Queen".to_string()),
            ..Default::default()
        };
        let mut result = ProviderResult::new("test");
        result.title = Some("Bohemian Rhapsody (Remastered)".to_string());
        result.artist = Some("Queen".to_string());

        let score = scorer.score(&query, &result);
        assert!(score > 0.5, "partial match should score > 0.5, got {score}");
    }

    #[test]
    fn year_proximity_scoring() {
        assert_eq!(year_proximity(2020, 2020), 1.0);
        assert!((year_proximity(2020, 2021) - 0.9).abs() < 0.001);
        assert!((year_proximity(2020, 2025) - 0.5).abs() < 0.001);
        assert_eq!(year_proximity(2020, 2030), 0.0);
        assert_eq!(year_proximity(2020, 2040), 0.0);
    }

    #[test]
    fn normalize_string_strips_punctuation() {
        assert_eq!(normalize_string("Hello, World!"), "hello world");
        assert_eq!(normalize_string("It's a Test"), "it s a test");
        assert_eq!(normalize_string("  multiple   spaces  "), "multiple spaces");
    }

    #[test]
    fn isrc_normalization() {
        assert_eq!(normalize_isrc("US-S1Z-12-34567"), "USS1Z1234567");
        assert_eq!(normalize_isrc("uss1z1234567"), "USS1Z1234567");
    }

    #[test]
    fn empty_query_scores_zero() {
        let scorer = MatchScorer::default();
        let query = SearchQuery::default();
        let result = ProviderResult::new("test");
        assert_eq!(scorer.score(&query, &result), 0.0);
    }

    #[test]
    fn mismatch_scores_low() {
        let scorer = MatchScorer::default();
        let query = SearchQuery {
            title: Some("Bohemian Rhapsody".to_string()),
            ..Default::default()
        };
        let mut result = ProviderResult::new("test");
        result.title = Some("Stairway to Heaven".to_string());

        let score = scorer.score(&query, &result);
        assert!(score < 0.5, "mismatch should score < 0.5, got {score}");
    }
}
