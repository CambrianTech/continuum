//! VAD Evaluation Metrics
//!
//! Provides precision, recall, F1 score, and confusion matrix analysis
//! for evaluating Voice Activity Detection performance.

use serde::{Deserialize, Serialize};

/// Ground truth label for a frame
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroundTruth {
    Speech,
    Silence,
}

/// VAD prediction for a frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prediction {
    Speech,
    Silence,
}

/// Classification outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    TruePositive,  // Predicted speech, was speech
    TrueNegative,  // Predicted silence, was silence
    FalsePositive, // Predicted speech, was silence
    FalseNegative, // Predicted silence, was speech
}

impl Outcome {
    pub fn from_prediction(prediction: Prediction, ground_truth: GroundTruth) -> Self {
        match (prediction, ground_truth) {
            (Prediction::Speech, GroundTruth::Speech) => Outcome::TruePositive,
            (Prediction::Silence, GroundTruth::Silence) => Outcome::TrueNegative,
            (Prediction::Speech, GroundTruth::Silence) => Outcome::FalsePositive,
            (Prediction::Silence, GroundTruth::Speech) => Outcome::FalseNegative,
        }
    }
}

/// Confusion matrix for binary classification
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ConfusionMatrix {
    pub true_positives: usize,
    pub true_negatives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

impl ConfusionMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a prediction outcome
    pub fn record(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::TruePositive => self.true_positives += 1,
            Outcome::TrueNegative => self.true_negatives += 1,
            Outcome::FalsePositive => self.false_positives += 1,
            Outcome::FalseNegative => self.false_negatives += 1,
        }
    }

    /// Total number of samples
    pub fn total(&self) -> usize {
        self.true_positives + self.true_negatives + self.false_positives + self.false_negatives
    }

    /// Accuracy: (TP + TN) / Total
    pub fn accuracy(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.true_positives + self.true_negatives) as f64 / total as f64
    }

    /// Precision: TP / (TP + FP)
    /// "Of all predicted speech, how much was actually speech?"
    pub fn precision(&self) -> f64 {
        let denominator = self.true_positives + self.false_positives;
        if denominator == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denominator as f64
    }

    /// Recall (Sensitivity, True Positive Rate): TP / (TP + FN)
    /// "Of all actual speech, how much did we detect?"
    pub fn recall(&self) -> f64 {
        let denominator = self.true_positives + self.false_negatives;
        if denominator == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denominator as f64
    }

    /// F1 Score: 2 * (Precision * Recall) / (Precision + Recall)
    /// Harmonic mean of precision and recall
    pub fn f1_score(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if p + r == 0.0 {
            return 0.0;
        }
        2.0 * (p * r) / (p + r)
    }

    /// Specificity (True Negative Rate): TN / (TN + FP)
    /// "Of all actual silence, how much did we correctly identify?"
    pub fn specificity(&self) -> f64 {
        let denominator = self.true_negatives + self.false_positives;
        if denominator == 0 {
            return 0.0;
        }
        self.true_negatives as f64 / denominator as f64
    }

    /// False Positive Rate: FP / (FP + TN)
    pub fn false_positive_rate(&self) -> f64 {
        1.0 - self.specificity()
    }

    /// False Negative Rate: FN / (FN + TP)
    pub fn false_negative_rate(&self) -> f64 {
        1.0 - self.recall()
    }

    /// Matthews Correlation Coefficient: measures quality of binary classifications
    /// Range: [-1, 1] where 1 = perfect, 0 = random, -1 = total disagreement
    pub fn mcc(&self) -> f64 {
        let tp = self.true_positives as f64;
        let tn = self.true_negatives as f64;
        let fp = self.false_positives as f64;
        let fn_val = self.false_negatives as f64;

        let numerator = (tp * tn) - (fp * fn_val);
        let denominator = ((tp + fp) * (tp + fn_val) * (tn + fp) * (tn + fn_val)).sqrt();

        if denominator == 0.0 {
            return 0.0;
        }

        numerator / denominator
    }

    /// Pretty print the confusion matrix
    pub fn display(&self) -> String {
        format!(
            r#"
Confusion Matrix:
                Predicted
                Speech  Silence
Actual Speech   {:6}  {:6}  (TP, FN)
       Silence  {:6}  {:6}  (FP, TN)

Metrics:
  Accuracy:    {:.3} ({:.1}%)
  Precision:   {:.3} ({:.1}%)
  Recall:      {:.3} ({:.1}%)
  F1 Score:    {:.3}
  Specificity: {:.3} ({:.1}%)
  FPR:         {:.3} ({:.1}%)
  FNR:         {:.3} ({:.1}%)
  MCC:         {:.3}

Total Samples: {}
"#,
            self.true_positives,
            self.false_negatives,
            self.false_positives,
            self.true_negatives,
            self.accuracy(),
            self.accuracy() * 100.0,
            self.precision(),
            self.precision() * 100.0,
            self.recall(),
            self.recall() * 100.0,
            self.f1_score(),
            self.specificity(),
            self.specificity() * 100.0,
            self.false_positive_rate(),
            self.false_positive_rate() * 100.0,
            self.false_negative_rate(),
            self.false_negative_rate() * 100.0,
            self.mcc(),
            self.total()
        )
    }
}

/// VAD Evaluator - compares predictions against ground truth
pub struct VADEvaluator {
    matrix: ConfusionMatrix,
    predictions: Vec<(Prediction, GroundTruth, f32)>, // prediction, truth, confidence
}

impl VADEvaluator {
    pub fn new() -> Self {
        Self {
            matrix: ConfusionMatrix::new(),
            predictions: Vec::new(),
        }
    }

    /// Record a prediction with ground truth
    pub fn record(&mut self, prediction: bool, ground_truth: GroundTruth, confidence: f32) {
        let pred = if prediction {
            Prediction::Speech
        } else {
            Prediction::Silence
        };

        let outcome = Outcome::from_prediction(pred, ground_truth);
        self.matrix.record(outcome);
        self.predictions.push((pred, ground_truth, confidence));
    }

    /// Get the confusion matrix
    pub fn matrix(&self) -> &ConfusionMatrix {
        &self.matrix
    }

    /// Get all predictions (for ROC curve analysis)
    pub fn predictions(&self) -> &[(Prediction, GroundTruth, f32)] {
        &self.predictions
    }

    /// Calculate precision at different confidence thresholds
    /// Returns: Vec<(threshold, precision, recall, f1)>
    pub fn precision_recall_curve(&self, num_points: usize) -> Vec<(f32, f64, f64, f64)> {
        let mut thresholds: Vec<f32> = (0..=num_points)
            .map(|i| i as f32 / num_points as f32)
            .collect();

        thresholds.sort_by(|a, b| a.partial_cmp(b).unwrap());

        thresholds
            .into_iter()
            .map(|threshold| {
                let mut matrix = ConfusionMatrix::new();

                for &(_pred, truth, confidence) in &self.predictions {
                    // Re-classify based on threshold
                    let new_pred = if confidence >= threshold {
                        Prediction::Speech
                    } else {
                        Prediction::Silence
                    };

                    let outcome = Outcome::from_prediction(new_pred, truth);
                    matrix.record(outcome);
                }

                (
                    threshold,
                    matrix.precision(),
                    matrix.recall(),
                    matrix.f1_score(),
                )
            })
            .collect()
    }

    /// Find optimal threshold that maximizes F1 score
    pub fn optimal_threshold(&self) -> (f32, f64) {
        let curve = self.precision_recall_curve(100);

        let (best_threshold, best_f1) = curve
            .into_iter()
            .max_by(|(_, _, _, f1_a), (_, _, _, f1_b)| {
                f1_a.partial_cmp(f1_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(threshold, _p, _r, f1)| (threshold, f1))
            .unwrap_or((0.5, 0.0));

        (best_threshold, best_f1)
    }

    /// Generate summary report
    pub fn report(&self) -> String {
        let matrix_display = self.matrix.display();
        let (optimal_threshold, optimal_f1) = self.optimal_threshold();

        format!(
            "{matrix_display}\nOptimal Threshold: {optimal_threshold:.3} (F1: {optimal_f1:.3})"
        )
    }
}

impl Default for VADEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confusion_matrix_perfect() {
        let mut matrix = ConfusionMatrix::new();

        // Perfect classifier: 10 speech, 10 silence, all correct
        for _ in 0..10 {
            matrix.record(Outcome::TruePositive);
            matrix.record(Outcome::TrueNegative);
        }

        assert_eq!(matrix.accuracy(), 1.0);
        assert_eq!(matrix.precision(), 1.0);
        assert_eq!(matrix.recall(), 1.0);
        assert_eq!(matrix.f1_score(), 1.0);
        assert_eq!(matrix.specificity(), 1.0);
        assert_eq!(matrix.mcc(), 1.0);
    }

    #[test]
    fn test_confusion_matrix_all_wrong() {
        let mut matrix = ConfusionMatrix::new();

        // Worst classifier: predicts opposite of truth
        for _ in 0..10 {
            matrix.record(Outcome::FalsePositive);
            matrix.record(Outcome::FalseNegative);
        }

        assert_eq!(matrix.accuracy(), 0.0);
        assert_eq!(matrix.precision(), 0.0);
        assert_eq!(matrix.recall(), 0.0);
        assert_eq!(matrix.specificity(), 0.0);
    }

    #[test]
    fn test_evaluator() {
        let mut evaluator = VADEvaluator::new();

        // 8 correct speech detections
        for _ in 0..8 {
            evaluator.record(true, GroundTruth::Speech, 0.9);
        }

        // 2 missed speech (false negatives)
        for _ in 0..2 {
            evaluator.record(false, GroundTruth::Speech, 0.3);
        }

        // 9 correct silence detections
        for _ in 0..9 {
            evaluator.record(false, GroundTruth::Silence, 0.1);
        }

        // 1 false positive
        evaluator.record(true, GroundTruth::Silence, 0.6);

        let matrix = evaluator.matrix();
        assert_eq!(matrix.true_positives, 8);
        assert_eq!(matrix.false_negatives, 2);
        assert_eq!(matrix.true_negatives, 9);
        assert_eq!(matrix.false_positives, 1);

        assert_eq!(matrix.accuracy(), 17.0 / 20.0); // (8+9)/20 = 0.85
        assert_eq!(matrix.precision(), 8.0 / 9.0);  // 8/(8+1) ≈ 0.889
        assert_eq!(matrix.recall(), 8.0 / 10.0);    // 8/(8+2) = 0.8
    }

    #[test]
    fn test_optimal_threshold() {
        let mut evaluator = VADEvaluator::new();

        // High confidence speech
        for _ in 0..10 {
            evaluator.record(true, GroundTruth::Speech, 0.9);
        }

        // Low confidence silence
        for _ in 0..10 {
            evaluator.record(false, GroundTruth::Silence, 0.1);
        }

        let (threshold, f1) = evaluator.optimal_threshold();

        // Should find threshold around 0.5 with perfect F1
        assert!(threshold > 0.0 && threshold < 1.0);
        assert!(f1 > 0.9); // Near perfect
    }
}
