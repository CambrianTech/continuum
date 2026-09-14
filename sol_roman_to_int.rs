//! roman_to_int — convert a Roman numeral string to its integer value.
//!
//! Rules implemented (canonical numerals, 1..=3999):
//!   - Each symbol has a fixed value: I=1 V=5 X=10 L=50 C=100 D=500 M=1000
//!   - A smaller symbol immediately BEFORE a larger one is subtracted (IV, IX, XL, XC, CD, CM)
//!   - All other symbols are added left to right.
//! Empty or whitespace-only input yields 0; unknown characters are skipped.

pub fn roman_to_int(s: &str) -> i32 {
    let value = |c: char| -> Option<i32> {
        match c {
            'I' => Some(1),
            'V' => Some(5),
            'X' => Some(10),
            'L' => Some(50),
            'C' => Some(100),
            'D' => Some(500),
            'M' => Some(1000),
            _ => None,
        }
    };

    let mut total: i32 = 0;
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let Some(v) = value(c) else { continue };
        // Subtractive pair: a smaller symbol directly before a larger one.
        if i + 1 < chars.len() {
            if let Some(next_v) = value(chars[i + 1]) {
                if v < next_v {
                    total -= v;
                    continue;
                }
            }
        }
        total += v;
    }
    total
}

/// Canonical encoder (test harness only): 1..=3999 -> Roman numeral string.
fn to_roman(n: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];

    let mut r = String::new();
    let mut n = n;
    for &(v, sym) in TABLE {
        while n >= v {
            r.push_str(sym);
            n -= v;
        }
    }
    debug_assert_eq!(n, 0);
    r
}

#[cfg(test)]
mod tests {
    use super::{roman_to_int, to_roman};

    #[test]
    fn subtractive_pairs() {
        assert_eq!(roman_to_int("IV"), 4);
        assert_eq!(roman_to_int("IX"), 9);
        assert_eq!(roman_to_int("XL"), 40);
        assert_eq!(roman_to_int("XC"), 90);
        assert_eq!(roman_to_int("CD"), 400);
        assert_eq!(roman_to_int("CM"), 900);
    }

    #[test]
    fn mixed() {
        assert_eq!(roman_to_int("MCMXCIV"), 1994);
        assert_eq!(roman_to_int("MMXXIV"), 2024);
        assert_eq!(roman_to_int("LVIII"), 58);
        assert_eq!(roman_to_int("DCLXVI"), 666);
    }

    #[test]
    fn empty_and_unknown() {
        assert_eq!(roman_to_int(""), 0);
        assert_eq!(roman_to_int("   "), 0);
    }

    #[test]
    fn exhaustive_round_trip() {
        // Every canonical numeral in 1..=3999 must decode back to itself.
        for n in 1u32..=3999 {
            let r = to_roman(n);
            assert_eq!(roman_to_int(&r), n as i32, "round-trip failed at {n} -> {r}");
        }
    }

    #[test]
    fn encoder_sanity() {
        assert_eq!(to_roman(1), "I");
        assert_eq!(to_roman(9), "IX");
        assert_eq!(to_roman(40), "XL");
        assert_eq!(to_roman(3888), "MMMDCCCLXXXVIII");
        assert_eq!(to_roman(3999), "MMMCMXCIX");
    }
}

fn main() {
    let cases = [
        ("MCMXCIV", 1994), ("MMXXIV", 2024), ("LVIII", 58), ("IX", 9),
        ("XL", 40), ("XC", 90), ("CD", 400), ("CM", 900), ("IV", 4),
        ("DCLXVI", 666), ("MMDCCXXI", 2721), ("III", 3), ("MMCDXLIX", 2449),
        ("CXC", 190), ("MCMLVII", 1957), ("CMXCVII", 997), ("", 0),
    ];

    let mut failed = 0;
    for (input, expected) in cases {
        let got = roman_to_int(input);
        let ok = got == expected;
        if !ok {
            failed += 1;
        }
        println!("{:>9} -> {:>4}  (expected {:>4})  {}", input, got, expected, if ok { "OK" } else { "FAIL" });
    }

    // Property: every subtractive pair must be worth exactly the difference.
    let pairs = [("IV", 4), ("IX", 9), ("XL", 40), ("XC", 90), ("CD", 400), ("CM", 900)];
    for (input, expected) in pairs {
        assert_eq!(roman_to_int(input), expected, "pair {input}");
    }

    // Property: MCMXCIV must decompose as 1000 + 900 + 90 + 4.
    assert_eq!(roman_to_int("MCMXCIV"), roman_to_int("M") + roman_to_int("CM") + roman_to_int("XC") + roman_to_int("IV"));

    // Property: exhaustive round-trip over the entire representable range —
    // every canonical numeral must decode back to its own value.
    let mut round_trips = 0;
    for n in 1u32..=3999 {
        let r = to_roman(n);
        assert_eq!(roman_to_int(&r), n as i32, "round-trip failed at {n} -> {r}");
        round_trips += 1;
    }

    if failed == 0 {
        println!("\nall {} cases + {} round-trips passed", cases.len(), round_trips);
    } else {
        println!("\n{} case(s) FAILED", failed);
        std::process::exit(1);
    }
}
