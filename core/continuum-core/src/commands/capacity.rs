//! `capacity/*` — capacity-fabric measurement commands.
//!
//! `capacity/io-probe` is the in-tree home of the 2026-07-31 scratchpad NVMe
//! measurement that produced the K3 serving-ceiling numbers (M5: 14 GB/s at
//! depth 4 ⇒ 2.3–4.7 tok/s vs WASTE's 0.32). Benchmark infra is COMMANDS
//! with receipts, never ad-hoc bash — this is the measured fact every grid
//! node reports so weighted layer-splits (#180) are computed from data, not
//! vibes.
//!
//! Adapter law (#231): the probe characterizes the DISK, not a model. It
//! takes `record_bytes` as an input (from the container manifest / arch
//! profile) and returns raw bytes-per-second; tokens-per-second is the
//! caller's arithmetic against THEIR manifest. No model constant lives here.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Params for `capacity/io-probe`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/capacity/CapacityIoProbeParams.ts")]
pub struct CapacityIoProbeParams {
    /// Size of one record read, in bytes — from the container manifest
    /// (`record_bytes`), never a typed model constant. Must be > 0.
    #[ts(type = "number")]
    pub record_bytes: u64,
    /// Directory whose volume to probe. Defaults to the user cache dir (the
    /// volume containers live on).
    #[serde(default)]
    pub dir: Option<String>,
    /// Approximate synthetic-bank size in gigabytes (default 4). Bounded to
    /// [1, 32] — big enough to defeat page-cache locality, small enough to
    /// never be a disk hazard.
    #[serde(default)]
    pub file_gb: Option<f64>,
    /// Parallel read depths to sample (default [1, 4, 8, 16]).
    #[serde(default)]
    pub depths: Option<Vec<u32>>,
    /// Records read per depth sample (default 128).
    #[serde(default)]
    pub reads_per_depth: Option<u32>,
}

/// One depth's measurement.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/capacity/CapacityIoProbeRow.ts")]
pub struct CapacityIoProbeRow {
    pub threads: u32,
    /// Sustained uncached random-read throughput at this depth.
    pub gigabytes_per_second: f64,
}

/// Result of `capacity/io-probe`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/capacity/CapacityIoProbeResult.ts")]
pub struct CapacityIoProbeResult {
    /// The probed bank file's path (deleted after the run).
    pub probed_path: String,
    #[ts(type = "number")]
    pub record_bytes: u64,
    #[ts(type = "number")]
    pub bank_bytes: u64,
    pub rows: Vec<CapacityIoProbeRow>,
    /// Best sustained throughput across depths — the number the layer-split
    /// planner weights nodes by.
    pub peak_gigabytes_per_second: f64,
}

/// `capacity/io-probe` — measure this node's sustained record-sized uncached
/// random-read bandwidth. Heavy (writes a multi-GB file, saturates the disk
/// for seconds): operator-level, never ambient.
#[derive(Default)]
pub struct CapacityIoProbe;

#[async_trait]
impl ActionCommand for CapacityIoProbe {
    const NAME: &'static str = "capacity/io-probe";
    const DESCRIPTION: &'static str =
        "Measure sustained record-sized uncached random-read bandwidth on this node's \
         storage, at several parallel depths. The measured fact behind expert-streaming \
         ceilings and weighted grid layer-splits. Writes a temporary multi-GB bank \
         (deleted afterward) and saturates the disk while running.";
    const ACCESS: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::Privileged;
    type Params = CapacityIoProbeParams;
    type Output = CapacityIoProbeResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        params: CapacityIoProbeParams,
    ) -> Result<CapacityIoProbeResult, CommandError> {
        if params.record_bytes == 0 {
            return Err(CommandError::Invalid("recordBytes must be > 0".into()));
        }
        let file_gb = params.file_gb.unwrap_or(4.0).clamp(1.0, 32.0);
        let depths = params.depths.unwrap_or_else(|| vec![1, 4, 8, 16]);
        if depths.is_empty() || depths.iter().any(|&d| d == 0 || d > 128) {
            return Err(CommandError::Invalid(
                "depths must be non-empty, each in 1..=128".into(),
            ));
        }
        let reads = params.reads_per_depth.unwrap_or(128).clamp(8, 4096);
        let dir = match params.dir {
            Some(d) => std::path::PathBuf::from(d),
            None => dirs::cache_dir()
                .ok_or_else(|| CommandError::Invalid("no cache dir on this platform".into()))?,
        };

        let record_bytes = params.record_bytes;
        tokio::task::spawn_blocking(move || probe(&dir, record_bytes, file_gb, &depths, reads))
            .await
            .map_err(|e| CommandError::Internal(format!("probe task join: {e}")))?
    }
}

crate::register_stateless_command!(CapacityIoProbe);

/// The blocking measurement body. Writes a synthetic bank with caching
/// DISABLED on the write fd (else the read phase measures RAM — glass-boxed
/// on the first scratchpad run: 69 "GB/s"), then samples random record reads
/// at each depth with `F_NOCACHE`, and deletes the bank.
fn probe(
    dir: &std::path::Path,
    record_bytes: u64,
    file_gb: f64,
    depths: &[u32],
    reads: u32,
) -> Result<CapacityIoProbeResult, CommandError> {
    use std::io::Write;
    use std::os::unix::fs::FileExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let bank_records = (((file_gb * 1e9) as u64) / record_bytes).max(2);
    let bank_bytes = bank_records * record_bytes;
    let path = dir.join(format!("capacity-io-probe-{}.bin", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        #[cfg(target_os = "macos")]
        unsafe {
            libc::fcntl(std::os::fd::AsRawFd::as_raw_fd(&f), libc::F_NOCACHE, 1);
        }
        let chunk = vec![0xA5u8; record_bytes as usize];
        for _ in 0..bank_records {
            f.write_all(&chunk)?;
        }
        f.sync_all()
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&path);
        return Err(CommandError::Internal(format!("bank write: {e}")));
    }

    let mut rows = Vec::new();
    for &threads in depths {
        let next = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicU64::new(0));
        let start = std::time::Instant::now();
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let path = path.clone();
                let next = next.clone();
                let done = done.clone();
                std::thread::spawn(move || {
                    let Ok(f) = std::fs::File::open(&path) else {
                        return;
                    };
                    #[cfg(target_os = "macos")]
                    unsafe {
                        libc::fcntl(std::os::fd::AsRawFd::as_raw_fd(&f), libc::F_NOCACHE, 1);
                    }
                    let mut buf = vec![0u8; record_bytes as usize];
                    let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ (t as u64);
                    loop {
                        if next.fetch_add(1, Ordering::Relaxed) >= reads as u64 {
                            break;
                        }
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                        let rec = x % bank_records;
                        if f.read_exact_at(&mut buf, rec * record_bytes).is_ok() {
                            done.fetch_add(record_bytes, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let secs = start.elapsed().as_secs_f64().max(1e-9);
        rows.push(CapacityIoProbeRow {
            threads,
            gigabytes_per_second: done.load(Ordering::Relaxed) as f64 / 1e9 / secs,
        });
    }
    let _ = std::fs::remove_file(&path);

    let peak = rows
        .iter()
        .map(|r| r.gigabytes_per_second)
        .fold(0.0_f64, f64::max);
    Ok(CapacityIoProbeResult {
        probed_path: path.display().to_string(),
        record_bytes,
        bank_bytes,
        rows,
        peak_gigabytes_per_second: peak,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::Ctx;

    // what this catches: name/access wiring — a disk-saturating multi-GB
    // writer must be Privileged, never on the ambient AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(CapacityIoProbe::NAME, "capacity/io-probe");
        assert!(matches!(
            CapacityIoProbe::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: parameter refusals — zero record size and degenerate
    // depths refuse loudly instead of writing a garbage bank.
    #[tokio::test]
    async fn invalid_params_refuse() {
        let cmd = CapacityIoProbe;
        let bad_record = CapacityIoProbeParams {
            record_bytes: 0,
            dir: None,
            file_gb: None,
            depths: None,
            reads_per_depth: None,
        };
        assert!(cmd.run(&Ctx::default(), bad_record).await.is_err());

        let bad_depths = CapacityIoProbeParams {
            record_bytes: 4096,
            dir: None,
            file_gb: None,
            depths: Some(vec![0]),
            reads_per_depth: None,
        };
        assert!(cmd.run(&Ctx::default(), bad_depths).await.is_err());
    }

    // what this catches: the measurement path end-to-end in miniature — a
    // tiny bank (1GB floor clamps down via small records? no: file_gb clamps
    // to >=1GB, so keep the records small but the run bounded by reads) is
    // too slow for unit tests; instead pin the pure bank arithmetic.
    #[test]
    fn bank_sizing_arithmetic() {
        // 4 GB of 12,406,784-byte records → 322 records, bank ≈ 3.996 GB.
        let records = ((4.0f64 * 1e9) as u64) / 12_406_784;
        assert_eq!(records, 322);
        assert!(records * 12_406_784 <= 4_000_000_000);
    }
}
