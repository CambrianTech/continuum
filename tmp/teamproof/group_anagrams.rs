use std::collections::HashMap;

pub fn group_anagrams(words: Vec<String>) -> Vec<Vec<String>> {
    let mut anagram_map = HashMap::new();

    for word in words {
        let mut chars: Vec<char> = word.chars().collect();
        chars.sort_unstable();
        let sorted_word: String = chars.into_iter().collect();

        anagram_map.entry(sorted_word).or_insert_with(Vec::new).push(word);
    }

    anagram_map.into_values().collect()
}

fn main() {
    let words = vec![
        "listen".to_string(),
        "silent".to_string(),
        "enlist".to_string(),
        "google".to_string(),
        "gooegl".to_string(),
        "inlets".to_string(),
        "banana".to_string(),
    ];

    let grouped_anagrams = group_anagrams(words);
    for group in grouped_anagrams {
        println!("{:?}", group);
    }
}