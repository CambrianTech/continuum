fn main() {
    let s = String::from("hello");
    let reversed: String = s.chars().rev().collect();
    println!("{}", reversed);
}