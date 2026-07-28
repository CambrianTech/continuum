fn main() {
    println!("Hello, world!");
}
#![allow(unused)]

use std::collections::HashMap;

fn count_words(text: &str) -> HashMap<String, usize> {
    let mut word_count = HashMap::new();
    for word in text.split_whitespace() {
        *word_count.entry(word.to_string()).or_insert(0) += 1;
    }
    word_count
}

fn main() {
    let text = "hello world hello rust world rust hello";
    let counts = count_words(text);
    for (word, count) in counts.iter() {
        println("{} : {}", word, count);
    }
}