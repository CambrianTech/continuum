fn reverse_words(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ")
}
