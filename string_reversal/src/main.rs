fn main() {
    let input = "hello";
    let reversed: String = input.chars().rev().collect();
    println!("Original: {}", input);
    println!("Reversed: {}", reversed);
}