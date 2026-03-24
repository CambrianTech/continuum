//! Grid transport implementations.
//!
//! Two outliers that validate the GridTransport trait:
//! - Tailscale: TCP over managed WireGuard mesh (IP-based, working NOW)
//! - Reticulum: Encrypted mesh with cryptographic identity (infrastructure-free, future)

pub mod tailscale;
pub mod reticulum;
