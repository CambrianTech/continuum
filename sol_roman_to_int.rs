//! roman_to_int — convert a Roman numeral string to its integer value.
//!
//! Subtractive pairs are handled by the classic scan: if a symbol is worth
//! less than the one immediately after it, it is subtracted (IV = 4, IX = 9,
//! XL = 40, XC = 90, CD = 400, CM = 900); otherwise it is added.

/// Convert a Roman numeral string to its integer value.
///
/// Valid input uses the symbols I (1), V (5), X (10), L (50), C (100),
/// D (500) and M (1000). A symbol worth less than the following one is
/// subtracted; all others are added. Characters outside that set are
/// ignored, so garbage input degrades gracefully instead of panicking.
pub fn roman_to_int(s: &str) -> i32 {
    let value = |c: u8| -> i32 {
        match c {
            b'I' => 1,
            b'V' => 5,
            b'X' => 10,
            b'L' => 50,
            b'C' => 100,
            b'D' => 500,
            b'M' => 1_000,
            _ => 0,
        }
    };

    let bytes = s.as_bytes();
    let mut total: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let v = value(b);
        if v == 0 {
            continue; // not a Roman numeral symbol — ignore
        }
        match bytes.get(i + 1).map(|&n| value(n)) {
            Some(nv) if nv > v => total -= v, // subtractive pair: IV, IX, XL, XC, CD, CM
            _ => total += v,
        }
    }
    total
}

fn main() {
    let cases: [(&str, i32); 16] = [
        ("MCMXCIV", 1_994), // classic example: M + CM + XC + IV
        ("MMXXIV", 2_024),  // the year, in Roman numerals
        ("LVIII", 58),      // plain additive
        ("IX", 9),          // subtractive pair
        ("XL", 40),         // subtractive pair
        ("XC", 90),         // subtractive pair
        ("CD", 400),        // subtractive pair
        ("CM", 900),        // subtractive pair
        ("IV", 4),          // subtractive pair
        ("DCLXVI", 666),    // additive mix
        ("MMDCCXXI", 2_721),
        ("III", 3),         // repeated ones
        ("MMCDXLIX", 2_449),
        ("CXC", 190),       // XC embedded in C _ XC
        ("MCMLVII", 1_957), // canonical 1957: M + CM + L + VII
        ("CMXCVII", 997),   // canonical 997: CM + XC + VII (not 1957 — C before M is additive here)
        ("" , 0),           // empty input: nothing to add
    ];

    let mut failed = 0;
    for (input, expected) in cases {
        let got = roman_to_int(input);
        let ok = got == expected;
        if !ok {
            failed += 1;
        }
        println!("{:>9} -> {:>5}  (expected {:>5})  {}", input, got, expected, if ok { "OK" } else { "FAIL" });
    }

    // Property: every subtractive pair must be worth exactly the difference.
    let pairs = [("IV", 4), ("IX", 9), ("XL", 40), ("XC", 90), ("CD", 400), ("CM", 900)];
    for (input, expected) in pairs {
        assert_eq!(roman_to_int(input), expected, "pair {input}");
    }

    // Property: MCMXCIV must decompose as 1000 + 900 + 90 + 4.
    assert_eq!(roman_to_int("MCMXCIV"), roman_to_int("M") + roman_to_int("CM") + roman_to_int("XC") + roman_to_int("IV"));

    if failed == 0 {
        println!("\nall {} cases passed", cases.len());
    } else {
        println!("\n{} case(s) FAILED", failed);
        std::process::exit(1);
    }
}
