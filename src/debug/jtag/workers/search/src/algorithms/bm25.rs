/// BM25 Algorithm (Best Matching 25)
///
/// Industry-standard ranking function for information retrieval.
/// TF-IDF variant with term frequency saturation and document length normalization.
///
/// Parameters:
/// - k1: Term frequency saturation (default: 1.2, range 1.2-2.0)
/// - b: Document length normalization (default: 0.75, range 0-1)
use super::{SearchAlgorithm, SearchInput, SearchOutput};
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct Bm25Algorithm {
    /// Term frequency saturation parameter
    k1: f64,
    /// Document length normalization parameter
    b: f64,
    /// Case-insensitive matching
    case_insensitive: bool,
    /// Minimum term length
    min_term_length: usize,
}

impl Bm25Algorithm {
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
            .map(String::from)
            .collect()
    }

    /// Build term frequency map for a document
    fn term_frequencies(&self, doc: &str) -> HashMap<String, usize> {
        let mut tf: HashMap<String, usize> = HashMap::new();
        for term in self.tokenize(doc) {
            *tf.entry(term).or_insert(0) += 1;
        }
        tf
    }

    /// Calculate IDF for a term across corpus
    fn idf(&self, term: &str, doc_term_freqs: &[HashMap<String, usize>], n: usize) -> f64 {
        let docs_containing = doc_term_freqs
            .iter()
            .filter(|tf| tf.contains_key(term))
            .count();

        if docs_containing == 0 {
            return 0.0;
        }

        // IDF formula: ln((N - n + 0.5) / (n + 0.5) + 1)
        let n_f = n as f64;
        let df = docs_containing as f64;
        ((n_f - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// Score a single document
    fn score_document(
        &self,
        query_terms: &[String],
        doc_tf: &HashMap<String, usize>,
        doc_len: usize,
        avg_doc_len: f64,
        idf_cache: &HashMap<String, f64>,
    ) -> f64 {
        let mut score = 0.0;

        for term in query_terms {
            let idf = idf_cache.get(term).copied().unwrap_or(0.0);
            let tf = *doc_tf.get(term).unwrap_or(&0) as f64;

            if tf > 0.0 {
                // BM25 formula
                let numerator = tf * (self.k1 + 1.0);
                let denominator =
                    tf + self.k1 * (1.0 - self.b + self.b * (doc_len as f64 / avg_doc_len));
                score += idf * (numerator / denominator);
            }
        }

        score
    }

    /// Normalize scores to 0-1 range
    fn normalize_scores(scores: &mut [f64]) {
        let max = scores.iter().cloned().fold(0.0_f64, f64::max);
        if max > 0.0 {
            for score in scores.iter_mut() {
                *score /= max;
            }
        }
    }
}

impl Default for Bm25Algorithm {
    fn default() -> Self {
        Self {
            k1: 1.2,
            b: 0.75,
            case_insensitive: true,
            min_term_length: 2,
        }
    }
}

impl SearchAlgorithm for Bm25Algorithm {
    fn name(&self) -> &'static str {
        "bm25"
    }

    fn execute(&self, input: &SearchInput) -> SearchOutput {
        let n = input.corpus.len();
        if n == 0 {
            return SearchOutput {
                scores: vec![],
                ranked_indices: vec![],
            };
        }

        // Pre-compute term frequencies for all documents
        let doc_term_freqs: Vec<HashMap<String, usize>> = input
            .corpus
            .iter()
            .map(|doc| self.term_frequencies(doc))
            .collect();

        // Document lengths
        let doc_lens: Vec<usize> = input
            .corpus
            .iter()
            .map(|d| self.tokenize(d).len())
            .collect();
        let avg_doc_len = doc_lens.iter().sum::<usize>() as f64 / n as f64;

        // Query terms
        let query_terms = self.tokenize(&input.query);

        // Pre-compute IDF for query terms
        let mut idf_cache: HashMap<String, f64> = HashMap::new();
        for term in &query_terms {
            if !idf_cache.contains_key(term) {
                idf_cache.insert(term.clone(), self.idf(term, &doc_term_freqs, n));
            }
        }

        // Score each document
        let mut scores: Vec<f64> = doc_term_freqs
            .iter()
            .zip(doc_lens.iter())
            .map(|(tf, &len)| self.score_document(&query_terms, tf, len, avg_doc_len, &idf_cache))
            .collect();

        // Normalize to 0-1
        Self::normalize_scores(&mut scores);

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
            "k1" => Some(json!(self.k1)),
            "b" => Some(json!(self.b)),
            "case_insensitive" => Some(json!(self.case_insensitive)),
            "min_term_length" => Some(json!(self.min_term_length)),
            _ => None,
        }
    }

    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "k1" => {
                self.k1 = value.as_f64().ok_or("k1 must be float")?;
                Ok(())
            }
            "b" => {
                self.b = value.as_f64().ok_or("b must be float")?;
                Ok(())
            }
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
        vec!["k1", "b", "case_insensitive", "min_term_length"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_ranking() {
        let algo = Bm25Algorithm::default();
        let input = SearchInput {
            query: "genome register".to_string(),
            corpus: vec![
                "Use genome/paging-register with personaId".to_string(),
                "The weather is nice today".to_string(),
                "Register genome adapters for personas".to_string(),
                "genome genome genome register register".to_string(), // Term saturation test
            ],
        };

        let output = algo.execute(&input);

        // Docs with query terms should score higher
        assert!(output.scores[0] > output.scores[1]);
        assert!(output.scores[2] > output.scores[1]);

        // Term saturation: repeating terms shouldn't dominate
        // Doc 3 has more repetition but shouldn't be much higher than doc 0
        assert!(output.scores[3] < output.scores[0] * 2.0);
    }
}
