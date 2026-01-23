//! Lock-Free Ring Buffer
//!
//! Fixed allocation at startup. Slots recycled. Never grows.
//! Backpressure via blocking when full.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tokio::sync::Notify;

/// Slot reference - 8 bytes total, this is what gets passed around
#[derive(Debug, Clone, Copy)]
pub struct SlotRef {
    pub ring_id: u16,
    pub slot: u16,
    pub generation: u32,
}

impl SlotRef {
    /// Check if this ref is still valid (generation matches)
    pub fn is_valid(&self, current_generation: u32) -> bool {
        self.generation == current_generation
    }
}

/// Ring buffer with fixed capacity
///
/// Generic over T (the frame type) and N (capacity).
/// Uses atomic operations for lock-free single-producer/single-consumer.
pub struct RingBuffer<T, const N: usize> {
    /// Ring ID for SlotRef creation
    ring_id: u16,

    /// The slots - fixed allocation
    slots: Box<[Mutex<Option<T>>; N]>,

    /// Write position (producer)
    write_pos: AtomicUsize,

    /// Read position (consumer)
    read_pos: AtomicUsize,

    /// Generation counter - increments on wrap for stale detection
    generation: AtomicU64,

    /// Notify when slot becomes available (for blocking push)
    slot_available: Notify,

    /// Notify when data becomes available (for blocking pop)
    data_available: Notify,
}

impl<T, const N: usize> RingBuffer<T, N> {
    /// Create a new ring buffer with given ID
    pub fn new(ring_id: u16) -> Self {
        // Initialize slots array
        let slots: Vec<Mutex<Option<T>>> = (0..N).map(|_| Mutex::new(None)).collect();
        let slots: Box<[Mutex<Option<T>>; N]> = slots.into_boxed_slice().try_into().ok().unwrap();

        Self {
            ring_id,
            slots,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            slot_available: Notify::new(),
            data_available: Notify::new(),
        }
    }

    /// Get ring capacity
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Get current length (items in buffer)
    pub fn len(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        write.wrapping_sub(read)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if full
    pub fn is_full(&self) -> bool {
        self.len() >= N
    }

    /// Try to push (non-blocking). Returns None if full.
    pub fn try_push(&self, item: T) -> Option<SlotRef> {
        if self.is_full() {
            return None;
        }

        let write = self.write_pos.load(Ordering::Acquire);
        let slot_idx = write % N;

        // Store item
        {
            let mut slot = self.slots[slot_idx].lock();
            *slot = Some(item);
        }

        // Advance write position
        self.write_pos.fetch_add(1, Ordering::Release);

        // Check for wrap and increment generation
        if (write + 1) % N == 0 {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }

        // Notify waiting consumers
        self.data_available.notify_one();

        Some(SlotRef {
            ring_id: self.ring_id,
            slot: slot_idx as u16,
            generation: self.generation.load(Ordering::Relaxed) as u32,
        })
    }

    /// Push with blocking (async). Waits for slot to become available.
    pub async fn push(&self, item: T) -> SlotRef {
        loop {
            // Check if we can push (without consuming item)
            if !self.is_full() {
                // Safe to push now
                return self.try_push(item).expect("push after is_full check");
            }
            // Wait for slot to become available
            self.slot_available.notified().await;
        }
    }

    /// Try to pop (non-blocking). Returns None if empty.
    pub fn try_pop(&self) -> Option<(SlotRef, T)> {
        if self.is_empty() {
            return None;
        }

        let read = self.read_pos.load(Ordering::Acquire);
        let slot_idx = read % N;

        // Take item
        let item = {
            let mut slot = self.slots[slot_idx].lock();
            slot.take()?
        };

        let slot_ref = SlotRef {
            ring_id: self.ring_id,
            slot: slot_idx as u16,
            generation: self.generation.load(Ordering::Relaxed) as u32,
        };

        // Advance read position
        self.read_pos.fetch_add(1, Ordering::Release);

        // Notify waiting producers
        self.slot_available.notify_one();

        Some((slot_ref, item))
    }

    /// Pop with blocking (async). Waits for data to become available.
    pub async fn pop(&self) -> (SlotRef, T) {
        loop {
            if let Some(result) = self.try_pop() {
                return result;
            }
            // Wait for data to become available
            self.data_available.notified().await;
        }
    }

    /// Peek at slot without consuming (for read-only access)
    /// Returns a guard that derefs to the item
    pub fn peek(&self, slot_ref: &SlotRef) -> Option<PeekGuard<'_, T>> {
        if slot_ref.ring_id != self.ring_id {
            return None;
        }

        let slot = self.slots[slot_ref.slot as usize].lock();
        if slot.is_some() {
            Some(PeekGuard { guard: slot })
        } else {
            None
        }
    }
}

/// Guard for peeking at ring buffer contents
pub struct PeekGuard<'a, T> {
    guard: parking_lot::MutexGuard<'a, Option<T>>,
}

impl<'a, T> std::ops::Deref for PeekGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}

// RingBuffer is Send + Sync if T is Send
unsafe impl<T: Send, const N: usize> Send for RingBuffer<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for RingBuffer<T, N> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let ring: RingBuffer<i32, 4> = RingBuffer::new(0);

        assert!(ring.is_empty());
        assert!(!ring.is_full());

        let slot = ring.try_push(1).unwrap();
        assert_eq!(ring.len(), 1);
        assert_eq!(slot.slot, 0);

        let (_, item) = ring.try_pop().unwrap();
        assert_eq!(item, 1);
        assert!(ring.is_empty());
    }

    #[test]
    fn test_ring_buffer_full() {
        let ring: RingBuffer<i32, 2> = RingBuffer::new(0);

        ring.try_push(1).unwrap();
        ring.try_push(2).unwrap();

        assert!(ring.is_full());
        assert!(ring.try_push(3).is_none()); // Should fail - full
    }

    #[tokio::test]
    async fn test_ring_buffer_async() {
        let ring: RingBuffer<i32, 4> = RingBuffer::new(0);

        let slot = ring.push(42).await;
        assert_eq!(slot.slot, 0);

        let (_, item) = ring.pop().await;
        assert_eq!(item, 42);
    }
}
