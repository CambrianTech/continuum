use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: wordstats <file_path>");
        return Ok(());
    }

    let file_path = &args[1];
    let file = File::open(file_path)?;
    let reader = io::BufReader::new(file);

    let mut word_counts: HashMap<String, usize> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let words = line.split_whitespace();
        for word in words {
            *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
        }
    }

    // Collect the top-10 words by frequency
    let mut top_words: Vec<(String, usize)> = word_counts.into_iter().collect();
    top_words.sort_by(|a, b| b.1.cmp(&a.1));
    let top_10 = top_words.into_iter().take(10).collect::<Vec<_>>();

    for (word, count) in top_10 {
        println!("{}: {}", word, count);
    }

    Ok(())
}