use tokio::sync::mpsc;
use std::sync::Arc;

/// Trait for processing messages concurrently
///
/// OOP interface pattern - implement this for any message type
#[async_trait::async_trait]
pub trait MessageProcessor: Send + Sync {
    type Message: Send + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Process a single message
    async fn process(&self, message: Self::Message) -> Result<(), Self::Error>;

    /// Called when processor starts
    async fn on_start(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called when processor stops
    async fn on_stop(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Concurrent message processor using worker pool
///
/// Pattern: N worker tasks pull from shared channel (work-stealing)
pub struct ConcurrentProcessor<P: MessageProcessor> {
    tx: mpsc::UnboundedSender<P::Message>,
    #[allow(dead_code)] // Kept to maintain Arc reference count
    processor: Arc<P>,
}

impl<P: MessageProcessor + 'static> ConcurrentProcessor<P> {
    /// Create processor with N worker tasks
    pub fn new(processor: P, worker_count: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = Arc::new(tokio::sync::Mutex::new(rx));
        let processor = Arc::new(processor);

        // Spawn worker pool
        for worker_id in 0..worker_count {
            let rx = rx.clone();
            let processor = processor.clone();

            tokio::spawn(async move {
                if let Err(e) = processor.on_start().await {
                    eprintln!("Worker {worker_id}: start error: {e}");
                    return;
                }

                loop {
                    let message = {
                        let mut rx = rx.lock().await;
                        rx.recv().await
                    };

                    match message {
                        Some(msg) => {
                            if let Err(e) = processor.process(msg).await {
                                eprintln!("Worker {worker_id}: process error: {e}");
                            }
                        }
                        None => break,  // Channel closed
                    }
                }

                let _ = processor.on_stop().await;
            });
        }

        Self { tx, processor }
    }

    /// Submit message for processing (non-blocking)
    pub fn submit(&self, message: P::Message) {
        let _ = self.tx.send(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestProcessor {
        counter: Arc<AtomicUsize>,
    }

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error("test error")]
        #[allow(dead_code)]
        Test,
    }

    #[async_trait::async_trait]
    impl MessageProcessor for TestProcessor {
        type Message = u32;
        type Error = TestError;

        async fn process(&self, _message: Self::Message) -> Result<(), Self::Error> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_concurrent_processor() {
        let counter = Arc::new(AtomicUsize::new(0));
        let processor = TestProcessor { counter: counter.clone() };
        let concurrent = ConcurrentProcessor::new(processor, 4);

        // Submit 100 messages
        for i in 0..100 {
            concurrent.submit(i);
        }

        // Wait for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // All messages should be processed
        assert_eq!(counter.load(Ordering::SeqCst), 100);
    }
}
