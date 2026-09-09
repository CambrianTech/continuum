include!("sol_sum_evens.rs");

fn main() {
    let mut ok = true;
    macro_rules! chk {
        ($e:expr, $want:expr) => {{
            let got: i32 = sum_evens(&$e);
            if got != $want {
                ok = false;
                eprintln!("FAIL {:?} -> {} (want {})", $e, got, $want);
            }
        }};
    }
    chk!([0i32], 0);                          // empty-ish: single zero is even
    chk!([1i32, 3, 5], 0);                    // no evens at all
    chk!([1i32, 2, 3, 4], 6);                 // mixed: 2 + 4
    chk!([-4i32, -3, -2], -6);                // negative evens keep their sign
    chk!([0i32, 1, 0], 0);                    // zero is even; contributes nothing
    chk!([1i32, i32::MAX - 1, i32::MIN], -2); // (2^31-2) + (-2^31) = -2, no overflow in debug
    let empty: [i32; 0] = [];
    chk!(&empty as &[i32], 0);                // empty slice sums to zero
    if ok { println!("all sum_evens checks passed"); } else { std::process::exit(1); }
}
