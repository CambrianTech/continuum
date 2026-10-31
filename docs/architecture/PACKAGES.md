# Packages: one load/store for every layer, any depth

**Status:** design, 2026-10-24 (Fable). Joel: "A packaging solution is necessary all over ... think in
and out of everything, a load/store customizable for anything. I'd do this for all my layers." /
"Anything that needs to store things gets it easily, by merely implementing. And it handles any depth."

## The problem, measured (core/, 2026-10-24, `rg -t rust`, vendor excluded)

- ~51 `fn save*`, 120 `fn load*`, 51 `*persist*`, 75 `*restore*` implementations; serde_json called
  ~540 times each way; 749 `fs::write(` sites.
- 44 hand-written temp+rename sites in 40 files beside three `continuum-base` helpers with 1-6 callers
  each; most never fsync; four are bare non-atomic writes (`grid/registry.rs:331`,
  `code/repo_registry.rs:36`, `served_window_store.rs:184`, `plasticity/topology.rs`).
- Four copy-pasted engine map stores (`decode_knee`, `serving_rates`, `prefill_rate`, `lane_footprint`)
  that ignore a failed rename and read a corrupt file as empty: a silent fallback, four times.
- A KV page is `.bin` + `.ckpt` + `.lora` + `.dft`, its compatibility in a separate ledger keyed on
  paths and mtimes; publishing the four as one grew a staging dir + commit record + recovery reader
  (fork #70). A page cannot leave its node.
- Genes: three manifests (`TrainedAdapter`, `GeneHandle`, `BundleManifest`), three id schemes
  (`ArtifactId` = 16 of 32 SHA-256 bytes, `AlloyHash` string, bundle bare hex), lineage twice.
- Room/activity state stored three ways (airc wall, ORM, local JSON); no project or persona export.

Every layer re-solves the same four problems (encode, write atomically, version, move) and each
solution is partial. The fix is one layer every type plugs into.

## The design: a type says WHAT it is; a store says WHERE bytes go

Two traits, separated the way NSCoding separates an object from its archive and OCI separates an
image from its registry. Neither knows the other's half.

### `Packable`: the type's side, customizable, usually derived

```rust
pub trait Packable: Sized + Send + Sync + 'static {
    const KIND: Kind;                      // Kind { name: "gene", schema: 3 }
    type Context: ?Sized;                  // what loading needs from the host: model, embedder, keys
    fn pack(&self, p: &mut Packer<'_>) -> Result<(), PackError>;
    fn unpack(u: &mut Unpacker<'_>, cx: &Self::Context) -> Result<Self, PackError>;
}
```

A type describes itself as parts. The `Packer` verbs are the whole vocabulary:

| verb | what it is | stored as |
|---|---|---|
| `field(name, &T)` | small structured data (serde) | inline in the manifest |
| `blob(name, media, source)` | large bytes: a buffer, an mmap, a reader, or a writer closure (the engine fills it) | a blob, streamed and hashed in flight, never copied |
| `child(name, &C)` / `children(name, iter)` | another `Packable`, any kind | a blob that is the child's manifest: recursion |
| `secret(name, &T, audience)` | a field or blob only its owner/audience may read | encrypted part; the digest covers the ciphertext, so stores verify without keys |
| `requires(req)` | what a loader must match: `Model(ContentId)`, `KvFormat{..}`, `Embedder(ContentId)`, `Key(audience)` | in the manifest, checked first |

`#[derive(Packable)]` makes this one line per type; attributes customize parts:

```rust
#[derive(Packable)]
#[pack(kind = "gene", schema = 3, migrate = gene_migrations)]
struct Gene {
    #[pack(blob = "application/vnd.gguf.lora")] weights: Blob,
    signature: BehaviourSignature,               // field
    #[pack(child)] parents: Vec<Handle<Gene>>,  // lineage, recorded once
    #[pack(requires)] base: ModelId,
}
```

A hand-written `pack`/`unpack` is for types that need more (the KV page, whose bytes come out of the
engine); the derive covers the rest.

### `Store`: the medium's side, five verbs

```rust
pub trait Store: Send + Sync {
    fn put(&self, src: BlobSource) -> Result<ContentId, StoreError>;     // idempotent, hashes in flight
    fn get(&self, id: &ContentId, range: Range) -> Result<BlobRead, StoreError>; // bytes | mmap | stream
    fn has(&self, id: &ContentId) -> bool;
    fn set_ref(&self, name: &RefName, to: ContentId, expect: Option<ContentId>) -> Result<(), StoreError>;
    fn get_ref(&self, name: &RefName) -> Option<ContentId>;
}
```

Blobs are immutable and named by content, so they never need a commit protocol. `set_ref` is the ONLY
mutation: one atomic compare-and-swap of a name onto a manifest. Implementations:

- `DiskStore`: `blobs/sha256/<hex>` + `refs/<scope>/<name>`. Its one write path (temp, sync, rename,
  dir sync, errors propagated) is where all 44 hand-written sites and the three base helpers go.
- `RamStore`: bounded, a `ResourcePool` (eviction already exists).
- `Tiered(RamStore, DiskStore, Remote)`: stores composed; promotion and demotion IS paging
  (VRAM > RAM > SSD > peer). `PagedResourcePool` and spill's `LivePin` become its policy, not a parallel path.
- `AircStore`, `OciStore`, `HfStore`, `GhStore`: remotes. Today's `Publisher` trait becomes these.

Transport between any two stores is one generic function: walk the DAG from a root, copy what the
destination lacks (`has`), flip the ref last. Dedup, resume and integrity come with it.

## Depth: a Merkle DAG

A child is a part whose bytes are another package's manifest, so depth is unbounded: a persona holds
memories that hold engrams; a project holds genes that hold their parents. Content addressing makes
it a DAG: a package's digest covers its children, so it cannot contain itself, and an identical
subtree (a gene twenty minds share, a common checkpoint) is stored once.

- **Save** walks bottom-up and flips ONE ref at the root: a whole persona commits atomically or not
  at all. A crash leaves the old root current and unreferenced blobs for collection.
- **Load** walks top-down and lazily: `Handle<T>` is a `ContentId` + a store; `.load()` fetches on
  first use; blob reads take ranges (spill's `read_extent` is this). Opening a project does not pull
  every gene's weights.

## Identity

`ContentId([u8; 32])`: full SHA-256, on the stack, rendered `sha256:<hex>` only at the edges. The
manifest is RFC 8785 JCS canonical JSON, so the same value always has the same id; an OCI store wraps
it in an OCI artifact manifest (OCI annotations are strings only). Provenance is the package's alloy. This one id replaces `ArtifactId`, the bundle digest and the model-dir hash an alloy would otherwise compute.

## Provenance: forge-alloy, the chain we already ship

Joel: "Merkle maintains provenance and locks it all down cryptographically" / forge-alloy "is already
being used for provenance and Merkle, ties in so well for distribution purposes."

A package says WHAT a thing is; an **alloy** (github.com/CambrianTech/forge-alloy) says WHO did WHAT to
make it: a chain of signed stages, `input hash -> stage -> output hash -> signature`, verified offline,
in the browser or from a QR code, already domain-agnostic (adapter, dataset, compute receipt,
delivery). Packages carry alloys; they never grow a second provenance format.

- **A package's provenance part is an alloy**: a child of media type `application/vnd.forge-alloy+json`
  whose final stage's output hash is the package's `ContentId`. A gene from its training run, a page
  from a turn, a persona from her history, a published model from its forge: each arrives with the
  chain of how it was made. `GenomeProvenance` (a separate ed25519 format) retires into it.
- **One canonical form.** Manifests are RFC 8785 JCS, the canonicalization alloy already signs, so a
  manifest's id and an alloy signature are computed over the same bytes rule and one verifier serves
  both.
- **One hash per artifact.** alloy names a packaged artifact by its `ContentId` (a `package` domain
  in alloy), never by a second scheme over the same bytes; `forge-alloy verify` then works on
  anything packed. The root's id commits to every byte at any depth, so the alloy's signature over it
  vouches for the whole tree.
- **Verified on every load and every transport.** A blob must hash to the id that names it whichever
  store or peer it came from: a remote, cache or registry is trusted for availability, never integrity.
- **Lineage is identity.** Parents are children or ids inside the manifest, and the alloy's stage
  inputs name them, so provenance cannot be stripped without changing what the thing is. Credit
  travels with the work.
- **Who may load what** is `requires(Key(audience))` plus the signer's trust scope: a mind accepts
  packages whose alloy is signed by scopes it trusts and refuses the rest with a typed reason.
- **Migration keeps the chain.** A migrated package has a new id; its alloy gains a `migrate` stage
  (old id -> new id), signed by whoever ran it, so the original signature still verifies its own link.
- **Layering.** `continuum-pack` (base layer) knows only the provenance part's media type. Attaching
  and verifying alloys sits one layer up (or a `pack` feature of the forge-alloy crate), so the base
  never imports model-forge stage types.

## Versions: a migration chain, never two readers

Each kind carries a schema number and an ordered list of pure functions `manifest(n) -> manifest(n+1)`,
run on load. An unknown newer schema, a digest mismatch, or an unmet `requires` is a typed refusal,
never a default (`unwrap_or_default` on a corrupt file is the defect this removes).

## What it is not

- The ORM stays the QUERY layer for records (`OrmEntity`, `StorageAdapter`). A package can be indexed
  into a collection (its ref + queryable fields); rows do not become packages.
- IPC frames and live wire messages stay wire. Append logs (captures, `experience.jsonl`) keep
  `CappedAppender` for the open tail; sealing tail segments into packages is a later slice.

## One conformance suite for every type

`pack::conformance::<T>()` runs for every `Packable`, so a type that implements the trait inherits the
proof: round trip; same value = same id; a fault at every store operation (a `FaultStore` wrapper)
leaves the previous root loadable; a flipped byte is caught; an unmet `requires` refuses before any
blob read; every recorded past schema migrates; a shared child is stored once; a lazy child loads.
Test the contract once; never per type.

## Cost (ballpark, then measured)

Small state is a manifest-only package: one blob and one ref, two syncs. A 27B KV page is ~2.1 GB;
SHA-256 with the ARM extensions is on the order of 2 GB/s per core, so serial hashing would add ~1 s to
a ~0.6 s spill. Parts therefore hash in flight on their own blocking threads, in parallel. Validator:
spill at 35k tokens on the 27B with and without the digest; over 10% added and local KV parts take a
generation id with the digest computed at first export, with no change to the manifest format.

## Where it lives and the order it lands

`continuum-pack`, a leaf crate beside `continuum-base` (no upward imports), with the derive in
`continuum-pack-derive` and ts-rs exports for clients. The fork's server gets a small C++ writer for
the one kind it produces (the KV page) in the same `DiskStore` layout.

Each slice lands with its validator and deletes what it replaces:
1. The crate: `ContentId`, `Packable` + derive, `DiskStore`, `RamStore`, `Handle`, conformance suite,
   `FaultStore`. The base atomic helpers fold into `DiskStore`.
2. The four engine map stores, `served_window`, `engine_residency`, `mind_state`, `room_directory`,
   `repo_registry`, `grid/registry` become derived `Packable`s (mechanical; one PR, cheap model).
   Then the remaining hand-written temp+rename sites migrate by owner.
3. The KV page kind, in the fork and the engine: `.bin` + sidecars + #70's staging/commit code go; the
   ledger keeps only slot occupancy. Old sidecar pages are a classified one-time cold start.
4. The gene kind: `gene_bundle`, `adapter_manifest`, `GeneHandle`, `ArtifactId` fold into one; HF, GH
   and OCI become stores.
5. The persona as a package: identity, memories, genes (by ref), pages, volatile state as children,
   with secrets for what is hers alone. A whole mind backs up, restores and moves between nodes.
6. `AircStore` transfer between nodes; `project` packages (refs only, never copies of personal data).

## Found while building slice 1 (#5371), and the answers

1. SHA-256 measured 603 MB/s (software path; sha2 needs its `asm` feature for the ARM SHA unit): a
   2.1 GB page is ~3.5 s, not ~1 s. Hardware hashing is a slice-3 prerequisite; failing that, local
   KV parts take a generation id and their digest at first export.
2. `has` and `get_ref` return `Result`: a disk error is never "absent".
3. `set_ref(expect: None)` means "must not exist"; there is no blind overwrite.
4. A range read cannot be checked against a whole-blob hash: verified ranges need per-chunk hashes
   (a chunked blob part). Until then the method is `read_range_unverified`.
5. An OCI store wraps the manifest; it is not an OCI manifest as-is.
6. Requirements are keyed by part name; `KvFormat` gets defined fields in slice 3; audience matching
   is any-of.
7. Migration re-ids a package; its alloy records the `migrate` stage (above).
8. A new field, even optional, is a schema bump plus a migration: a missing field is an error.
9. Transport trusts "a stored manifest implies its children are stored": collection must keep it,
   and a scrub verifies stored blobs and repairs from a peer.
10. `RamStore` cannot be a `ResourcePool` from the base layer; the pool wraps it one layer up.
11. Sealed parts derive their nonce from the content and the audience key, so the same value keeps
   the same id.
12. Still to come: parallel per-part hashing, ts-rs exports, a derive attribute for a load context.
