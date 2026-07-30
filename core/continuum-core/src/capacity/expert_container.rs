//! Expert container reader — the #269 side of the #268 seam.
//!
//! BigMama's foundry (#268) re-packs a MoE's routed experts into a streaming
//! container (docs/reference/WASTE-EXTRACT.md, mined from sqliteai/waste):
//! a directory of `manifest.json` + one `experts-L{n}.bin` bank per layer,
//! each bank a run of fixed-size 4 KiB-aligned records sorted by expert id.
//! The bank file is THE grid shard unit — a peer that holds `experts-L07.bin`
//! can serve layer 7 to the mesh without holding anything else.
//!
//! This module is the READ side only: open a container, verify its geometry
//! loudly, and fetch one expert's whole record in one positioned read. The
//! write side (converter/re-packer) is the foundry's; the cache that decides
//! WHICH records stay resident is [`super::expert_ecache`]. Keeping the three
//! separate is what lets the same reader serve a local NVMe bank and a
//! remote peer's shard behind one fetch seam.
//!
//! Loudness contract: geometry violations (record not a page multiple, bank
//! size not a record multiple, header identity mismatch) are ERRORS, never
//! best-effort reads. WASTE's `bank_open` verifies alignment because O_DIRECT
//! turns a misaligned record into `EINVAL` on every read — we verify for the
//! same reason plus one more: a silently-wrong offset table is exactly the
//! shape of bug that produced the three-day reuse=0 mystery.

use std::fs::File;
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::expert_ecache::ExpertKey;

/// Record alignment the format guarantees (WASTE FORMAT.md). O_DIRECT needs
/// 512 or 4096; whole-4KiB records satisfy both block sizes by construction.
pub const RECORD_ALIGN: u64 = 4096;

/// Per-record header magic: ASCII `WEXP`, little-endian on disk.
pub const RECORD_MAGIC: u32 = u32::from_le_bytes(*b"WEXP");

/// Bytes of `ExpertRec` header we verify on fetch: magic(4) + layer(2) +
/// expert_id(2). The rest of the header (fmt, offsets, block count) belongs
/// to the decode stage, not the fetch seam.
const HEADER_IDENT_BYTES: usize = 8;

/// `manifest.json` at the container root — the seam contract with the
/// foundry (#268). The foundry WRITES this; nothing on the serving side ever
/// hand-authors one. Versioned so the format can grow without silent
/// misreads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContainerManifest {
    /// Manifest schema version. Readers refuse versions they don't know.
    pub version: u32,
    /// Model this container was packed from (e.g. "kimi-k3"). Identity for
    /// grid advertisement; never parsed for behavior.
    pub model: String,
    /// Record encoding (e.g. "VQ3R", "VQ2R"). Decode-stage concern; the
    /// fetch seam only carries it through.
    pub fmt: String,
    /// Fixed size of every expert record in every bank, in bytes. MUST be a
    /// multiple of [`RECORD_ALIGN`].
    pub record_bytes: u64,
    /// Number of MoE layers packed (one `experts-L{n}.bin` each, 0-indexed).
    pub n_layers: u16,
    /// Routed experts per layer (records per bank).
    pub experts_per_layer: u16,
    /// Experts activated per token (arch fact, e.g. 8 for K3's top-k). This
    /// is the input to the cliff arithmetic in
    /// [`super::expert_ecache::EcacheBudget::one_token_working_set`].
    pub activated_per_token: u32,
}

/// Everything that can go wrong opening or reading a container. Every
/// variant names the file and the numbers — a geometry error you can't act
/// on from the message alone is a bug in the error, not just the container.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("container manifest {path}: {source}")]
    ManifestIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("container manifest {path}: invalid JSON: {source}")]
    ManifestParse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("container manifest {path}: unknown version {found} (reader knows ≤ {known})")]
    ManifestVersion { path: PathBuf, found: u32, known: u32 },
    #[error(
        "container manifest {path}: record_bytes {record_bytes} is not a multiple of \
         {align} — misaligned records make every O_DIRECT read fail EINVAL"
    )]
    RecordMisaligned {
        path: PathBuf,
        record_bytes: u64,
        align: u64,
    },
    #[error("expert bank {path}: {source}")]
    BankIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "expert bank {path}: size {file_bytes} is not records×{record_bytes} for \
         {experts_per_layer} experts (expected {expected_bytes}) — truncated or \
         wrong-manifest bank"
    )]
    BankGeometry {
        path: PathBuf,
        file_bytes: u64,
        record_bytes: u64,
        experts_per_layer: u16,
        expected_bytes: u64,
    },
    #[error("layer {layer} out of range: container has {n_layers} layers")]
    LayerOutOfRange { layer: u16, n_layers: u16 },
    #[error("expert {expert} out of range: {experts_per_layer} experts per layer")]
    ExpertOutOfRange { expert: u16, experts_per_layer: u16 },
    #[error(
        "expert record identity mismatch in {path} at offset {offset}: header says \
         (magic {found_magic:#010x}, layer {found_layer}, expert {found_expert}), \
         fetch asked for (layer {want_layer}, expert {want_expert}) — corrupt bank \
         or drifted offset table"
    )]
    RecordIdentity {
        path: PathBuf,
        offset: u64,
        found_magic: u32,
        found_layer: u16,
        found_expert: u16,
        want_layer: u16,
        want_expert: u16,
    },
}

/// One opened `experts-L{n}.bin`: a verified run of fixed-size records
/// sorted by expert id. Offset arithmetic is the WHOLE index — contiguity +
/// sort order means `expert_id × record_bytes`, no side table to drift.
#[derive(Debug)]
pub struct ExpertBank {
    file: File,
    path: PathBuf,
    layer: u16,
    record_bytes: u64,
    experts: u16,
}

impl ExpertBank {
    /// Open + verify one bank (WASTE `bank_open`). Checks size geometry
    /// against the manifest; identity of individual records is verified per
    /// fetch (cheap: 8 bytes of an already-read buffer).
    pub fn open(
        path: &Path,
        layer: u16,
        record_bytes: u64,
        experts_per_layer: u16,
    ) -> Result<Self, ContainerError> {
        let file = File::open(path).map_err(|source| ContainerError::BankIo {
            path: path.to_path_buf(),
            source,
        })?;
        let file_bytes = file
            .metadata()
            .map_err(|source| ContainerError::BankIo {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        let expected_bytes = record_bytes * experts_per_layer as u64;
        if file_bytes != expected_bytes {
            return Err(ContainerError::BankGeometry {
                path: path.to_path_buf(),
                file_bytes,
                record_bytes,
                experts_per_layer,
                expected_bytes,
            });
        }
        Ok(Self {
            file,
            path: path.to_path_buf(),
            layer,
            record_bytes,
            experts: experts_per_layer,
        })
    }

    /// Fetch one expert's whole record: ONE positioned read of
    /// `record_bytes`, then an 8-byte identity check against the header.
    pub fn fetch(&self, expert: u16, buf: &mut [u8]) -> Result<(), ContainerError> {
        if expert >= self.experts {
            return Err(ContainerError::ExpertOutOfRange {
                expert,
                experts_per_layer: self.experts,
            });
        }
        debug_assert_eq!(buf.len() as u64, self.record_bytes);
        let offset = expert as u64 * self.record_bytes;
        self.file
            .read_exact_at(buf, offset)
            .map_err(|source| ContainerError::BankIo {
                path: self.path.clone(),
                source,
            })?;
        self.verify_identity(buf, offset, expert)
    }

    fn verify_identity(
        &self,
        buf: &[u8],
        offset: u64,
        expert: u16,
    ) -> Result<(), ContainerError> {
        let found_magic = u32::from_le_bytes(buf[0..4].try_into().expect("8-byte header"));
        let found_layer = u16::from_le_bytes(buf[4..6].try_into().expect("8-byte header"));
        let found_expert = u16::from_le_bytes(buf[6..8].try_into().expect("8-byte header"));
        if found_magic != RECORD_MAGIC || found_layer != self.layer || found_expert != expert {
            return Err(ContainerError::RecordIdentity {
                path: self.path.clone(),
                offset,
                found_magic,
                found_layer,
                found_expert,
                want_layer: self.layer,
                want_expert: expert,
            });
        }
        Ok(())
    }
}

/// An opened container directory: manifest + lazily-opened banks. Banks are
/// opened on first touch, not at container-open — a grid node holding two of
/// sixty shards must not fail (or stat sixty files) for the ones it serves
/// from peers instead.
#[derive(Debug)]
pub struct ExpertContainer {
    root: PathBuf,
    manifest: ContainerManifest,
    banks: Vec<Option<ExpertBank>>,
}

impl ExpertContainer {
    /// Highest manifest version this reader understands.
    pub const KNOWN_VERSION: u32 = 1;

    /// Open a container directory: parse + verify the manifest. No bank IO
    /// happens here.
    pub fn open(root: &Path) -> Result<Self, ContainerError> {
        let manifest_path = root.join("manifest.json");
        let mut raw = String::new();
        File::open(&manifest_path)
            .and_then(|mut f| f.read_to_string(&mut raw))
            .map_err(|source| ContainerError::ManifestIo {
                path: manifest_path.clone(),
                source,
            })?;
        let manifest: ContainerManifest =
            serde_json::from_str(&raw).map_err(|source| ContainerError::ManifestParse {
                path: manifest_path.clone(),
                source,
            })?;
        if manifest.version > Self::KNOWN_VERSION {
            return Err(ContainerError::ManifestVersion {
                path: manifest_path,
                found: manifest.version,
                known: Self::KNOWN_VERSION,
            });
        }
        if manifest.record_bytes == 0 || manifest.record_bytes % RECORD_ALIGN != 0 {
            return Err(ContainerError::RecordMisaligned {
                path: manifest_path,
                record_bytes: manifest.record_bytes,
                align: RECORD_ALIGN,
            });
        }
        let banks = (0..manifest.n_layers).map(|_| None).collect();
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
            banks,
        })
    }

    pub fn manifest(&self) -> &ContainerManifest {
        &self.manifest
    }

    /// Path of one layer's bank — the grid shard unit, exposed so placement
    /// can advertise/transfer shards without going through fetch.
    pub fn bank_path(&self, layer: u16) -> PathBuf {
        self.root.join(format!("experts-L{layer}.bin"))
    }

    /// Fetch one expert's record into `buf` (must be exactly
    /// `manifest.record_bytes` long). Opens the layer's bank on first touch.
    pub fn fetch(&mut self, key: ExpertKey, buf: &mut [u8]) -> Result<(), ContainerError> {
        if key.layer >= self.manifest.n_layers {
            return Err(ContainerError::LayerOutOfRange {
                layer: key.layer,
                n_layers: self.manifest.n_layers,
            });
        }
        let slot = &mut self.banks[key.layer as usize];
        if slot.is_none() {
            *slot = Some(ExpertBank::open(
                &self.root.join(format!("experts-L{}.bin", key.layer)),
                key.layer,
                self.manifest.record_bytes,
                self.manifest.experts_per_layer,
            )?);
        }
        slot.as_ref().expect("just opened").fetch(key.expert, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Foundry stand-in: write a tiny valid container (record = one page).
    fn write_container(root: &Path, n_layers: u16, experts: u16) -> ContainerManifest {
        let manifest = ContainerManifest {
            version: 1,
            model: "test-moe".into(),
            fmt: "VQ3R".into(),
            record_bytes: RECORD_ALIGN,
            n_layers,
            experts_per_layer: experts,
            activated_per_token: 2,
        };
        std::fs::write(
            root.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest");
        for layer in 0..n_layers {
            let mut f = File::create(root.join(format!("experts-L{layer}.bin"))).expect("bank");
            for expert in 0..experts {
                let mut rec = vec![0u8; RECORD_ALIGN as usize];
                rec[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
                rec[4..6].copy_from_slice(&layer.to_le_bytes());
                rec[6..8].copy_from_slice(&expert.to_le_bytes());
                // Distinguishable payload so round-trip is provable.
                rec[HEADER_IDENT_BYTES] = (layer as u8) ^ 0xA0;
                rec[HEADER_IDENT_BYTES + 1] = expert as u8;
                f.write_all(&rec).expect("record");
            }
        }
        manifest
    }

    #[test]
    fn fetch_round_trips_every_expert_in_one_read() {
        // what this catches: offset arithmetic drift (the reuse=0 bug shape) —
        // every (layer, expert) must come back with ITS OWN header + payload.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 3, 4);
        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; c.manifest().record_bytes as usize];
        for layer in 0..3u16 {
            for expert in 0..4u16 {
                c.fetch(ExpertKey { layer, expert }, &mut buf).expect("fetch");
                assert_eq!(buf[HEADER_IDENT_BYTES], (layer as u8) ^ 0xA0);
                assert_eq!(buf[HEADER_IDENT_BYTES + 1], expert as u8);
            }
        }
    }

    #[test]
    fn geometry_violations_fail_loud_at_open_not_at_read() {
        // what this catches: silent tolerance of a truncated bank or
        // misaligned record size — both must refuse before any fetch.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 4);

        // Truncated bank: chop one record off.
        let bank = dir.path().join("experts-L0.bin");
        let full = std::fs::read(&bank).expect("read bank");
        std::fs::write(&bank, &full[..full.len() - RECORD_ALIGN as usize]).expect("truncate");
        let mut c = ExpertContainer::open(dir.path()).expect("manifest still fine");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(ExpertKey { layer: 0, expert: 0 }, &mut buf)
            .expect_err("truncated bank must refuse");
        assert!(matches!(err, ContainerError::BankGeometry { .. }), "{err}");

        // Misaligned record_bytes: manifest itself must refuse.
        manifest.record_bytes = RECORD_ALIGN + 512;
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite manifest");
        let err = ExpertContainer::open(dir.path()).expect_err("misaligned must refuse");
        assert!(matches!(err, ContainerError::RecordMisaligned { .. }), "{err}");
    }

    #[test]
    fn record_identity_mismatch_is_an_error_not_a_wrong_answer() {
        // what this catches: a drifted offset table serving expert A's bytes
        // for expert B — the identity check must turn that into an ERROR.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 1, 2);
        // Corrupt record 1's header to claim it is expert 0.
        let bank = dir.path().join("experts-L0.bin");
        let mut bytes = std::fs::read(&bank).expect("read");
        let off = RECORD_ALIGN as usize + 6;
        bytes[off..off + 2].copy_from_slice(&0u16.to_le_bytes());
        std::fs::write(&bank, &bytes).expect("write");

        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(ExpertKey { layer: 0, expert: 1 }, &mut buf)
            .expect_err("identity mismatch must refuse");
        assert!(matches!(err, ContainerError::RecordIdentity { .. }), "{err}");
    }

    #[test]
    fn ecache_over_container_skips_disk_on_a_recurring_working_set() {
        // what this catches: the #268/#269 seam composing wrong — with a
        // budget above the cliff, the SECOND pass over the same working set
        // must hit cache and issue ZERO container reads. This is the whole
        // point of the lane: reuse > 0, measured, not assumed.
        use super::super::expert_ecache::{EcacheBudget, EvictionPolicy, ExpertEcache};

        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_container(dir.path(), 2, 8);
        let mut container = ExpertContainer::open(dir.path()).expect("open");

        let budget = EcacheBudget::derive(
            32 * RECORD_ALIGN, // 32 records available >> 2-per-token cliff
            manifest.activated_per_token,
            manifest.record_bytes,
        )
        .expect("above the cliff");
        let mut cache = ExpertEcache::new(budget, EvictionPolicy::Lfru);

        let working_set: Vec<ExpertKey> = (0..2u16)
            .flat_map(|layer| (0..8u16).map(move |expert| ExpertKey { layer, expert }))
            .collect();
        let mut buf = vec![0u8; manifest.record_bytes as usize];
        let mut disk_reads = 0usize;
        for pass in 0..2 {
            for &key in &working_set {
                if !cache.touch(key) {
                    container.fetch(key, &mut buf).expect("fetch");
                    disk_reads += 1;
                }
            }
            if pass == 0 {
                assert_eq!(disk_reads, working_set.len(), "cold pass reads each once");
            }
        }
        assert_eq!(
            disk_reads,
            working_set.len(),
            "warm pass must be served entirely from cache — zero new disk reads"
        );
    }

    #[test]
    fn unknown_manifest_version_is_refused() {
        // what this catches: a v2 foundry container silently misread by a v1
        // reader — version gates the whole open.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 1);
        manifest.version = ExpertContainer::KNOWN_VERSION + 1;
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite");
        let err = ExpertContainer::open(dir.path()).expect_err("future version must refuse");
        assert!(matches!(err, ContainerError::ManifestVersion { .. }), "{err}");
    }
}
