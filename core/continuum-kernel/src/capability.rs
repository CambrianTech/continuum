//! How capable a model is, as the one type every comparison reads: a score someone MEASURED, or a
//! proxy derived from the weights when nobody has. Any measured score outranks any proxy, by the
//! type's own order, so no ceiling on the proxy is needed to keep a big unmeasured model below a
//! measured frontier (the `UNMEASURED_RANK_CAP = 40` this replaces).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A model's capability. The derived order is the planner's: every `Proxy` ranks below every
/// `Measured`, and within a kind the higher score ranks higher. A role's minimum is the same
/// type, so a `Measured` minimum is never met by a `Proxy`, and a `Proxy` minimum is met by a
/// proxy at or above it or by any measured model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/serving/Capability.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Capability {
    /// Derived from the weights (GB on disk, plus a tool-use bonus): "bigger is smarter within a
    /// family", nothing across families.
    Proxy { score: u8 },
    /// A measured score (the catalog's benchmark index).
    Measured { score: u8 },
}

impl Capability {
    /// The least capability: every model holds it. A role that declares no minimum requires this.
    pub const ANY: Self = Self::Proxy { score: 0 };
    /// The greatest capability: nothing outranks it (the planner's promoted incumbent).
    pub const TOP: Self = Self::Measured { score: u8::MAX };

    /// A measured score when one exists, else the proxy.
    pub fn of(measured: Option<u8>, proxy: u8) -> Self {
        measured.map_or(Self::Proxy { score: proxy }, |score| Self::Measured { score })
    }

    /// The score within its kind, for a probe or a receipt; never compared across kinds.
    pub fn score(self) -> u8 {
        match self {
            Self::Proxy { score } | Self::Measured { score } => score,
        }
    }

    /// Whether the score was measured.
    pub fn is_measured(self) -> bool {
        matches!(self, Self::Measured { .. })
    }
}

impl Default for Capability {
    /// A role that declares no minimum requires the least capability: any model.
    fn default() -> Self {
        Self::ANY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a big unmeasured model outranking a measured one (the weights proxy of a
    // 250 GB model read 250 against a measured 42 until a cap of 40 was hand-tuned in), and a
    // measured minimum met by a proxy.
    #[test]
    fn any_measured_score_outranks_any_proxy() {
        assert!(Capability::Measured { score: 1 } > Capability::Proxy { score: u8::MAX });
        assert!(Capability::Proxy { score: 18 } >= Capability::Proxy { score: 12 }, "a 27B proxy holds a proxy minimum of 12");
        assert!(Capability::Proxy { score: 11 } < Capability::Proxy { score: 12 }, "a 14B proxy does not");
        assert!(Capability::Proxy { score: 200 } < Capability::Measured { score: 12 }, "a proxy never meets a measured minimum");
        assert_eq!(Capability::default(), Capability::ANY);
        assert!(Capability::ANY <= Capability::Proxy { score: 0 } && Capability::TOP >= Capability::Measured { score: 99 });
        assert_eq!(Capability::of(Some(42), 18), Capability::Measured { score: 42 });
        assert_eq!(Capability::of(None, 18), Capability::Proxy { score: 18 });
    }
}
