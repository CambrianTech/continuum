//! Validate a participant's recovery before changing any retained receipt entry.
use crate::runtime::{CustodyState, HeldCustody, SettledCustody};

#[derive(Debug, thiserror::Error)]
#[error("invalid recovery evidence from {participant}: {reason}")]
pub struct CustodyRecoveryError {
    pub participant: String,
    pub reason: CustodyRecoveryRefusal,
}

/// The failed custody invariant, preserved for callers without parsing diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CustodyRecoveryRefusal {
    #[error("recovery must cover exactly the retained entries")]
    IncompleteCoverage,
    #[error("endpoint and original incarnation must identify one retained entry")]
    UnmatchedIncarnation,
    #[error("a retained entry was returned more than once")]
    DuplicateEntry,
    #[error("saved recovery requires a verified process incarnation")]
    UnverifiedProcess,
    #[error("retained custody must resolve to exactly one installed owner")]
    AmbiguousOwner,
    #[error("this participant does not retain process custody")]
    NonCustodian,
    #[error("only retained custody is recovered")]
    NotRetained,
    #[error("the retained endpoint is not one this process pages")]
    UnknownEndpoint,
}

pub(super) fn merge_recovery(
    receipt: &mut [SettledCustody],
    participant: &str,
    recovered: Vec<HeldCustody>,
) -> Result<(), CustodyRecoveryError> {
    let invalid = |reason| CustodyRecoveryError {
        participant: participant.to_owned(), reason,
    };
    let retained: Vec<usize> = receipt.iter().enumerate()
        .filter(|(_, entry)| entry.participant == participant && entry.state == CustodyState::Retained)
        .map(|(index, _)| index).collect();
    if retained.len() != recovered.len() {
        return Err(invalid(CustodyRecoveryRefusal::IncompleteCoverage));
    }
    let mut changes = Vec::with_capacity(retained.len());
    for update in recovered {
        let matches: Vec<usize> = retained.iter().copied().filter(|&index| {
            let original = &receipt[index];
            original.endpoint == update.endpoint &&
                (original.process.is_none() || original.process == update.process)
        }).collect();
        let [index] = matches.as_slice() else {
            return Err(invalid(CustodyRecoveryRefusal::UnmatchedIncarnation));
        };
        if changes.iter().any(|(used, _)| used == index) {
            return Err(invalid(CustodyRecoveryRefusal::DuplicateEntry));
        }
        if update.state == CustodyState::Saved && update.process.is_none() {
            return Err(invalid(CustodyRecoveryRefusal::UnverifiedProcess));
        }
        changes.push((*index, update));
    }
    // Validation completes before publication: invalid evidence changes nothing.
    for (index, update) in changes {
        receipt[index].process = update.process;
        receipt[index].state = update.state;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::SettledProcess;

    #[test]
    fn a_replacement_cannot_clear_retention_or_rewrite_saved_work() {
        let process = SettledProcess { pid: 42, started_s: 10 };
        let entry = |endpoint: &str, state| SettledCustody {
            participant: "pages".into(), endpoint: endpoint.into(), process: Some(process), state,
        };
        let mut receipt = vec![entry("saved", CustodyState::Saved), entry("held", CustodyState::Retained)];
        let before = receipt.clone();
        assert!(merge_recovery(&mut receipt, "pages", vec![HeldCustody {
            endpoint: "held".into(), process: Some(SettledProcess { pid: 42, started_s: 11 }), state: CustodyState::Saved,
        }]).is_err());
        assert_eq!(receipt, before);
        assert!(merge_recovery(&mut receipt, "pages", vec![HeldCustody {
            endpoint: "held".into(), process: Some(process), state: CustodyState::Lost,
        }]).is_ok());
        assert_eq!(receipt[0], before[0]);
        assert_eq!(receipt[1].state, CustodyState::Lost);
        assert!(merge_recovery(&mut receipt, "pages", vec![HeldCustody {
            endpoint: "held".into(), process: Some(process), state: CustodyState::Saved,
        }]).is_err());
    }
}
