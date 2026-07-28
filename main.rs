#![allow(unused)]
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead};

fn main() -> io::Result<()> {
    // Collect command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file>", args[0]);
        std::process::exit(1);
    }

    // Open the input file with proper error handling
    let filename = &args[1];
    let file = match File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file '{}': {}", filename, e);
            std::process::exit(1);
        },
    };

    // Create a buffer to read the file line by line
    let reader = io::BufReader::new(file);

    // Initialize a HashMap to store word counts
    let mut word_counts = HashMap::new();

    // Process each line in the file
    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                // Normalize and split words
                for word in line.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
                    if !word.is_empty() {
                        *word_counts.entry(word).or_insert(0) += 1;
                    }
                }
            },
            Err(e) => {
                eprintln!("Error reading line: {}", e);
            },
        }
    }

    // Sort words by frequency and print top 10
    let mut sorted_words: Vec<(&str, &usize)> = word_counts.iter().collect();
    sorted_words.sort_by(|a, b| b.1.cmp(a.1));

    println!("Top 10 most frequent words:");
    for (word, count) in sorted_words.into_iter().take(10) {
        println!("{}: {}", word, count);
    }

    Ok(())
}
