//! How relevant each tool is to what she is doing: the cold-start input, read from text.
//!
//! [`Relevance`] is the seam. The intended implementation is the core's embedding owner
//! (`cognition::tool_embedding`: the tool descriptions embedded once, her recent turns
//! embedded per turnover, cosine between them), which needs the inference adapters and so
//! cannot run in this crate. [`Lexical`] is the offline stand-in: TF-IDF cosine between her
//! recent acts and each tool's name and description.

use std::collections::HashMap;

use crate::vocab::Vocabulary;

/// The relevance of every tool (indexed by [`crate::ToolId::index`]) to `context`, in [0, 1].
pub trait Relevance {
    fn relevance(&self, context: &str) -> Vec<f32>;
}

/// TF-IDF cosine over the words of each tool's name and description.
#[derive(Debug, Clone)]
pub struct Lexical {
    /// Inverse document frequency of each word the descriptions use.
    idf: HashMap<String, f32>,
    /// Each tool's unit TF-IDF vector.
    tools: Vec<HashMap<String, f32>>,
}

impl Lexical {
    pub fn new(vocab: &Vocabulary) -> Self {
        let documents: Vec<Vec<String>> = vocab
            .ids()
            .map(|tool| {
                let schema = vocab.schema(tool);
                words(&format!("{} {}", schema.name, schema.description))
            })
            .collect();
        let mut seen_in: HashMap<String, usize> = HashMap::new();
        for document in &documents {
            let mut unique: Vec<&String> = document.iter().collect();
            unique.sort_unstable();
            unique.dedup();
            for word in unique {
                *seen_in.entry(word.clone()).or_insert(0) += 1;
            }
        }
        let count = documents.len() as f32;
        let idf = seen_in.into_iter().map(|(word, n)| (word, ((count + 1.0) / (n as f32 + 1.0)).ln())).collect();
        let mut lexical = Self { idf, tools: Vec::new() };
        lexical.tools = documents.iter().map(|document| lexical.vector(document)).collect();
        lexical
    }

    /// The unit TF-IDF vector of `words`, over the words the descriptions know.
    fn vector(&self, words: &[String]) -> HashMap<String, f32> {
        let mut counts: HashMap<&str, f32> = HashMap::new();
        for word in words {
            *counts.entry(word.as_str()).or_insert(0.0) += 1.0;
        }
        let mut vector: HashMap<String, f32> = counts
            .into_iter()
            .filter_map(|(word, n)| self.idf.get(word).map(|idf| (word.to_owned(), (1.0 + n.ln()) * idf)))
            .collect();
        let norm = vector.values().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            vector.values_mut().for_each(|value| *value /= norm);
        }
        vector
    }
}

impl Relevance for Lexical {
    fn relevance(&self, context: &str) -> Vec<f32> {
        let query = self.vector(&words(context));
        self.tools
            .iter()
            .map(|tool| query.iter().map(|(word, weight)| tool.get(word).map_or(0.0, |other| weight * other)).sum::<f32>()) // map_or: a word the tool never uses adds nothing to the cosine
            .collect()
    }
}

/// The lowercase words of `text`: runs of letters, digits and `_` that start with a letter and
/// are at least two characters long.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| word.len() >= 2 && word.starts_with(|c: char| c.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::ToolSchema;

    // what this catches: the stand-in ranking by something other than shared rare words, the
    // one property a cold start needs from it (her act about a git diff ranks the diff verb).
    #[test]
    fn an_act_ranks_the_tool_whose_description_shares_its_rare_words() {
        let schema = |name: &str, description: &str| ToolSchema { name: name.into(), description: description.into(), tokens: 1 };
        let vocab = Vocabulary::new(vec![
            schema("code/git/diff", "Show the git diff of the working tree"),
            schema("web/fetch", "Fetch a web page by URL"),
            schema("work/get", "Read one card of the work board"),
        ])
        .expect("small vocabulary");
        let relevance = Lexical::new(&vocab).relevance("I should look at the diff before committing to git");
        let diff = vocab.id("code/git/diff").expect("listed").index();
        let best = (0..relevance.len()).max_by(|a, b| relevance[*a].total_cmp(&relevance[*b]));
        assert_eq!(best, Some(diff), "{relevance:?}");
        assert!(relevance.iter().all(|r| (0.0..=1.0 + 1e-6).contains(r)), "{relevance:?}");
        assert!(Lexical::new(&vocab).relevance("").iter().all(|r| *r == 0.0));
    }
}
