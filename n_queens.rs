/// Returns the number of distinct ways to place n non-attacking queens on an n×n board.
pub fn n_queens(n: usize) -> usize {
    if n == 0 { return 1; }
    let mask = (1u64 << n) - 1;
    
    fn backtrack(row: usize, cols: u64, diag1: u64, diag2: u64, n: usize, mask: u64) -> usize {
        if row == n {
            return 1;
        }
        let available = mask & !(cols | diag1 | diag2);
        let mut count = 0;
        let mut bits = available;
        while bits != 0 {
            let bit = bits & bits.wrapping_neg(); // lowest set bit
            bits &= bits.wrapping_sub(1); // remove lowest set bit
            count += backtrack(row + 1, cols | bit, (diag1 | bit) << 1, (diag2 | bit) >> 1, n, mask);
        }
        count
    }
    
    backtrack(0, 0, 0, 0, n, mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_known_values() {
        assert_eq!(n_queens(1), 1);
        assert_eq!(n_queens(2), 0);
        assert_eq!(n_queens(3), 0);
        assert_eq!(n_queens(4), 2);
        assert_eq!(n_queens(5), 10);
        assert_eq!(n_queens(6), 4);
        assert_eq!(n_queens(7), 40);
        assert_eq!(n_queens(8), 92);
        assert_eq!(n_queens(9), 352);
        assert_eq!(n_queens(10), 724);
    }
}
