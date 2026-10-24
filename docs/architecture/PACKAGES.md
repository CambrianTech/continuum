# Packages: one shape for everything a mind owns, saves, moves and shares

**Status:** design, 2026-10-24 (Fable). Joel: "Packaging needs an elegant design, think about it like
projects, shareables, eventually in and out of serialization with good structure."

## Why

A mind's state is saved today as loose files: a KV page is `a-<persona>.bin` plus `.ckpt`, `.lora` and
`.dft` sidecars, and its compatibility lives elsewhere, in a per-directory `saved-ledger.json` keyed on
model paths and mtimes. Publishing four files as one took a staging dir, a commit record and a reader
that finishes interrupted commits (fork #70), and every review round found another durability edge in
that machinery. A page cannot leave the node it was made on. Genes have three manifests
(`TrainedAdapter`, `GeneHandle`, `BundleManifest`, plus the doc-only `genome/manifest.json`), three id
schemes (`ArtifactId` = 16 of 32 SHA-256 bytes, `AlloyHash` = `"sha256:"` string, `GeneBundle.digest`
= bare hex) and lineage recorded twice. Projects and activities have no export at all.

The repo already chose the answer for genes (REPO-GENOME-AND-COURSES.md §6: "OCI-first ... One
manifest, one digest identity, three transports"). This document makes that the shape of EVERY
package, KV pages included.

## The model (OCI's, adopted, not reinvented)

- **Blob:** immutable bytes named by their digest. Written once (temp, sync, rename to its digest
  name); a second write of the same bytes is a no-op. Blobs never change, so they never need a commit
  protocol.
- **Manifest:** a small typed document naming the package's kind, schema, what it requires, and its
  parts as `(media type, digest, size, annotations)`. A manifest is itself a blob, so its digest IS the
  package's identity, and it covers every part (a Merkle root).
- **Ref:** a mutable name pointing at a manifest digest (`a-benchy` -> `sha256:...`). Publishing a new
  generation = write the new blobs, write the manifest, then ONE atomic rename of the ref. That single
  rename replaces the staging dir + commit record + roll-forward reader of #70. A crash before the
  rename leaves the old generation current and some unreferenced blobs that eviction collects.

Because this is the OCI image-manifest model, a package pushes to and pulls from any OCI registry
(GHCR, Docker Hub, self-hosted) unchanged, travels over airc as the same digests, and publishes to HF
with the manifest beside it. One identity, any transport.

## Identity: one id

`ContentId([u8; 32])`: the full SHA-256, on the stack, rendered `sha256:<hex>` only at the edges (an id
is a u128-class value, the string is a rendering). It replaces `ArtifactId`'s truncated UUID,
`AlloyHash`'s string and the bundle's bare hex. Provenance (`GenomeProvenance`, ed25519) signs a
manifest's `ContentId`; lineage is the manifest's `parents: [ContentId]`, recorded once.

## Kinds

One envelope, a typed `kind`, a schema number per kind. Parts are typed by media type.

| Kind | Parts | Requires | Notes |
|---|---|---|---|
| `kv-page` | sequence state, context checkpoints, draft (MTP) state, speculative tail | model content id + KV format (cache types, n_layer, n_embd_k/v, recurrent shape); the gene set it was computed under, as gene `ContentId`s | replaces `.bin` + sidecars + the ledger's fingerprint; portable across nodes with the same model bytes |
| `gene` | adapter weights (GGUF LoRA), behaviour signature, README (outside identity) | base model content id | replaces `BundleManifest`, `TrainedAdapter`, `GeneHandle` (a handle = a `ContentId` + a locator) |
| `memory` | engram/recall shards | the embedder's content id | personal: encrypted parts, shared only with her permission and keys |
| `project` | refs only: repo + rev, activity recipe, the genes and memories it uses | n/a | a shareable bundle; carries digests, never copies of personal data |

`requires` is what makes a package refuse instead of corrupt: a `kv-page` whose model or KV format
does not match the engine is refused with a typed reason before any byte is read (today that check
lives in the ledger and the engine's magic, separately).

## Where it lives

- **Rust:** a leaf crate `continuum-package` (envelope, `ContentId`, `PackageKind`, the blob store,
  reader/writer, `requires` checks; ts-rs for clients). Every other format migrates onto it.
- **Store:** `~/.continuum/store/{blobs/sha256/<hex>, refs/<scope>/<name>}`, one per machine, shared
  by every model and mind (docker's layer store). It is a `TrackedDir` with an eviction decision:
  derived parts (KV) evict by rebuild-cost x reuse / bytes; a last copy is never evicted.
- **Engine (fork):** the server writes and reads `kv-page` packages in the same layout through a small
  C++ writer (blobs, manifest, ref rename); verbs and page names do not change. The S4 contract
  (`page_out|page_in|spill|page_drop`, typed page fates) is untouched.

## Cost on the hot path (ballpark first, then measured)

A 27B page is ~2.1 GB. SHA-256 with the ARM SHA extensions runs on the order of 2 GB/s per core, so
hashing serially would add about a second to a ~0.6 s spill. The digest is therefore computed in
stream, on the part's own thread, while the bytes go to disk, and parts hash in parallel. Validator:
spill time with and without the digest at 35k tokens on the 27B; if the digest adds over 10%, local KV
parts are named by a cheap generation id and get their digest lazily, at the first export. Either way
the manifest format does not change.

## Slices (each lands with its validator, each deletes what it replaces)

1. `continuum-package`: `ContentId`, envelope, blob store, ref rename. Validator: crash at every step
   of a publish leaves the old ref readable; 1k publishes leave no orphan after eviction.
2. Fork: `kv-page` packages replace `.bin` + sidecars + the staging/commit code of #70 (net deletion).
   Validator: the existing page tests, plus spill timing on the 27B against the 588 ms disk save.
   Existing sidecar pages are an intentional, classified cold start once (no dual reader).
3. Rust KV side reads `requires` from the package; `saved-ledger.json` keeps only slot occupancy.
4. Genes: `gene` packages replace `BundleManifest`/`TrainedAdapter`/`GeneHandle`; one id scheme;
   HF/GH publishers push the same package.
5. A page moves between two nodes over airc by digest (the first portable mind state).
6. `project` bundles and OCI push/pull.
