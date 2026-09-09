include!("sol_sum_evens.rs");

fn main() {
    let mut ok = true;
    macro_rules! chk { ($e:expr, $want:expr) => {{
        let got = sum_evens(&$e);
        if got != $want { ok = false; eprintln!("FAIL {:?} -> {} (want {})", $e, got, $want); }
    }}}
    chk!([], 0i32);                          // empty slice sums to zero
    chk!([1i32, 3, 5], 0);                  // no evens at all
    chk!([1i32, 2, 3, 4], 6);               // mixed: 2 + 4
    chk!([-4i32, -3, -2], -6);              // negative evens keep their sign (-4 % 2 == 0)
    chk!([0i32, 1, 0], 0);                  // zero is even; contributes nothing
    chk!([1i32, i32::MAX - 1, i32::MIN], -2); // (2^31-2) + (-2^31) = -2, no overflow in debug build
    if ok { println!("all sum_evens checks passed"); } else { std::process::exit(1); }
}
