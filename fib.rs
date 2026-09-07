pub fn fib(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        match a.checked_add(b) {
            Some(c) => {
                a = b;
                b = c;
            }
            None => return u64::MAX,
        }
    }
    a
}
