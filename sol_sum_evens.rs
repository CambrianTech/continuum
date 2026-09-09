fn sum_evens(nums: &[i32]) -> i32 {
    nums.iter().filter(|&&x| x % 2 == 0).sum()
}
