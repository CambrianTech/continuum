//! What a core reports about its engine on `ping`, and the deploy verdict on it (card a3e92354).
//!
//! Two facts, never one: the INSTALLED engine (the stamp of the slot `current` names, the binary
//! the next launch takes) and the SERVED engine (the live lane's own `/props` build, read by the
//! reconcile for the incarnation that answered). Holding the node to its pin promotes the pointer
//! first and relaunches the lane after, and either half can fail or still be in flight, so a pinned
//! pointer proves nothing about what serves (Codex on #5535). A deploy is verified only when the
//! live engine this core owns reports the pin and was launched from the installed build.
//!
//! An operator's own engine (`LLAMA_SERVER_BIN`) is its own typed mode, never an absence: the core
//! does not hold it to the pin and the verdict says so by name.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::inference::engine_residency::EngineIncarnation;

use super::engine_pin::{pinned_engine, EngineStamp};

/// The engine half of `ping`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/EngineReport.ts"
)]
pub enum EngineReport {
    /// This core owns its engine and holds it to `pin`: `installed` is the current slot's stamp
    /// (`None` when no owned slot is installed), `served` the live engine's own account (`None`
    /// until a live incarnation of it has answered `/props`).
    Owned {
        pin: String,
        installed: Option<String>,
        served: Option<ServedEngineReport>,
    },
    /// `LLAMA_SERVER_BIN` names an operator's own engine: not ours to converge or pin.
    Operator { pin: String, bin: String },
}

/// The engine process serving now, by its own account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ServedEngineReport.ts"
)]
pub struct ServedEngineReport {
    /// The live engine's pid.
    pub pid: u32,
    /// Its OS start time, seconds since the Unix epoch: with the pid and port, the incarnation.
    #[ts(type = "number")]
    pub started_s: u64,
    /// The port it serves.
    pub port: u16,
    /// What its own `/props` `build_info` named (`b10765-741b0175b`).
    pub build: String,
    /// The stamp of the engine slot its binary was launched from (the lane record's
    /// `engine_bin`); `None` for a binary outside the slots.
    pub slot_stamp: Option<String>,
}

/// PURE: the served report, only when the reconcile's reading belongs to the incarnation that is
/// live now (OS-confirmed). A reading from an engine since replaced, or a live engine whose
/// `/props` has not answered yet, is no evidence.
pub(crate) fn served_report(
    reading: Option<(EngineIncarnation, String)>,
    live: Option<EngineIncarnation>,
    slot_stamp: Option<String>,
) -> Option<ServedEngineReport> {
    let (seen, build) = reading?;
    (Some(seen) == live).then_some(ServedEngineReport {
        pid: seen.pid,
        started_s: seen.started_s,
        port: seen.port,
        build,
        slot_stamp,
    })
}

/// The live lane's served report: its record, the OS's account of that process, and the engine's
/// OWN `/props` reading must all name one incarnation. A launch's slot stamp, which the same cell
/// holds until `/props` answers (for `tail_tools`), never counts: the reading is typed `Observed`.
fn served_now() -> Option<ServedEngineReport> {
    let lane = crate::inference::lane_registry::live_lane()?;
    let live = EngineIncarnation::of(lane.pid, lane.port).filter(|now| *now == lane.incarnation());
    let slot_stamp = lane
        .engine_bin
        .as_deref()
        .and_then(crate::inference::engine_slots::stamp_of_bin);
    served_report(
        super::engine_features::served_engine_reading(),
        live,
        slot_stamp,
    )
}

/// This core's engine report, read now.
pub fn engine_report() -> EngineReport {
    let pin = pinned_engine().to_string();
    match super::engine_install::engine_override() {
        Some(bin) => EngineReport::Operator { pin, bin },
        None => EngineReport::Owned {
            pin,
            installed: super::engine_install::installed_engine_stamp(),
            served: served_now(),
        },
    }
}

/// PURE: the deploy verdict on an engine report. `Ok` names what was proven; every gap refuses.
pub fn engine_verdict(report: &EngineReport) -> Result<String, String> {
    let (pin, installed, served) = match report {
        EngineReport::Operator { pin, bin } => {
            return Ok(format!(
                "engine: the operator's own engine {bin} (LLAMA_SERVER_BIN); this core pins {pin} and does not hold an operator's engine to it"
            ))
        }
        EngineReport::Owned { pin, installed, served } => (pin, installed, served),
    };
    let installed = installed.as_deref().ok_or_else(|| {
        format!("ENGINE NOT VERIFIED: this core owns its engine but no engine slot is current (no stamp); `continuum install` builds pin {pin}")
    })?;
    if !EngineStamp::parse(installed).is_some_and(|stamp| stamp.holds(pin)) {
        return Err(format!(
            "ENGINE NOT PINNED: the current slot holds {installed} but this core pins {pin}. No slot holds the pin for this \
             host's target (`continuum install` builds it), or a promote was refused (read `serving.engine.repin_refused`)"
        ));
    }
    let served = served.as_ref().ok_or_else(|| {
        format!(
            "ENGINE NOT ADOPTED: the current slot holds pin {installed}, but no live engine has reported its build: a relaunch \
             onto it is in progress or failed, or nothing serves (read `serving.engine.converged` / `converge_failed`)"
        )
    })?;
    let lane = format!("live engine pid {} (port {})", served.pid, served.port);
    if !continuum_cli_lifecycle::deploy_provenance::sha_matches(
        super::engine_install::build_commit(&served.build),
        pin,
    ) {
        return Err(format!(
            "ENGINE NOT ADOPTED: the current slot holds pin {installed}, but the {lane} reports build {}: the lane has not \
             relaunched onto the pinned engine (read `serving.engine.converged` / `converge_failed`)",
            served.build
        ));
    }
    match served.slot_stamp.as_deref() {
        Some(launched) if launched == installed => {}
        Some(launched) => {
            return Err(format!(
                "ENGINE NOT ADOPTED: the {lane} reports the pin but was launched from slot stamp {launched}, not the installed {installed}"
            ))
        }
        None => {
            return Err(format!(
                "ENGINE NOT ADOPTED: the {lane} reports the pin but runs a binary outside the engine slots, not the installed {installed}"
            ))
        }
    }
    Ok(format!("✅ engine verified: the {lane} reports build {} from slot stamp {installed}, this core's pin", served.build))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN: &str = "741b0175b9a728530b7dcccaa2b6a039c47e5d9e";

    fn owned(installed: Option<&str>, served: Option<ServedEngineReport>) -> EngineReport {
        EngineReport::Owned {
            pin: PIN.into(),
            installed: installed.map(str::to_string),
            served,
        }
    }

    fn live(build: &str, slot_stamp: Option<&str>) -> ServedEngineReport {
        ServedEngineReport {
            pid: 100,
            started_s: 1_700_000_000,
            port: 8080,
            build: build.into(),
            slot_stamp: slot_stamp.map(str::to_string),
        }
    }

    // what this catches (Codex on #5535): "engine verified" printed for a pinned POINTER while the
    // old engine still served (promote done, relaunch pending or failed); it passes only once the
    // live engine reports the pin and was launched from the installed slot.
    #[test]
    fn a_pinned_slot_with_a_mismatching_live_engine_fails_until_the_lane_is_adopted() {
        let installed = Some("741b0175b:metal");
        let stale = owned(
            installed,
            Some(live("b10765-e7e1077b3", Some("e7e1077b3:metal"))),
        );
        assert!(engine_verdict(&stale)
            .unwrap_err()
            .contains("ENGINE NOT ADOPTED"));
        let relaunching = owned(installed, None);
        assert!(engine_verdict(&relaunching)
            .unwrap_err()
            .contains("ENGINE NOT ADOPTED"));
        let other_slot = owned(
            installed,
            Some(live("b10765-741b0175b", Some("741b0175b:cpu"))),
        );
        assert!(engine_verdict(&other_slot)
            .unwrap_err()
            .contains("ENGINE NOT ADOPTED"));
        let outside_slots = owned(installed, Some(live("b10765-741b0175b", None)));
        assert!(engine_verdict(&outside_slots).is_err());
        let unreadable_build = owned(
            installed,
            Some(live("b10765-unknown", Some("741b0175b:metal"))),
        );
        assert!(engine_verdict(&unreadable_build).is_err());

        let adopted = owned(
            installed,
            Some(live("b10765-741b0175b", Some("741b0175b:metal"))),
        );
        assert!(engine_verdict(&adopted)
            .unwrap()
            .contains("engine verified"));
    }

    // what this catches (Codex on #5535): missing evidence read as success ("an older core", "no
    // owned install"); only the operator's typed mode passes without the pin.
    #[test]
    fn missing_engine_evidence_refuses_and_only_the_operator_mode_is_exempt() {
        assert!(engine_verdict(&owned(None, None))
            .unwrap_err()
            .contains("ENGINE NOT VERIFIED"));
        assert!(engine_verdict(&owned(Some("e7e1077b3:metal"), None))
            .unwrap_err()
            .contains("ENGINE NOT PINNED"));
        assert!(
            engine_verdict(&owned(Some("741b0175b"), None))
                .unwrap_err()
                .contains("ENGINE NOT PINNED"),
            "a stamp with no target"
        );
        let operator = EngineReport::Operator {
            pin: PIN.into(),
            bin: "/opt/llama/llama-server".into(),
        };
        assert!(engine_verdict(&operator)
            .unwrap()
            .contains("operator's own engine /opt/llama/llama-server"));
    }

    // what this catches: a /props reading from an engine since replaced at the same port (or a
    // live engine whose /props has not answered) reported as the served engine.
    #[test]
    fn a_reading_counts_only_for_the_incarnation_live_now() {
        let a = EngineIncarnation {
            pid: 100,
            started_s: 1_700_000_000,
            port: 8080,
        };
        let b = EngineIncarnation {
            pid: 200,
            started_s: 1_700_000_500,
            port: 8080,
        };
        let reading = Some((a, "b10765-741b0175b".to_string()));
        assert_eq!(
            served_report(reading.clone(), Some(a), None).map(|r| r.pid),
            Some(100)
        );
        assert_eq!(served_report(reading.clone(), Some(b), None), None);
        assert_eq!(served_report(reading, None, None), None);
        assert_eq!(served_report(None, Some(a), None), None);
    }

    // what this catches (Codex on #5535 x #5538): the launch publishes the slot-derived build into
    // the served-evidence cell before `/props` and keeps it across a transient read failure. Read
    // as the engine's own answer, deploy-verify passed for a live engine that never reported.
    #[test]
    fn a_launch_stamp_is_not_adoption_until_the_live_engine_answers() {
        use super::super::engine_features::ServedEngine;
        let a = EngineIncarnation {
            pid: 100,
            started_s: 1_700_000_000,
            port: 8080,
        };
        let installed = "741b0175b:metal";
        let verdict = |evidence: &ServedEngine| {
            engine_verdict(&EngineReport::Owned {
                pin: PIN.into(),
                installed: Some(installed.into()),
                served: served_report(evidence.observed_reading(), Some(a), Some(installed.into())),
            })
        };

        let mut evidence = ServedEngine::Launched {
            incarnation: a,
            slot_build: Some("741b0175b".into()),
        };
        assert!(
            evidence.takes_tail_tools(PIN),
            "the capability consumer takes the launch stamp"
        );
        assert!(
            verdict(&evidence)
                .unwrap_err()
                .contains("ENGINE NOT ADOPTED"),
            "the receipt does not"
        );
        evidence.observe(Some(a), Err(()));
        assert!(
            verdict(&evidence)
                .unwrap_err()
                .contains("ENGINE NOT ADOPTED"),
            "nor after a failed /props read"
        );

        evidence.observe(Some(a), Ok(Some("b10765-741b0175b")));
        assert!(
            verdict(&evidence).unwrap().contains("engine verified"),
            "the live engine's own matching answer adopts"
        );
    }
}
