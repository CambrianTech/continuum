//! Pure disk admission policy, also projected by manifest-gen for first-install
//! transports that cannot yet invoke a trusted installed CLI.

pub const DISK_CAPACITY_REFUSAL_BASIS_POINTS: u64 = 9_500;

pub fn projected_fits(used: u64, capacity: u64, projected: u64, basis_points: u64) -> bool {
    capacity != 0
        && basis_points != 0
        && basis_points <= 10_000
        && used.saturating_add(projected) <= capacity.saturating_mul(basis_points) / 10_000
}
