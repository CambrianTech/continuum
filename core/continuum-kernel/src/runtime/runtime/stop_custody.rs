//! The stop's custody evidence (CONTINUUM-ENGINE 4.9, I1): what each stop participant leaves in
//! custody per process, bound to exactly that process, counted in the receipt's verdict, and run on
//! every shutdown path, the signal's included.
#![cfg(test)]

use super::*;

    /// A stop participant that speaks for one process, saved: the shape of the page checkpoint.
    struct SavesOneProcess;

    impl crate::runtime::extensions::SettleBeforeStop for SavesOneProcess {
        fn name(&self) -> &'static str {
            "saves_one_process"
        }
        fn settle(&self, _cap: std::time::Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<crate::runtime::SettledCustody>> + Send + 'static>> {
            Box::pin(async {
                vec![crate::runtime::SettledCustody {
                    participant: "saves_one_process".into(),
                    endpoint: "test://signal-path".into(),
                    pid: 7,
                    started_s: 11,
                    state: crate::runtime::CustodyState::Saved { residents: 0 },
                }]
            })
        }
    }

    // what this catches (Codex on #6042): the signal path begins its shutdown with ZERO settle
    // time, and zero skipped every participant, so a SIGTERM never ran the page checkpoint and
    // lost every resident's newest KV. Zero gives the turns no time; it never skips a participant.
    #[tokio::test]
    async fn a_zero_settle_still_runs_every_stop_participant() {
        crate::runtime::extensions::install_settle_before_stop(Arc::new(SavesOneProcess));
        let op = ShutdownOperation::new();
        let runtime = Arc::new(Runtime::new());
        let mut rx = op.begin(&runtime, std::time::Duration::ZERO);
        let receipt = rx.wait_for(Option::is_some).await.expect("the owner publishes").clone().expect("published");
        assert_eq!(
            receipt.custody_of("test://signal-path", 7, 11),
            Some(crate::runtime::CustodyState::Saved { residents: 0 }),
            "the participant ran and its custody reached the receipt: {:?}", receipt.custody
        );
    }

    // what this catches (CONTINUUM-ENGINE 4.9, I1; Codex's acceptance cases): a receipt read
    // as permission to end a process it does not name. Custody is bound to the exact endpoint
    // and incarnation; a reused pid, another start time or another endpoint finds nothing, and
    // one participant that retained the process outweighs every other that saved it.
    #[test]
    fn custody_authorizes_only_the_exact_process_it_names() {
        use crate::runtime::{CustodyState, SettledCustody};
        let held = |participant: &str, state| SettledCustody {
            participant: participant.into(),
            endpoint: "http://127.0.0.1:58057".into(),
            pid: 49_553,
            started_s: 1_794_679_637,
            state,
        };
        let mut receipt = ShutdownReceipt { modules: Vec::new(), total_ms: 0, custody: vec![held("page_io", CustodyState::Saved { residents: 4 })] };
        assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_637), Some(CustodyState::Saved { residents: 4 }));
        assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_638), None, "a reused pid is another process");
        assert_eq!(receipt.custody_of("http://127.0.0.1:58058", 49_553, 1_794_679_637), None, "another endpoint");
        assert!(receipt.state_is_durable(), "every module durable and the KV saved");
        receipt.custody.push(held("other", CustodyState::Retained));
        assert_eq!(receipt.custody_of("http://127.0.0.1:58057", 49_553, 1_794_679_637), Some(CustodyState::Retained), "one retention outweighs every save");
        assert!(!CustodyState::Retained.ends_safely());
        // Every module durable, the engine's KV retained: the stop is NOT durable, and says so.
        assert!(!receipt.state_is_durable(), "retained custody is unsaved state");
        assert!(receipt.summary().contains("still held only by the live process"), "{}", receipt.summary());
        receipt.custody.pop();
        assert!(receipt.state_is_durable(), "saved custody alone is durable");
    }

