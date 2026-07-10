fn reverse_string(s: &str) -> String {
    let mut chars = s.chars();
    let mut reversed = String::new();
    while let Some(c) = chars.next() {
        reversed.push(c);
    }
    reversed
}

fn main() {
    let original_string = "Hello, World!";
    let reversed_string = reverse_string(original_string);
    println!("Original String: {}", original_string);
    println!("Reversed String: {}", reversed_string);
}