/// Reverses the order of whitespace-separated words in `s`.
fn reverse_words(s: &str) -> String {
    s.split_whitespace()
        .rev()
        .collect::<Vec<&str>>()
        .join(" ")
}
