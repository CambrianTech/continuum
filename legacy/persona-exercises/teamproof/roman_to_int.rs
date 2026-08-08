fn roman_to_int(s: &str) -> i32 {
    let mut total = 0;
    let mut prev_value = 0;

    let roman_map: std::collections::HashMap<char, i32> = [
        ('I', 1),
        ('V', 5),
        ('X', 10),
        ('L', 50),
        ('C', 100),
        ('D', 500),
        ('M', 1000),
    ]
    .iter()
    .cloned()
    .collect();

    for c in s.chars().rev() {
        let &value = roman_map.get(&c).unwrap();
        if value < prev_value {
            total -= value;
        } else {
            total += value;
        }
        prev_value = value;
    }

    total
}

fn main() {
    // Test cases
    println!("{}", roman_to_int("III"));     // Output: 3
    println!("{}", roman_to_int("IV"));      // Output: 4
    println!("{}", roman_to_int("IX"));      // Output: 9
    println!("{}", roman_to_int("LVIII"));   // Output: 58
    println!("{}", roman_to_int("MCMXCIV")); // Output: 1994
}