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

/// Canonical builder — the mirror image of `roman_to_int`, used below to
/// round-trip-test every value in the representable range.
fn to_roman(mut n: u32) -> String {
    let table: [(u32, &str); 13] = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut out = String::new();
    for (val, sym) in table {
        while n >= val {
            out.push_str(sym);
            n -= val;
        }
    }
    out
}

fn main() {
    // Hand-picked cases: plain values, every subtractive pair, and real dates.
    let cases = [
        ("MCMXCIV", 1994),   // the classic "year of the first computer" example
        ("MMXXIV", 2024),
        ("LVIII", 58),
        ("IX", 9),
        ("XL", 40),
        ("XC", 90),
        ("CD", 400),
        ("CM", 900),
        ("IV", 4),
        ("DCLXVI", 666),
        ("MMDCCXXI", 2721),
        ("III", 3),
        ("MMCDXLIX", 2449),
        ("CXC", 190),
        ("MCMLVII", 1957),
        ("CMXCVII", 997),
        ("", 0),             // empty input -> 0
    ];

    let mut failed = 0;
    for (input, expected) in cases {
        let got = roman_to_int(input);
        let ok = got == expected;
        println!("{:>10} -> {:>4}  (expected {:>4})  {}", input, got, expected, if ok { "OK" } else { "FAIL" });
        if !ok {
            failed += 1;
        }
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
