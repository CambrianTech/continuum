// Grading harness for card b2f0818b (eval id: reverse_words).
// Links the standalone `reverse_words` rlib and runs the exact expected
// assertions from docs/genome/coder-write-eval.jsonl.
use reverse_words::reverse_words;

fn main() {
    assert_eq!(reverse_words("hello world foo"), "foo world hello");
    assert_eq!(reverse_words("single"), "single");
    println!("PASS: both expected assertions hold (card b2f0818b)");
}
