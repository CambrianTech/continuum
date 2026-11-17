//! A measurement receipt for card 9cf06ade slice 1 (#6104), run once on a non-serving CI executor and
//! never merged: the cost of cloning a real-size request whose tool head is an `Arc<[NativeToolSpec]>`
//! (after) against deep-copying the same head as an owned list (before, what `Option<Vec<_>>` did),
//! with allocation counts from a counting allocator in this test binary only, and wire equality.
//! Real size, the M5 2026-11-17 13:45Z: Aiko's submitted request carried 32 tool schemas (25,776
//! bytes of JSON, the fixture: our own command schemas), 483 messages (626,535 bytes) and an
//! 8,138-byte system prompt. The messages here are synthetic at those sizes; none of hers are copied.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use continuum_engine::ai::types::{ChatMessage, MessageContent, NativeToolSpec, TextGenerationRequest};

struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: forwards the caller's layout to the system allocator unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by `alloc` above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Median nanoseconds, allocations and bytes allocated per call of `clone_once`.
fn measure<T>(rounds: usize, mut clone_once: impl FnMut() -> T) -> (u128, usize, usize) {
    let mut nanos = Vec::with_capacity(rounds);
    let (allocations, bytes) = (ALLOCATIONS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
    for _ in 0..rounds {
        let started = Instant::now();
        let kept = clone_once();
        nanos.push(started.elapsed().as_nanos());
        drop(kept);
    }
    let (allocations, bytes) = (ALLOCATIONS.load(Ordering::Relaxed) - allocations, BYTES.load(Ordering::Relaxed) - bytes);
    nanos.sort_unstable();
    (nanos[rounds / 2], allocations / rounds, bytes / rounds)
}

#[test]
#[ignore = "a one-off measurement receipt, run with --ignored --nocapture"]
fn request_clone_receipt() {
    let head: Arc<[NativeToolSpec]> = serde_json::from_str::<Vec<NativeToolSpec>>(include_str!("fixtures/resident_tool_head_2026-11-17.json")).expect("the captured schemas decode").into();
    assert_eq!(head.len(), 32);
    let message = |index: usize| ChatMessage { role: if index % 2 == 0 { "user" } else { "assistant" }.to_string(), content: MessageContent::Text("x".repeat(626_535 / 483)), name: None };
    let request = TextGenerationRequest { messages: (0..483).map(message).collect(), system_prompt: Some("s".repeat(8_138)), tools: Some(Arc::clone(&head)), ..Default::default() };
    let rounds = 501;

    let (head_after_ns, head_after_allocs, head_after_bytes) = measure(rounds, || request.tools.clone());
    let (head_before_ns, head_before_allocs, head_before_bytes) = measure(rounds, || head.to_vec());
    let (request_after_ns, request_after_allocs, request_after_bytes) = measure(rounds, || request.clone());
    let (request_before_ns, request_before_allocs, request_before_bytes) = measure(rounds, || (request.clone(), head.to_vec()));

    let shared_wire = serde_json::to_vec(&request.tools).expect("encodes");
    let owned_wire = serde_json::to_vec(&Some(head.to_vec())).expect("encodes");
    assert_eq!(shared_wire, owned_wire, "wire bytes are identical whoever holds the schemas");
    let cloned = request.clone();
    assert!(Arc::ptr_eq(cloned.tools.as_ref().expect("tools"), &head), "the clone shares the head");

    println!("RECEIPT 9cf06ade slice 1 (median of {rounds}; per clone)");
    println!("RECEIPT tool head  before(owned Vec) {head_before_ns} ns, {head_before_allocs} allocs, {head_before_bytes} B | after(Arc) {head_after_ns} ns, {head_after_allocs} allocs, {head_after_bytes} B");
    println!("RECEIPT whole req  before(+deep head) {request_before_ns} ns, {request_before_allocs} allocs, {request_before_bytes} B | after {request_after_ns} ns, {request_after_allocs} allocs, {request_after_bytes} B");
    println!("RECEIPT wire equal: {} bytes, identical", shared_wire.len());
}
