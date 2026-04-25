//! Grid module helpers — shared utilities.

use super::node::{TransportAddress, TrustLevel};
use super::transport::GridTransport;
use std::sync::Arc;

/// Find a transport that can handle a given address type.
pub fn find_transport_for_address<'a>(
    transports: &'a [Arc<dyn GridTransport>],
    address: &TransportAddress,
) -> Option<&'a Arc<dyn GridTransport>> {
    let transport_name = address.transport_name();
    transports
        .iter()
        .find(|t| t.name() == transport_name && t.local_address().is_some())
}

/// Parse a trust level string.
pub fn parse_trust_level(s: &str) -> Result<TrustLevel, String> {
    match s {
        "blocked" => Ok(TrustLevel::Blocked),
        "provisional" => Ok(TrustLevel::Provisional),
        "trusted" => Ok(TrustLevel::Trusted),
        "owner" => Ok(TrustLevel::Owner),
        _ => Err(format!(
            "Invalid trust level: {s}. Use: blocked, provisional, trusted, owner"
        )),
    }
}

/// Generate a short correlation ID.
pub fn correlation_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    format!("{:012x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::grid::transports::tailscale::TailscaleTransport;

    #[test]
    fn test_parse_trust_levels() {
        assert_eq!(parse_trust_level("owner").unwrap(), TrustLevel::Owner);
        assert_eq!(parse_trust_level("trusted").unwrap(), TrustLevel::Trusted);
        assert_eq!(
            parse_trust_level("provisional").unwrap(),
            TrustLevel::Provisional
        );
        assert_eq!(parse_trust_level("blocked").unwrap(), TrustLevel::Blocked);
        assert!(parse_trust_level("invalid").is_err());
    }

    #[test]
    fn test_find_transport_for_address() {
        let transports: Vec<Arc<dyn GridTransport>> =
            vec![Arc::new(TailscaleTransport::with_default_port())];

        let ts_addr = TransportAddress::Tailscale {
            ip: "100.1.2.3".into(),
            port: 7117,
            machine_name: None,
        };

        // Transport not started → local_address() returns None → no match
        let result = find_transport_for_address(&transports, &ts_addr);
        assert!(result.is_none());

        // Reticulum address shouldn't match tailscale transport
        let ret_addr = TransportAddress::Reticulum {
            destination_hash: "abcd".into(),
        };
        let result = find_transport_for_address(&transports, &ret_addr);
        assert!(result.is_none());
    }
}
