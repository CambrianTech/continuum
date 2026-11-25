#![cfg(test)]
use super::*;

/// A tokenizer where `<|im_end|>` is the one control token, id 7.
fn control(stop: &str) -> Option<i32> {
    (stop == "<|im_end|>").then_some(7)
}

/// An answer opened with `stops`, sorted by the fake tokenizer.
fn open(stops: &[&str]) -> Open {
    let stops: Vec<String> = stops.iter().map(|stop| stop.to_string()).collect();
    let stops = Stops::of(&stops, control);
    let (sink, _) = tokio::sync::mpsc::unbounded_channel();
    Open { tail: Tail::with_capacity(stops.longest()), sink, stops, cancel: Arc::new(AtomicBool::new(false)), piece: Vec::new() }
}

/// Feed `pieces` (each with its token id); the index of the piece that stopped it, if any.
fn first_stop(open: &mut Open, pieces: &[(&str, i32)]) -> Option<usize> {
    pieces.iter().position(|(piece, token)| {
        open.tail.push(piece.as_bytes());
        open.stopped_by(*token)
    })
}

// what this catches: a text stop that tokenizes across two pieces ("\n\nUs" + "er:") missed
// because each piece is checked alone, or a tail too short to hold it.
#[test]
fn a_text_stop_split_across_two_tokens_is_caught() {
    let mut answer = open(&["\n\nUser:"]);
    let pieces = [("Hello", 100), (" there.", 101), ("\n\nUs", 102), ("er:", 103), (" more", 104)];
    assert_eq!(first_stop(&mut answer, &pieces), Some(3));
}

// what this catches: a control-token stop matched as text, which never fires because its piece
// renders as nothing; and a control token that is not a stop ending the answer.
#[test]
fn a_control_token_stop_fires_on_its_id_even_with_no_text() {
    let mut answer = open(&["<|im_end|>"]);
    assert!(answer.stops.texts.is_empty(), "the control stop is watched by id, never as text");
    assert_eq!(first_stop(&mut answer, &[("Done", 100), ("", 8), ("", 7)]), Some(2), "id 8 is another control token, id 7 the stop");
}

// what this catches: the watched text growing with the answer (the old whole-answer String).
// A long answer with no stop leaves the tail at the longest stop's bytes, and a stop at its very
// end is still seen.
#[test]
fn the_tail_stays_the_longest_stops_size_over_a_long_answer() {
    let mut answer = open(&["</answer>", "STOP"]);
    let capacity = answer.tail.bytes.len();
    assert_eq!(capacity, "</answer>".len());
    for i in 0..100_000 {
        answer.tail.push(if i % 3 == 0 { b"a long piece of text " } else { b"x" });
        assert!(!answer.stopped_by(100));
        assert_eq!(answer.tail.bytes.len(), capacity, "the buffer never grows");
        assert!(answer.tail.len <= capacity);
    }
    answer.tail.push(b"</ans");
    answer.tail.push(b"wer>");
    assert!(answer.stopped_by(101), "a stop at the end of a long answer is still caught");
}

// what this catches: an answer with no stops paying for a tail, or an empty stop ending every
// answer at its first token.
#[test]
fn no_stops_and_empty_stops_hold_no_tail_and_never_stop() {
    let mut answer = open(&["", ""]);
    assert_eq!(answer.tail.bytes.len(), 0);
    assert_eq!(first_stop(&mut answer, &[("anything", 100), ("at all", 101)]), None);
}
