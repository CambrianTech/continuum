//! Priority-Based Inference Queue (RTOS-style)
//!
//! Implements a priority queue for inference requests with three levels:
//! - HOT:        User-facing, immediate response (streaming, chat)
//! - WARM:       Standard persona thoughts (AI-to-AI communication)
//! - BACKGROUND: Long-form generation, training, batch processing
//!
//! Architecture:
//! - Requests are sorted by priority, then by arrival time
//! - HOT requests preempt WARM/BACKGROUND
//! - Stats tracked per priority level for monitoring

use log::info;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Priority levels (higher = more urgent)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Background tasks: training, long-form generation, batch processing
    Background = 0,
    /// Standard persona thoughts, AI-to-AI communication
    Warm = 1,
    /// User-facing immediate response (chat, streaming)
    Hot = 2,
}

impl Priority {
    /// Parse from string (for gRPC metadata or config)
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hot" | "high" | "urgent" | "2" => Priority::Hot,
            "warm" | "normal" | "standard" | "1" => Priority::Warm,
            "background" | "low" | "batch" | "0" => Priority::Background,
            _ => Priority::Warm, // Default to Warm
        }
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Request in the priority queue
pub struct PriorityRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f64,
    pub priority: Priority,
    pub submitted_at: Instant,
    pub response_tx: oneshot::Sender<PriorityResponse>,
}

/// For BinaryHeap ordering - higher priority first, then FIFO within priority
impl Ord for PriorityRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // First by priority (higher = more urgent = should come first)
        match self.priority.cmp(&other.priority) {
            Ordering::Greater => Ordering::Greater,
            Ordering::Less => Ordering::Less,
            // Within same priority, earlier submission = higher priority (FIFO)
            Ordering::Equal => other.submitted_at.cmp(&self.submitted_at),
        }
    }
}

impl PartialOrd for PriorityRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for PriorityRequest {}

impl PartialEq for PriorityRequest {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.submitted_at == other.submitted_at
    }
}

/// Response from priority queue
pub struct PriorityResponse {
    pub text: String,
    pub tokens: usize,
    pub duration_ms: u64,
    pub wait_ms: u64, // Time spent waiting in queue
    pub priority: Priority,
    pub error: Option<String>,
}

/// Per-priority statistics
pub struct PriorityStats {
    pub hot_completed: AtomicU64,
    pub hot_total_wait_ms: AtomicU64,
    pub warm_completed: AtomicU64,
    pub warm_total_wait_ms: AtomicU64,
    pub background_completed: AtomicU64,
    pub background_total_wait_ms: AtomicU64,
}

impl PriorityStats {
    pub fn new() -> Self {
        Self {
            hot_completed: AtomicU64::new(0),
            hot_total_wait_ms: AtomicU64::new(0),
            warm_completed: AtomicU64::new(0),
            warm_total_wait_ms: AtomicU64::new(0),
            background_completed: AtomicU64::new(0),
            background_total_wait_ms: AtomicU64::new(0),
        }
    }

    pub fn record(&self, priority: Priority, wait_ms: u64) {
        match priority {
            Priority::Hot => {
                self.hot_completed.fetch_add(1, AtomicOrdering::SeqCst);
                self.hot_total_wait_ms
                    .fetch_add(wait_ms, AtomicOrdering::SeqCst);
            }
            Priority::Warm => {
                self.warm_completed.fetch_add(1, AtomicOrdering::SeqCst);
                self.warm_total_wait_ms
                    .fetch_add(wait_ms, AtomicOrdering::SeqCst);
            }
            Priority::Background => {
                self.background_completed
                    .fetch_add(1, AtomicOrdering::SeqCst);
                self.background_total_wait_ms
                    .fetch_add(wait_ms, AtomicOrdering::SeqCst);
            }
        }
    }

    /// Get average wait times per priority level
    pub fn average_waits(&self) -> (f64, f64, f64) {
        let hot_count = self.hot_completed.load(AtomicOrdering::SeqCst);
        let warm_count = self.warm_completed.load(AtomicOrdering::SeqCst);
        let bg_count = self.background_completed.load(AtomicOrdering::SeqCst);

        let hot_avg = if hot_count > 0 {
            self.hot_total_wait_ms.load(AtomicOrdering::SeqCst) as f64 / hot_count as f64
        } else {
            0.0
        };

        let warm_avg = if warm_count > 0 {
            self.warm_total_wait_ms.load(AtomicOrdering::SeqCst) as f64 / warm_count as f64
        } else {
            0.0
        };

        let bg_avg = if bg_count > 0 {
            self.background_total_wait_ms.load(AtomicOrdering::SeqCst) as f64 / bg_count as f64
        } else {
            0.0
        };

        (hot_avg, warm_avg, bg_avg)
    }
}

impl Default for PriorityStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Priority-based inference queue
///
/// Usage:
/// ```ignore
/// let queue = PriorityQueue::new(generate_fn);
/// let rx = queue.submit("prompt", 100, 0.7, Priority::Hot).await?;
/// let response = rx.await?;
/// ```
pub struct PriorityQueue {
    request_tx: mpsc::Sender<PriorityRequest>,
    stats: Arc<PriorityStats>,
}

impl PriorityQueue {
    /// Create a new priority queue with a generation function
    ///
    /// The generation function takes (prompt, max_tokens, temperature) and
    /// returns Result<(text, token_count), error_string>
    pub fn new<F>(generate_fn: F) -> Self
    where
        F: Fn(&str, usize, f64) -> Result<(String, usize), String> + Send + Sync + 'static,
    {
        let (request_tx, request_rx) = mpsc::channel::<PriorityRequest>(64);
        let stats = Arc::new(PriorityStats::new());

        // Spawn the priority queue processor
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            Self::run_queue(request_rx, generate_fn, stats_clone).await;
        });

        Self { request_tx, stats }
    }

    /// Main queue processing loop
    async fn run_queue<F>(
        mut request_rx: mpsc::Receiver<PriorityRequest>,
        generate_fn: F,
        stats: Arc<PriorityStats>,
    ) where
        F: Fn(&str, usize, f64) -> Result<(String, usize), String> + Send + Sync + 'static,
    {
        // Priority heap for pending requests
        let heap: Arc<Mutex<BinaryHeap<PriorityRequest>>> =
            Arc::new(Mutex::new(BinaryHeap::new()));

        // Spawn receiver that adds to heap
        let heap_clone = heap.clone();
        tokio::spawn(async move {
            while let Some(request) = request_rx.recv().await {
                let mut heap = heap_clone.lock().await;
                heap.push(request);
            }
        });

        // Main processing loop - continuously process highest priority
        loop {
            // Get next request (if any)
            let request = {
                let mut heap = heap.lock().await;
                heap.pop()
            };

            match request {
                Some(req) => {
                    let wait_ms = req.submitted_at.elapsed().as_millis() as u64;
                    let priority = req.priority;
                    let start = Instant::now();

                    // Log priority info
                    info!(
                        "⚡ Processing {:?} priority request (waited {}ms)",
                        priority, wait_ms
                    );

                    // Generate response
                    let result = generate_fn(&req.prompt, req.max_tokens, req.temperature);
                    let duration_ms = start.elapsed().as_millis() as u64;

                    // Record stats
                    stats.record(priority, wait_ms);

                    // Build response
                    let response = match result {
                        Ok((text, tokens)) => PriorityResponse {
                            text,
                            tokens,
                            duration_ms,
                            wait_ms,
                            priority,
                            error: None,
                        },
                        Err(e) => PriorityResponse {
                            text: String::new(),
                            tokens: 0,
                            duration_ms,
                            wait_ms,
                            priority,
                            error: Some(e),
                        },
                    };

                    // Send response (ignore if receiver dropped)
                    let _ = req.response_tx.send(response);
                }
                None => {
                    // No requests, sleep briefly to avoid busy-loop
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }
    }

    /// Submit a request with priority
    pub async fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        temperature: f64,
        priority: Priority,
    ) -> Result<oneshot::Receiver<PriorityResponse>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        let request = PriorityRequest {
            prompt,
            max_tokens,
            temperature,
            priority,
            submitted_at: Instant::now(),
            response_tx,
        };

        self.request_tx
            .send(request)
            .await
            .map_err(|e| format!("Failed to submit: {}", e))?;

        Ok(response_rx)
    }

    /// Get current statistics
    pub fn stats(&self) -> &Arc<PriorityStats> {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Hot > Priority::Warm);
        assert!(Priority::Warm > Priority::Background);
        assert!(Priority::Hot > Priority::Background);
    }

    #[test]
    fn test_priority_from_str() {
        assert_eq!(Priority::from_str("hot"), Priority::Hot);
        assert_eq!(Priority::from_str("HIGH"), Priority::Hot);
        assert_eq!(Priority::from_str("warm"), Priority::Warm);
        assert_eq!(Priority::from_str("background"), Priority::Background);
        assert_eq!(Priority::from_str("batch"), Priority::Background);
        assert_eq!(Priority::from_str("unknown"), Priority::Warm); // default
    }
}
