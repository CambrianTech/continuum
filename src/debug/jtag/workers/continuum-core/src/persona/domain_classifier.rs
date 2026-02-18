//! Domain Classifier — Adapter-aware text classification
//!
//! Classifies incoming text into skill domains based on vocabulary
//! extracted from registered adapters. Adapters DEFINE the domain space:
//! when an adapter with domain "plumbing" is registered, the classifier
//! automatically knows about plumbing. No hardcoding required.
//!
//! Design: keyword scoring with TF-IDF-like weighting, < 1ms per classification.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use ts_rs::TS;

use super::genome_paging::GenomeAdapterInfo;

// =============================================================================
// TYPES (ts-rs generated)
// =============================================================================

/// Result of classifying text into a skill domain.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/DomainClassification.ts")]
pub struct DomainClassification {
    /// Classified domain (e.g. "web-api-development", "plumbing", "general")
    pub domain: String,
    /// Confidence score 0.0-1.0
    pub confidence: f32,
    /// Matching adapter name (None = gap — domain recognized but no adapter)
    #[ts(optional)]
    pub adapter_name: Option<String>,
    /// Classification time in microseconds
    #[ts(type = "number")]
    pub decision_time_us: u64,
}

// =============================================================================
// DOMAIN VOCABULARY
// =============================================================================

/// Keywords and metadata for a single domain.
#[derive(Debug, Clone)]
struct DomainVocabulary {
    /// Keywords associated with this domain (lowercased)
    keywords: Vec<String>,
    /// The adapter that covers this domain (if any)
    adapter_name: Option<String>,
}

// =============================================================================
// BUILT-IN VOCABULARIES
// =============================================================================

/// Built-in domain vocabularies for common skill areas.
/// These provide a baseline — adapter registrations add more keywords.
fn builtin_vocabularies() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("code", vec![
            "function", "import", "export", "const", "let", "var", "class",
            "interface", "type", "async", "await", "promise", "return",
            "typescript", "javascript", "python", "rust", "compile", "debug",
            "error", "bug", "fix", "refactor", "test", "api", "endpoint",
            "database", "query", "sql", "schema", "migration", "deploy",
            "git", "commit", "branch", "merge", "pull request", "docker",
            "npm", "cargo", "pip", "webpack", "vite", "react", "node",
            "express", "routes", "middleware", "http", "rest", "graphql",
            "algorithm", "data structure", "array", "hashmap", "tree",
        ]),
        ("conversation", vec![
            "hello", "hi", "hey", "thanks", "thank you", "please", "help",
            "how are you", "what do you think", "opinion", "feel", "chat",
            "talk", "discuss", "agree", "disagree", "interesting", "cool",
            "awesome", "great", "good morning", "good night", "welcome",
        ]),
        ("teaching", vec![
            "teach", "learn", "lesson", "curriculum", "exam", "quiz", "test",
            "exercise", "practice", "student", "teacher", "explain", "tutorial",
            "course", "module", "assignment", "grade", "knowledge", "skill",
            "training", "workshop", "seminar", "lecture",
        ]),
        ("creative", vec![
            "write", "story", "poem", "creative", "imagine", "fiction",
            "character", "plot", "narrative", "dialogue", "scene", "art",
            "design", "color", "style", "aesthetic", "music", "song",
            "compose", "paint", "draw", "sketch", "illustration",
        ]),
        ("analysis", vec![
            "analyze", "analysis", "data", "statistics", "trend", "pattern",
            "insight", "metric", "benchmark", "performance", "optimize",
            "efficiency", "throughput", "latency", "profiling", "bottleneck",
            "report", "dashboard", "visualization", "chart", "graph",
        ]),
    ]
}

// =============================================================================
// DOMAIN CLASSIFIER
// =============================================================================

/// Classifies text into skill domains using adapter-aware keyword scoring.
///
/// Adapters define the domain space: registering an adapter with domain
/// "plumbing" automatically teaches the classifier about plumbing.
/// Keywords can be enriched over time (academy sessions add learning_objectives).
#[derive(Debug)]
pub struct DomainClassifier {
    /// domain → vocabulary (keywords + adapter mapping)
    domains: HashMap<String, DomainVocabulary>,
    /// Fallback domain when no keywords match
    fallback_domain: String,
}

impl DomainClassifier {
    /// Create a new classifier with built-in vocabularies.
    pub fn new() -> Self {
        let mut domains = HashMap::new();

        for (domain, keywords) in builtin_vocabularies() {
            domains.insert(domain.to_string(), DomainVocabulary {
                keywords: keywords.iter().map(|k| k.to_lowercase()).collect(),
                adapter_name: None,
            });
        }

        Self {
            domains,
            fallback_domain: "general".to_string(),
        }
    }

    /// Rebuild domain→adapter mappings from current adapter state.
    /// Call after genome sync or adapter registration.
    pub fn sync_from_adapters(&mut self, adapters: &[GenomeAdapterInfo]) {
        // Clear all adapter mappings first
        for vocab in self.domains.values_mut() {
            vocab.adapter_name = None;
        }

        for adapter in adapters {
            let domain = adapter.domain.to_lowercase();

            // If domain already exists, just update the adapter mapping
            if let Some(vocab) = self.domains.get_mut(&domain) {
                vocab.adapter_name = Some(adapter.name.clone());
            } else {
                // New domain from adapter — create vocabulary with adapter name as keyword
                self.domains.insert(domain.clone(), DomainVocabulary {
                    keywords: vec![domain.clone(), adapter.name.to_lowercase()],
                    adapter_name: Some(adapter.name.clone()),
                });
            }
        }
    }

    /// Classify text into a skill domain.
    /// Returns the best-matching domain with confidence and adapter info.
    ///
    /// Algorithm: for each domain, count keyword matches in the text.
    /// Score = matches / total_keywords (normalized). Highest score wins.
    /// Confidence scales with match density.
    pub fn classify(&self, text: &str) -> DomainClassification {
        let start = Instant::now();
        let text_lower = text.to_lowercase();
        let text_words: Vec<&str> = text_lower.split_whitespace().collect();

        let mut best_domain = self.fallback_domain.clone();
        let mut best_score: f32 = 0.0;
        let mut best_adapter: Option<String> = None;

        for (domain, vocab) in &self.domains {
            if vocab.keywords.is_empty() {
                continue;
            }

            let mut matches = 0u32;
            for keyword in &vocab.keywords {
                // Multi-word keywords: check substring
                if keyword.contains(' ') {
                    if text_lower.contains(keyword.as_str()) {
                        matches += 2; // Multi-word matches are worth more
                    }
                } else {
                    // Single-word keywords: check word boundaries
                    if text_words.iter().any(|w| w.trim_matches(|c: char| !c.is_alphanumeric()) == keyword.as_str()) {
                        matches += 1;
                    }
                    // Also check substring for compound words (e.g. "typescript" in "typescript-expertise")
                    else if text_lower.contains(keyword.as_str()) {
                        matches += 1;
                    }
                }
            }

            if matches == 0 {
                continue;
            }

            // Normalize by vocabulary size (smaller vocabs need fewer matches)
            let vocab_size = vocab.keywords.len() as f32;
            let raw_score = matches as f32;
            // Score favors absolute matches but normalizes to prevent tiny vocabularies from always winning
            let score = raw_score / (1.0 + vocab_size.sqrt());

            if score > best_score {
                best_score = score;
                best_domain = domain.clone();
                best_adapter = vocab.adapter_name.clone();
            }
        }

        // Confidence: sigmoid-like curve on raw match count
        // 0 matches = 0.0, 3 matches = ~0.5, 8+ matches = ~0.9
        let confidence = if best_score > 0.0 {
            let raw_matches = best_score * (1.0 + (self.domains.get(&best_domain)
                .map(|v| v.keywords.len() as f32)
                .unwrap_or(1.0)).sqrt());
            (1.0 - (-0.3 * raw_matches).exp()).min(1.0)
        } else {
            0.0
        };

        DomainClassification {
            domain: best_domain,
            confidence,
            adapter_name: best_adapter,
            decision_time_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Get the adapter name for a domain (or None if gap).
    pub fn adapter_for_domain(&self, domain: &str) -> Option<&str> {
        self.domains.get(domain)
            .and_then(|v| v.adapter_name.as_deref())
    }

    /// Register new keywords for a domain (e.g., from academy curriculum).
    /// Merges with existing keywords — does not replace.
    pub fn register_domain_keywords(&mut self, domain: &str, keywords: Vec<String>) {
        let entry = self.domains.entry(domain.to_string()).or_insert_with(|| DomainVocabulary {
            keywords: vec![],
            adapter_name: None,
        });

        for kw in keywords {
            let lower = kw.to_lowercase();
            if !entry.keywords.contains(&lower) {
                entry.keywords.push(lower);
            }
        }
    }

    /// Get all known domains with their adapter status.
    pub fn domain_summary(&self) -> Vec<(String, bool)> {
        self.domains.iter()
            .map(|(domain, vocab)| (domain.clone(), vocab.adapter_name.is_some()))
            .collect()
    }
}

// =============================================================================
// INTERACTION QUALITY SCORING
// =============================================================================

/// Quality score for a single interaction (input→output pair).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/QualityScore.ts")]
pub struct QualityScore {
    /// Overall quality score 0.0-1.0
    pub score: f32,
    /// Individual quality factors
    pub factors: QualityFactors,
}

/// Breakdown of quality factors for an interaction.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/QualityFactors.ts")]
pub struct QualityFactors {
    /// Human feedback signal (positive reply, "thanks", corrections)
    pub human_feedback: f32,
    /// Task completion success signal
    pub task_success: f32,
    /// Response substance (length, specificity, structure)
    pub substance: f32,
    /// Was this a corrected response? (gold standard for training)
    pub correction: f32,
}

/// Score the quality of an interaction for training data selection.
/// Higher quality examples produce better fine-tuning results.
pub fn score_interaction_quality(
    input: &str,
    output: &str,
    feedback: Option<&str>,
    task_outcome: Option<bool>,
) -> QualityScore {
    let mut factors = QualityFactors {
        human_feedback: 0.5,  // Neutral default
        task_success: 0.5,
        substance: 0.0,
        correction: 0.0,
    };

    // Factor 1: Human feedback
    if let Some(fb) = feedback {
        let fb_lower = fb.to_lowercase();
        if fb_lower.contains("thank") || fb_lower.contains("great") || fb_lower.contains("perfect")
            || fb_lower.contains("exactly") || fb_lower.contains("good") || fb_lower.contains("awesome")
        {
            factors.human_feedback = 0.9;
        } else if fb_lower.contains("wrong") || fb_lower.contains("no") || fb_lower.contains("incorrect")
            || fb_lower.contains("bad") || fb_lower.contains("fix")
        {
            factors.human_feedback = 0.2;
            factors.correction = 0.8; // Corrections are gold — the corrected version is valuable
        } else {
            factors.human_feedback = 0.6; // Any feedback is slightly positive
        }
    }

    // Factor 2: Task success
    if let Some(success) = task_outcome {
        factors.task_success = if success { 0.9 } else { 0.2 };
    }

    // Factor 3: Substance — longer, structured responses are higher quality training data
    let output_len = output.len();
    factors.substance = if output_len < 20 {
        0.1  // Too short to be useful
    } else if output_len < 100 {
        0.4
    } else if output_len < 500 {
        0.7
    } else {
        0.9
    };

    // Bonus for structured content (code blocks, lists)
    if output.contains("```") || output.contains("- ") || output.contains("1.") {
        factors.substance = (factors.substance + 0.1).min(1.0);
    }

    // Penalize very short inputs (less context for learning)
    if input.len() < 10 {
        factors.substance *= 0.5;
    }

    // Overall score: weighted average
    let score = factors.human_feedback * 0.3
        + factors.task_success * 0.25
        + factors.substance * 0.3
        + factors.correction * 0.15;

    QualityScore {
        score: score.min(1.0),
        factors,
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adapter(name: &str, domain: &str) -> GenomeAdapterInfo {
        GenomeAdapterInfo {
            name: name.to_string(),
            domain: domain.to_string(),
            size_mb: 50.0,
            priority: 0.5,
            is_loaded: false,
            last_used_ms: 0,
            ollama_model_name: None,
        }
    }

    #[test]
    fn test_classify_code_domain() {
        let classifier = DomainClassifier::new();
        let result = classifier.classify("How do I set up Express routes with async middleware?");
        assert_eq!(result.domain, "code", "Should classify as code, got: {}", result.domain);
        assert!(result.confidence > 0.0);
        assert!(result.adapter_name.is_none(), "No adapters registered yet");
    }

    #[test]
    fn test_classify_conversation_domain() {
        let classifier = DomainClassifier::new();
        let result = classifier.classify("Hello! How are you doing today? Thanks for the help.");
        assert_eq!(result.domain, "conversation", "Should classify as conversation, got: {}", result.domain);
    }

    #[test]
    fn test_classify_teaching_domain() {
        let classifier = DomainClassifier::new();
        let result = classifier.classify("Can you teach me about this? I want to learn and practice.");
        assert_eq!(result.domain, "teaching", "Should classify as teaching, got: {}", result.domain);
    }

    #[test]
    fn test_classify_creative_domain() {
        let classifier = DomainClassifier::new();
        let result = classifier.classify("Write me a story with interesting characters and a compelling plot narrative.");
        assert_eq!(result.domain, "creative", "Should classify as creative, got: {}", result.domain);
    }

    #[test]
    fn test_classify_unknown_returns_general() {
        let classifier = DomainClassifier::new();
        let result = classifier.classify("xyzzy foobar baz qux");
        assert_eq!(result.domain, "general", "Unknown text should return general fallback");
        assert!((result.confidence - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_sync_from_adapters_maps_domains() {
        let mut classifier = DomainClassifier::new();
        let adapters = vec![
            make_adapter("ts-expert", "code"),
            make_adapter("chat-bot", "conversation"),
        ];
        classifier.sync_from_adapters(&adapters);

        let result = classifier.classify("Fix this TypeScript import error in the API endpoint");
        assert_eq!(result.domain, "code");
        assert_eq!(result.adapter_name, Some("ts-expert".to_string()));
    }

    #[test]
    fn test_sync_from_adapters_creates_new_domain() {
        let mut classifier = DomainClassifier::new();
        let adapters = vec![
            make_adapter("plumbing-expert", "plumbing"),
        ];
        classifier.sync_from_adapters(&adapters);

        // "plumbing" keyword should now be recognized
        let result = classifier.classify("I need help with plumbing under the sink");
        assert_eq!(result.domain, "plumbing");
        assert_eq!(result.adapter_name, Some("plumbing-expert".to_string()));
    }

    #[test]
    fn test_register_domain_keywords() {
        let mut classifier = DomainClassifier::new();
        classifier.register_domain_keywords("plumbing", vec![
            "pipe".to_string(), "faucet".to_string(), "drain".to_string(),
            "leak".to_string(), "water".to_string(), "plumber".to_string(),
        ]);

        let result = classifier.classify("The pipe under the faucet has a leak and the drain is clogged");
        assert_eq!(result.domain, "plumbing");
        assert!(result.confidence > 0.5, "Multiple keyword matches should give high confidence");
    }

    #[test]
    fn test_adapter_for_domain() {
        let mut classifier = DomainClassifier::new();
        assert!(classifier.adapter_for_domain("code").is_none());

        classifier.sync_from_adapters(&[make_adapter("ts-expert", "code")]);
        assert_eq!(classifier.adapter_for_domain("code"), Some("ts-expert"));
        assert!(classifier.adapter_for_domain("unknown").is_none());
    }

    #[test]
    fn test_classification_speed() {
        let classifier = DomainClassifier::new();
        // Warm up — first call may be slow due to cache effects
        let _ = classifier.classify("warmup text");
        let result = classifier.classify("How do I set up Express routes with async middleware and TypeScript interfaces?");
        // Debug builds are ~50x slower than release; use generous threshold
        // Release target: <1ms, Debug target: <100ms
        let threshold = if cfg!(debug_assertions) { 100_000 } else { 1_000 };
        assert!(
            result.decision_time_us < threshold,
            "Classification should be <{}us, was {}us",
            threshold, result.decision_time_us
        );
    }

    #[test]
    fn test_domain_summary() {
        let mut classifier = DomainClassifier::new();
        classifier.sync_from_adapters(&[make_adapter("ts-expert", "code")]);

        let summary = classifier.domain_summary();
        let code_entry = summary.iter().find(|(d, _)| d == "code");
        assert!(code_entry.is_some());
        assert!(code_entry.unwrap().1, "Code domain should have adapter");

        let conv_entry = summary.iter().find(|(d, _)| d == "conversation");
        assert!(conv_entry.is_some());
        assert!(!conv_entry.unwrap().1, "Conversation domain should NOT have adapter");
    }

    #[test]
    fn test_resync_clears_old_mappings() {
        let mut classifier = DomainClassifier::new();

        // First sync: code has adapter
        classifier.sync_from_adapters(&[make_adapter("ts-expert", "code")]);
        assert_eq!(classifier.adapter_for_domain("code"), Some("ts-expert"));

        // Second sync: different adapter set, code no longer covered
        classifier.sync_from_adapters(&[make_adapter("chat-bot", "conversation")]);
        assert!(classifier.adapter_for_domain("code").is_none(), "Code adapter should be cleared after resync");
        assert_eq!(classifier.adapter_for_domain("conversation"), Some("chat-bot"));
    }

    #[test]
    fn export_bindings_domain_classification() {
        DomainClassification::export_all().unwrap();
    }

    #[test]
    fn export_bindings_quality_score() {
        QualityScore::export_all().unwrap();
    }

    #[test]
    fn export_bindings_quality_factors() {
        QualityFactors::export_all().unwrap();
    }

    // ── Quality Scoring ─────────────────────────────────────────────

    #[test]
    fn test_quality_positive_feedback() {
        let score = score_interaction_quality(
            "How do I fix this bug?",
            "You need to check the null pointer at line 42. Here's the corrected code:\n```\nif (ptr) { ... }\n```",
            Some("Thanks, that's exactly what I needed!"),
            Some(true),
        );
        assert!(score.score > 0.6, "Positive feedback + success should score high, got {}", score.score);
        assert!(score.factors.human_feedback > 0.8);
        assert!(score.factors.task_success > 0.8);
    }

    #[test]
    fn test_quality_negative_feedback() {
        let score = score_interaction_quality(
            "What is 2+2?",
            "5",
            Some("That's wrong, it's 4"),
            Some(false),
        );
        assert!(score.score < 0.5, "Negative feedback + failure should score low, got {}", score.score);
        assert!(score.factors.human_feedback < 0.3);
        assert!(score.factors.correction > 0.5, "Correction signal should be high");
    }

    #[test]
    fn test_quality_no_feedback() {
        let score = score_interaction_quality(
            "Explain how async/await works in TypeScript",
            "Async/await is a syntactic sugar over Promises that makes asynchronous code look synchronous. When you mark a function as async, it returns a Promise.",
            None,
            None,
        );
        // No signals → moderate score based on substance alone
        assert!(score.score > 0.3 && score.score < 0.7, "No feedback should give moderate score, got {}", score.score);
    }

    #[test]
    fn test_quality_short_output_penalized() {
        let score = score_interaction_quality(
            "What is Rust?",
            "A language.",
            None,
            None,
        );
        assert!(score.factors.substance < 0.3, "Very short output should have low substance");
    }
}
