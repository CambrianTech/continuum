//! Boot phase 19: the periodic memory reporter and OOM guard.

use super::*;

pub(super) fn spawn_memory_guard(state: &Arc<ServerState>) {
    // (see detect_system_ram_mb below for how the guard's limit is derived)
    // Periodic memory leak reporter — logs RSS + top leakers every 10s
    // Also acts as OOM guard: exits gracefully before OOM kills us ungracefully.
    // Limit is 80% of system RAM (not a fixed 4GB) — scales from an 8GB MacBook
    // Air to a 192GB workstation without false kills. The old 4GB limit was killing
    // the process on 48GB machines where 5.6GB RSS is perfectly normal (whisper
    // 1.6GB + ORT embedding runtime 1.8GB one-time alloc + working set).
    let mem_rt = state.rt_handle.clone();
    mem_rt.spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        // Total RAM via sysinfo — already a direct dependency of this crate, described in
        // Cargo.toml as "Cross-platform CPU, memory, system resource monitoring", and previously
        // unused here.
        //
        // What it replaces: a `sysctl` shell-out for macOS, a /proc/meminfo parse for Linux, and
        // — for every OTHER target, meaning Windows — the literal `8192`. Not a detection failure:
        // there was no Windows arm at all. So on Windows this guard believed the machine had 8 GB
        // and armed a FATAL self-exit at 6553 MB RSS, on hosts doing MoE serving where the host-side
        // expert cache alone dwarfs that. The action here is `process::exit(1)`, so a wrong number
        // does not degrade anything gracefully, it terminates the core.
        //
        // And when detection fails there is now NO invented number. A guessed-low limit is strictly
        // worse than no limit, because the failure mode of guessing is killing a healthy process.
        let Some(system_ram_mb) = crate::ipc::diagnostics::detect_system_ram_mb() else {
            eprintln!(
                "[MEMGUARD] could not determine system RAM — guard DISABLED. Running with no OOM \
                 guard is safe; arming one against a guessed size is not (it self-exits)."
            );
            return;
        };
        // 80% of system RAM — aggressive enough to catch real leaks,
        // generous enough not to false-kill on big machines.
        let max_rss_mb: u64 = (system_ram_mb * 80) / 100;
        eprintln!(
            "[MEMGUARD] Memory guard: system={}MB, limit={}MB (80%)", system_ram_mb, max_rss_mb
        );
        loop {
            interval.tick().await;
            // ONE reading per tick, handed to both consumers (card 948c30c2, row 18).
            let rss = current_rss_mb();
            dump_memory_report(rss);
            if rss > max_rss_mb {
                eprintln!(
                    "[MEMLEAK] FATAL: RSS {}MB exceeds {}MB limit (80% of {}MB system RAM) — \
                     exiting gracefully to avoid OOM. Restart with: npm start. Fix tracked in #603.",
                    rss, max_rss_mb, system_ram_mb
                );
                // Give time for the message to flush
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                std::process::exit(1);
            }
        }
    });
}
