//! The stop's custody evidence (CONTINUUM-ENGINE 4.9, I1): what each stop participant leaves in
//! custody per endpoint, bound to the exact process its outcome was sealed against, counted in the
//! receipt's verdict, and run on every shutdown path, the signal's included.
#![cfg(test)]

use super::*;
use crate::runtime::{CustodyState, HeldCustody, SettledCustody, SettledProcess};

/// A stop participant that speaks for one process, saved: the shape of the page checkpoint.
struct SavesOneProcess;

impl crate::runtime::extensions::SettleBeforeStop for SavesOneProcess {
    fn name(&self) -> &'static str {
        "saves_one_process"
    }
    fn settle(&self, _cap: std::time::Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<HeldCustody>> + Send + 'static>> {
        Box::pin(async {
            vec![HeldCustody {
                endpoint: "test://signal-path".into(),
                process: Some(SettledProcess { pid: 7, started_s: 11 }),
                state: CustodyState::Saved,
            }]
        })
    }
}

// what this catches (Codex on #6042): the signal path begins its shutdown with ZERO settle time,
// and zero skipped every participant, so a SIGTERM never ran the page checkpoint and lost every
// resident's newest KV. Zero gives the turns no time; it never skips a participant.
#[tokio::test]
async fn a_zero_settle_still_runs_every_stop_participant() {
    crate::runtime::extensions::install_settle_before_stop(Arc::new(SavesOneProcess));
    let op = ShutdownOperation::new();
    let runtime = Arc::new(Runtime::new());
    let mut rx = op.begin(&runtime, std::time::Duration::ZERO);
    let receipt = rx.wait_for(Option::is_some).await.expect("the owner publishes").clone().expect("published");
    assert_eq!(
        receipt.custody_of("test://signal-path", 7, 11),
        Some(CustodyState::Saved),
        "the participant ran and its custody reached the receipt: {:?}", receipt.custody
    );
    assert!(receipt.custody.iter().any(|c| c.participant == "saves_one_process"), "the kernel stamps the participant's own name");
}

// what this catches (CONTINUUM-ENGINE 4.9, I1; Codex's acceptance cases on #6041/#6042): a receipt
// read as permission to end a process it does not name, or as durable while state is held only by
// a live process. Custody is bound to the exact endpoint and incarnation; a reused pid, another
// start time or another endpoint finds nothing; one participant that retained the process outweighs
// every other that saved it; and a retention bound to no process still makes the stop not durable.
#[test]
fn custody_authorizes_only_the_exact_process_it_names() {
    let held = |participant: &str, process: Option<SettledProcess>, state| SettledCustody {
        participant: participant.into(),
        endpoint: "http://127.0.0.1:58057".into(),
        process,
        state,
    };
    let engine = Some(SettledProcess { pid: 49_553, started_s: 1_794_679_637 });
    let mut receipt = ShutdownReceipt { modules: Vec::new(), total_ms: 0, custody: vec![held("page_io", engine, CustodyState::Saved)] };
    assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_637), Some(CustodyState::Saved));
    assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_638), None, "a reused pid is another process");
    assert_eq!(receipt.custody_of("http://127.0.0.1:58058", 49_553, 1_794_679_637), None, "another endpoint");
    assert!(receipt.state_is_durable(), "every module durable and the KV saved");

    receipt.custody.push(held("other", engine, CustodyState::Retained));
    assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_637), Some(CustodyState::Retained), "one retention outweighs every save");
    assert!(!CustodyState::Retained.ends_safely());
    assert!(!receipt.state_is_durable(), "retained custody is unsaved state");
    assert!(receipt.summary().contains("still held only by the live process"), "{}", receipt.summary());
    receipt.custody.pop();
    assert!(receipt.state_is_durable(), "saved custody alone is durable");

    receipt.custody.push(held("page_io", None, CustodyState::Retained));
    assert!(!receipt.state_is_durable(), "a retention bound to no process is still unsaved state");
    assert!(receipt.summary().contains("process unidentified"), "{}", receipt.summary());
    assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_637), Some(CustodyState::Saved), "an unbound entry never speaks for a process");
}
