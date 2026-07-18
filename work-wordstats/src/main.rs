use std::collections::HashMap;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file>", args[0]);
        return;
    }

    let filename = &args[1];
    let contents = fs::read_to_string(filename)
        .expect("Something went wrong reading the file");

    let mut words = HashMap::new();
    for word in contents.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        *words.entry(word).or_insert(0) += 1;
    }

    let mut vec = Vec::from_iter(words.into_iter());
    vec.sort_by(|a, b| b.1.cmp(&a.1));

    for (word, count) in vec.into_iter().take(10) {
        println!("{} {}", word, count);
    }
}
