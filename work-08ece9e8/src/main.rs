use clap::{App, Arg};

fn main() {
    let matches = App::new("wordstats")
        .version("1.0")
        .author("Asha <asha@example.com>")
        .about("Calculates word frequencies in a text file")
        .arg(
            Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("FILE")
                .help("Sets the input file to use")
                .takes_value(true),
        )
        .get_matches();
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};

fn main() {
    let matches = App::new("wordstats")
        .version("1.0")
        .author("Asha <asha@example.com>")
        .about("Calculates word frequencies in a text file")
        .arg(
            Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("FILE")
                .help("Sets the input file to use")
                .takes_value(true),
        )
        .get_matches();

    let input_file = matches.value_of("input").unwrap_or("sample.txt");

    // Read the file and count word frequencies
    let word_counts = match read_word_frequencies(input_file) {
        Ok(counts) => counts,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return;
        }
    };

    // Print the top 10 most frequent words
    let mut sorted_counts = word_counts.into_iter().collect::<Vec<_>>();
    sorted_counts.sort_by(|a, b| b.1.cmp(&a.1));
    println!("Top 10 most frequent words:");
    for (word, count) in sorted_counts.into_iter().take(10) {
        println!("{}: {}", word, count);
    }
}

fn read_word_frequencies(filename: &str) -> io::Result<HashMap<String, usize>> {
    let file = File::open(filename)?;
    let reader = io::BufReader::new(file);

    let mut word_counts = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        for word in line.split_whitespace() {
            *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
        }
    }

    Ok(word_counts)
}