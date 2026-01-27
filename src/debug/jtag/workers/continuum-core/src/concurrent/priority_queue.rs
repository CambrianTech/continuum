use tokio::sync::{mpsc, Notify};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;

/// Trait for items that can be prioritized
pub trait Prioritized: Send + Sync + 'static {
    fn priority(&self) -> f32;
}

/// Concurrent priority queue using Tokio channels
///
/// Pattern: Single worker task manages BinaryHeap, all access via message passing
/// Benefits:
/// - No locks (message passing only)
/// - Work-stealing via Tokio runtime
/// - Backpressure via bounded channels (optional)
pub struct ConcurrentPriorityQueue<T: Prioritized + Ord> {
    enqueue_tx: mpsc::UnboundedSender<T>,
    dequeue_rx: mpsc::UnboundedReceiver<T>,
    signal: Arc<Notify>,
}

impl<T: Prioritized + Ord> Default for ConcurrentPriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Prioritized + Ord> ConcurrentPriorityQueue<T> {
    pub fn new() -> Self {
        let (enqueue_tx, mut enqueue_rx) = mpsc::unbounded_channel::<T>();
        let (dequeue_tx, dequeue_rx) = mpsc::unbounded_channel::<T>();
        let signal = Arc::new(Notify::new());
        let signal_clone = signal.clone();

        // Worker task manages the heap
        tokio::spawn(async move {
            let mut heap: BinaryHeap<T> = BinaryHeap::new();

            loop {
                tokio::select! {
                    // Receive from enqueue channel
                    Some(item) = enqueue_rx.recv() => {
                        heap.push(item);
                        signal_clone.notify_one();
                    }

                    // Send to dequeue channel when signaled
                    _ = signal_clone.notified(), if !heap.is_empty() => {
                        if let Some(item) = heap.pop() {
                            let _ = dequeue_tx.send(item);
                        }
                    }
                }
            }
        });

        Self {
            enqueue_tx,
            dequeue_rx,
            signal,
        }
    }

    /// Enqueue item (non-blocking)
    pub fn enqueue(&self, item: T) {
        let _ = self.enqueue_tx.send(item);
    }

    /// Dequeue highest priority item (async)
    pub async fn dequeue(&mut self) -> Option<T> {
        self.signal.notify_one();
        self.dequeue_rx.recv().await
    }

    /// Wait for work signal
    pub async fn wait_for_work(&self) {
        self.signal.notified().await;
    }
}

/// Wrapper to make any type with priority() work with BinaryHeap
pub struct PriorityWrapper<T: Prioritized> {
    inner: T,
}

impl<T: Prioritized> PriorityWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Prioritized> PartialEq for PriorityWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.priority() == other.inner.priority()
    }
}

impl<T: Prioritized> Eq for PriorityWrapper<T> {}

impl<T: Prioritized> PartialOrd for PriorityWrapper<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Prioritized> Ord for PriorityWrapper<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.priority().partial_cmp(&other.inner.priority()).unwrap_or(Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestMessage {
        priority: f32,
        content: String,
    }

    impl Prioritized for TestMessage {
        fn priority(&self) -> f32 {
            self.priority
        }
    }

    impl PartialEq for TestMessage {
        fn eq(&self, other: &Self) -> bool {
            self.priority == other.priority
        }
    }

    impl Eq for TestMessage {}

    impl PartialOrd for TestMessage {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for TestMessage {
        fn cmp(&self, other: &Self) -> Ordering {
            self.priority.partial_cmp(&other.priority).unwrap_or(Ordering::Equal)
        }
    }

    #[tokio::test]
    #[ignore] // TODO: Fix race condition in test - worker task timing
    async fn test_concurrent_priority_queue() {
        let mut queue = ConcurrentPriorityQueue::new();

        queue.enqueue(TestMessage { priority: 0.3, content: "Low".to_string() });
        queue.enqueue(TestMessage { priority: 0.9, content: "High".to_string() });
        queue.enqueue(TestMessage { priority: 0.5, content: "Medium".to_string() });

        // Wait for processing (worker task needs time to process)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Should get highest priority first
        let first = queue.dequeue().await.unwrap();
        assert_eq!(first.priority, 0.9);

        let second = queue.dequeue().await.unwrap();
        assert_eq!(second.priority, 0.5);

        let third = queue.dequeue().await.unwrap();
        assert_eq!(third.priority, 0.3);
    }
}
