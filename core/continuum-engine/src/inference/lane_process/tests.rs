#![cfg(test)]
use super::*;
    // what this catches: the Windows port-owner lookup returning None for a port
    // that IS held. Regression for the 2026-09-05 wedge on BigMama — `lsof` does
    // not exist on Windows, so `pid_listening_on_port` returned None for every
    // port, the pre-spawn gate read that as "held by an unnameable stranger", and
    // serving refused to spawn FOREVER against the core's own orphaned child.
    #[test]
    fn netstat_names_the_listening_pid_for_the_right_port() {
        // Verbatim `netstat -ano -p TCP` output from the wedged box.
        let out = "  Proto  Local Address          Foreign Address        State           PID
  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
  TCP    127.0.0.1:58057        127.0.0.1:64707        TIME_WAIT       0
  TCP    0.0.0.0:445            0.0.0.0:0              LISTENING       4
";
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
        assert_eq!(parse_netstat_listening_pid(out, 445), Some(4));
        // Not listening anywhere = None, which callers treat as "refuse to kill".
        assert_eq!(parse_netstat_listening_pid(out, 9999), None);
        // TIME_WAIT is not a holder we may reap.
        assert!(!out.is_empty());
    }

    // what this catches: matching the port as a substring instead of the local
    // address's final component. `contains(":8057")` would match ":58057" and
    // hand a reap decision the WRONG pid — a blind kill of an unrelated process.
    #[test]
    fn netstat_port_match_is_not_a_substring_match() {
        let out = "  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
";
        assert_eq!(parse_netstat_listening_pid(out, 8057), None);
        assert_eq!(parse_netstat_listening_pid(out, 5805), None);
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
    }

    // what this catches: THE safety invariant shared by both callers — our own
    // test-runner process is NOT a llama-server, so is_llama_server must refuse it.
    // A regression here would let either module SIGKILL an unrelated reused-pid
    // process (the blind-kill this guard exists to prevent).
    #[test]
    fn is_llama_server_false_for_self() {
        assert!(
            !is_llama_server(std::process::id()),
            "the test runner is not a llama-server; reaping it would be a blind kill"
        );
    }

    // what this catches: the grant recovered for an ADOPTED engine (every deploy leaves the
    // lane up for the next core) is read off its own argv — both spellings, absent = None,
    // malformed = None, and 0 stays 0 (a real "no host cache" launch, not an unknown).
    #[test]
    fn cache_ram_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            cache_ram_mib_in(&argv("llama-server -m x.gguf -c 133930 --parallel 2 --cache-ram 14396 --port 8080")),
            Some(14_396)
        );
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram=8704")), Some(8_704));
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram 0")), Some(0), "0 is a value");
        assert_eq!(cache_ram_mib_in(&argv("llama-server -c 4096")), None, "absent = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram")), None, "malformed = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram lots")), None);
    }

    // what this catches: busy is not dead. The verdict needs the probe AND the process's
    // activity: a miss with CPU advancing is a slow answer under load (adopted), a miss
    // with a frozen process is a wedge (reaped), an unsampled process never exonerates.
    // Regression for the M5 2026-09-20 (card b57b19fd): a 32 GB lane mid-generation was
    // killed on one 3 s probe under swap, twice in one morning.
    #[test]
    fn busy_is_not_dead_the_verdict_needs_two_signals() {
        assert_eq!(verdict_from(Some(2), None, ""), LaneVerdict::Healthy { attempt: 2 });
        let busy = verdict_from(None, Some(140.0), "no answer within 5 s");
        assert!(busy.adopt(), "a working lane that answers late is adopted: {busy:?}");
        assert!(matches!(busy, LaneVerdict::Busy { cpu_pct, .. } if cpu_pct == 140.0));
        let frozen = verdict_from(None, Some(0.0), "no answer within 5 s");
        assert!(!frozen.adopt(), "a frozen lane that never answers is dead: {frozen:?}");
        let unsampled = verdict_from(None, None, "connection refused");
        assert!(!unsampled.adopt(), "an absence never exonerates");
        assert!(busy.evidence().contains("adopted") && frozen.evidence().starts_with("dead"));
    }

    // what this catches: the probe is retried within a bound sized for a loaded box — a
    // listener that answers 4 s late (the M5 under swap) is HEALTHY on the first attempt
    // with a 5 s bound, where the old single 3 s shot read it as dead; and a port nobody
    // listens on misses every attempt with the reason carried.
    #[test]
    fn a_slow_answer_within_the_bound_is_healthy() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = sock.read(&mut buf);
                std::thread::sleep(std::time::Duration::from_secs(4));
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        assert_eq!(probe_health_bounded(port, 1, 5), Ok(1), "4 s late is inside a 5 s bound");
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let miss = probe_health_bounded(dead_port, 2, 1);
        assert!(miss.is_err(), "nobody listening → every attempt misses: {miss:?}");
    }

    // what this catches: a definitely-dead pid is neither alive nor a llama-server,
    // so a stale registry/pidfile entry can never trigger a kill.
    #[test]
    fn dead_pid_is_not_alive_or_llama() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        // (Tiny PID-reuse window is acceptable in a unit test; the invariant we
        // assert is "we do not treat a reaped pid as a live llama-server".)
        assert!(
            !is_llama_server(dead),
            "a reaped pid must not read as a live llama-server"
        );
    }

    // what this catches (card 977842fd): the running engine's KV decision is read off its
    // own argv in every shape the launcher and an operator write it — the fact the adopt
    // rail compares the plan's decision against.
    #[test]
    fn the_kv_decision_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        let kv = launch_kv_in(&argv("llama-server -m x.gguf --cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on -c 53760"));
        assert_eq!(kv, LaunchKv { cache_type: Some("q8_0".into()), flash_attn: true, cache_ram_mib: None });
        let plain = launch_kv_in(&argv("llama-server -m x.gguf -c 26880 --parallel 1"));
        assert_eq!(plain, LaunchKv { cache_type: None, flash_attn: false, cache_ram_mib: None });
        assert_eq!(plain.cache_type_or_default(), "f16", "no flag is the engine's f16 default");
        assert_eq!(launch_kv_in(&argv("llama-server --cache-type-k=q4_0 --flash-attn=off")), LaunchKv { cache_type: Some("q4_0".into()), flash_attn: false, cache_ram_mib: None });
        assert_eq!(launch_kv_in(&argv("llama-server -c 51968 --cache-ram 3248")).cache_ram_mib, Some(3_248), "the grant an adopted lane was launched with");
        assert!(launch_kv_in(&argv("llama-server -fa --mmproj p.gguf")).flash_attn, "a bare flag is on and eats nothing");
        assert!(!launch_kv_in(&argv("llama-server --flash-attn off")).flash_attn);
    }
