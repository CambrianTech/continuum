/// Bag of Words Algorithm
///
/// Simple term overlap scoring with optional case sensitivity and stopwords.
/// Fast O(n*m) where n=query terms, m=doc terms.
use super::{SearchAlgorithm, SearchInput, SearchOutput};
use serde_json::{json, Value};
use std::collections::HashSet;

pub struct BowAlgorithm {
    /// Case-insensitive matching (default: true)
    case_insensitive: bool,
    /// Stopwords to ignore
    stopwords: HashSet<String>,
    /// Minimum term length to consider
    min_term_length: usize,
}

impl BowAlgorithm {
    /// Factory method (OpenCV create() pattern)
    pub fn create() -> Box<dyn SearchAlgorithm> {
        Box::new(Self::default())
    }

    /// Tokenize text into terms
    fn tokenize(&self, text: &str) -> Vec<String> {
        let text = if self.case_insensitive {
            text.to_lowercase()
        } else {
            text.to_string()
        };

        text.split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= self.min_term_length)
            .filter(|s| !self.stopwords.contains(*s))
            .map(String::from)
            .collect()
    }

    /// Score single document against query terms
    fn score_document(&self, query_terms: &HashSet<String>, doc: &str) -> f64 {
        let doc_terms: HashSet<String> = self.tokenize(doc).into_iter().collect();

        if doc_terms.is_empty() || query_terms.is_empty() {
            return 0.0;
        }

        let intersection = query_terms.intersection(&doc_terms).count();
        let union = query_terms.union(&doc_terms).count();

        // Jaccard similarity: |A ∩ B| / |A ∪ B|
        intersection as f64 / union as f64
    }
}

impl Default for BowAlgorithm {
    fn default() -> Self {
        let stopwords: HashSet<String> = [
            "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "must",
            "shall", "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on", "with",
            "at", "by", "from", "as", "into", "through", "during", "before", "after", "above",
            "below", "between", "under", "again", "further", "then", "once", "here", "there",
            "when", "where", "why", "how", "all", "each", "few", "more", "most", "other", "some",
            "such", "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very", "just",
            "and", "but", "if", "or", "because", "until", "while", "this", "that", "these",
            "those", "it", "its",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            case_insensitive: true,
            stopwords,
            min_term_length: 2,
        }
    }
}

impl SearchAlgorithm for BowAlgorithm {
    fn name(&self) -> &'static str {
        "bow"
    }

    fn execute(&self, input: &SearchInput) -> SearchOutput {
        let query_terms: HashSet<String> = self.tokenize(&input.query).into_iter().collect();

        let scores: Vec<f64> = input
            .corpus
            .iter()
            .map(|doc| self.score_document(&query_terms, doc))
            .collect();

        // Rank by score descending
        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let ranked_indices: Vec<usize> = ranked.into_iter().map(|(i, _)| i).collect();

        SearchOutput {
            scores,
            ranked_indices,
        }
    }

    fn get_param(&self, name: &str) -> Option<Value> {
        match name {
            "case_insensitive" => Some(json!(self.case_insensitive)),
            "min_term_length" => Some(json!(self.min_term_length)),
            "stopwords_count" => Some(json!(self.stopwords.len())),
            _ => None,
        }
    }

    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "case_insensitive" => {
                self.case_insensitive = value.as_bool().ok_or("case_insensitive must be bool")?;
                Ok(())
            }
            "min_term_length" => {
                self.min_term_length =
                    value.as_u64().ok_or("min_term_length must be uint")? as usize;
                Ok(())
            }
            _ => Err(format!("Unknown parameter: {name}")),
        }
    }

    fn param_names(&self) -> Vec<&'static str> {
        vec!["case_insensitive", "min_term_length", "stopwords_count"]
    }

    fn clear(&mut self) {
        self.stopwords.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scoring() {
        let algo = BowAlgorithm::default();
        let input = SearchInput {
            query: "genome register persona".to_string(),
            corpus: vec![
                "Use genome/paging-register with personaId and displayName".to_string(),
                "The weather is nice today".to_string(),
                "Register your persona in the genome system".to_string(),
            ],
        };

        let output = algo.execute(&input);

        // First and third docs should score higher than second
        assert!(output.scores[0] > output.scores[1]);
        assert!(output.scores[2] > output.scores[1]);
    }
}
