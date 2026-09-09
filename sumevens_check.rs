mod sol { include!("sol_sum_evens.rs"); }
fn main() {
    use sol::sum_evens;
    let mut ok = true;
    macro_rules! chk { ($e:expr, $want:expr) => {{
        let got = sum_evens(&$e);
        if got != $want { ok = false; eprintln!("FAIL {:?} -> {} (want {})", $e, got, $want); }
    }}};
    chk!([], 0i32);
    chk!([1i32, 3, 5], 0);
    chk!([1i32, 2, 3, 4], 6);
    chk!([-4i32, -3, -2], -6);
    chk!([0i32, 1, 0], 0);
    chk!([1i32, i32::MAX - 1, i32::MIN], -1); // 2^31-2 + (-2^31) = -2; no overflow
    if ok { println!("all sum_evens checks passed"); } else { std::process::exit(1); }
}
