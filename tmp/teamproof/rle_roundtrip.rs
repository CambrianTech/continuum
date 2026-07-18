pub fn rle_encode(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return result;
    }

    let mut count = 1;
    for i in 1..chars.len() {
        if chars[i] == chars[i - 1] {
            count += 1;
        } else {
            result.push(chars[i - 1]);
            result.push_str(&count.to_string());
            count = 1;
        }
    }
    result.push(chars[chars.len() - 1]);
    result.push_str(&count.to_string());

    result
}

pub fn rle_decode(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return result;
    }

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        i += 1;
        let mut count_str = String::new();
        while i < chars.len() && chars[i].is_digit(10) {
            count_str.push(chars[i]);
            i += 1;
        }
        let count: usize = count_str.parse().unwrap_or(1);
        result.extend(std::iter::repeat(ch).take(count));
    }

    result
}

fn main() {
    let test_cases = vec![
        ("aaabbc", "a3b2c1"),
        ("abc", "a1b1c1"),
        ("a12b3", "a12b3"),
        ("", ""),
    ];

    for (input, expected) in test_cases {
        let encoded = rle_encode(input);
        assert_eq!(encoded, expected, "Encoding failed: {} != {}");
        let decoded = rle_decode(&encoded);
        assert_eq!(decoded, input, "Decoding failed: {} != {}");
    }

    println!("All tests passed!");
}