use std::collections::HashMap;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file>", args[0]);
        return;
    }

    let filename = &args[1];
    let contents = std::fs::read_to_string(filename).expect("Failed to read file");

    let mut word_counts = HashMap::new();

    for word in contents.split_whitespace() {
        *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }

    // Sort by value, then take the top 10
    let mut top_words: Vec<(&String, &i32)> = word_counts.iter().collect();
    top_words.sort_by(|a, b| b.1.cmp(a.1));
    
    println!("Top 10 words by frequency:");
    for (word, count) in top_words.into_iter().take(10) {
        println!("{}: {}", word, count);
    }
}