//! Grid transport implementations.
//!
//! Three transports:
//! - Tailscale: TCP over managed WireGuard mesh (reliable commands, working NOW)
//! - Reticulum: Encrypted mesh with cryptographic identity (infrastructure-free, future)
//! - UDP Events: Fire-and-forget event streaming (sensor data, video, heartbeats)

pub mod reticulum;
pub mod tailscale;
pub mod udp_events;
