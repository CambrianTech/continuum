fn main() {
    let test_cases = vec![
        ("123", 123),
        ("-456", -456),
        ("   +789", 789),
        ("000", 0),
        ("abc", 0),
        ("123abc", 123),
        ("", 0),
        ("   ", 0),
        ("+9223372036854775807", i32::MAX),
        ("-9223372036854775808", i32::MIN),
    ];

    for (input, expected) in test_cases {
        let result = my_atoi(input);
        assert_eq!(result, expected, "Test failed for input: {}", input);
    }

    println!("All tests passed!");
}

pub fn my_atoi(s: &str) -> i32 {
    let mut chars = s.chars();
    let mut sign = 1;
    let mut result = 0;

    // Skip leading whitespace
    while let Some(c) = chars.next() {
        if c != ' ' {
            break;
        }
    }

    // Optional +/- sign
    if let Some(c) = chars.next() {
        if c == '+' {
            sign = 1;
        } else if c == '-' {
            sign = -1;
        } else if c.is_digit(10) {
            result = c.to_digit(10).unwrap();
        } else {
            return 0;
        }
    }

    // Read digits until a non-digit
    while let Some(c) = chars.next() {
        if c.is_digit(10) {
            let digit = c.to_digit(10).unwrap();
            result = result.saturating_mul(10);
            result = result.saturating_add(digit);
        } else {
            break;
        }
    }

    // Apply sign and clamp to i32::MIN/i32::MAX
    if sign == -1 {
        result = result.saturating_neg();
    }

    result as i32
}