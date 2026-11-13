# ORM and schema architecture

Design of record, 2026-11-13. Owner: Codex. This document consolidates the former
Rust ORM, decorator schema, ORM Phase 2, entity adapter, storage adapter and elegant
CRUD designs. It replaces their competing conversion, fallback and compatibility
instructions. Historical implementation claims in those documents are not evidence
of the current runtime.

The engine design owns serving, KV paging, prefix reuse, admission, GPU embeddings
and vector search. This document owns their storage boundary: native payload bytes,
typed descriptors, efficient relational selection and durable custody. Neither owner
introduces another scheduler, resource authority or cache framework.

## 1. One declaration, one storage owner

An entity declaration is the source of truth for fields, exact types, nullability,
creation defaults, relationships, foreign keys, indexes and named projections.
Rust derives and generated decorator metadata feed one typed schema representation;
there must not be a second handwritten table definition or field-name heuristic.
Unsupported metadata is an error, never silently dropped.

The existing shared storage owner (`continuum-kernel::orm`, exposed through
`continuum-store`) owns provider connections, statement construction, transactions,
query execution and committed revisions. SeaORM is the relational implementation.
Domain code uses typed owner interfaces; it does not instantiate database providers
or issue SQL. In-process calls stay typed and do not take an IPC/JSON round trip.
External clients use the same owner through its transport boundary.

Adding an entity extends its declaration and registration. It does not add another
CRUD implementation, serializer, connection pool, event dispatcher or cache policy.
Use composition and shared typed interfaces for common behavior. Derived metadata
is compiled once and reused, not reflected or rebuilt for every row.

## 2. Separate searchable columns from opaque payloads

Choose representation by the operations consumers actually need:

| Data | Declaration and storage | Read behavior |
|---|---|---|
| Identity, relationship, status, ordering key | Exact typed scalar column, declared constraints | Bind and decode directly to the requested type |
| Count, timestamp, revision | Exact integer with explicit range/unit; no float intermediate | Database comparison and aggregate |
| Embedding, tensor, image or other native payload | BLOB plus typed descriptor | Owned bytes or a valid borrowed/mapped view; no element decoding |
| Payload whose internal fields must be queried | Promote those fields into declared columns; genuinely document-shaped queries may use declared database-native binary JSON | Execute predicates in storage; materialize only requested result |
| Opaque addressed document or event | Original bytes plus schema/producer identity | Parse only at its addressed consumer if required |

Do not serialize a whole entity into a JSON body and then deserialize it to find its
status, owner, timestamp or dimensions. Do not maintain duplicate searchable values
by ad hoc caller code: the schema and shared write owner declare and atomically
maintain the needed columns. Do not infer a type from a name such as `embedding` or
from the first sample row.

Integers stay exact; NULL is distinct from missing, empty and zero. A creation default
applies only where the declaration explicitly permits it. A default never repairs an
invalid persisted value on read. Join nullability remains explicit, including an
unmatched outer-join row. A projection has its own result type: do not deserialize a
partial row into a full entity and manufacture missing fields.

## 3. Native binary boundary with the engine

The producer supplies bytes and a typed descriptor. The descriptor identifies the
element representation, shape, byte order, layout and producer/model identity.
Normalization, quantization parameters and precision/error guarantees must be explicit
when the consumer depends on them. Opaque nonnumeric payloads identify their format;
the storage contract is not an f32/f64-vector special case.

The producer chooses its representation and performs normalization or precision
selection once in its compute path. Storage binds and returns those native bytes.
It does not iterate through elements to encode, decode, normalize, check finiteness,
change endianness or promote precision. An incompatible compute consumer performs
necessary translation on the GPU, retaining the compatible result under the existing
cache identity rather than converting again per use.

Descriptor validation checks representation support, shape/stride arithmetic overflow,
payload length, identity and schema compatibility. It visits the small header, not
payload elements. Any numerical validation belongs to the producer's measured GPU
work. The descriptor and bytes commit atomically; a failed update cannot pair an old
payload with a new descriptor. Their revision is one identity for cache invalidation.

Payloads never pass through `serde_json::Value`, textual numbers, base64 or a second
in-memory numeric array on the native path. A required external encoding terminates
at the actual transport boundary; it does not become the internal storage contract.
Generic list/projection APIs must not accidentally hydrate binary bodies.

A returned buffer has explicit lifetime and custody. Mapped pages remain pinned while
a view is in use; a DMA source remains owned until the existing completion fence
confirms transfer. Request cancellation is not transfer completion. ResourceDaemon
remains physical authority for destination, overlap and scratch allocations.

On unified memory, use a valid shared mapping suitable for a no-copy Metal buffer.
On a discrete GPU, use the existing asynchronous transfer pipeline and overlap useful
work. SQLite returning a BLOB does not by itself prove mmap-to-Metal compatibility or
zero copies. Record actual copies, alignment, mapping lifetime and transferred bytes.
Do not expose SQLite internal pages as stable application buffers or claim zero-copy
because the machine has unified memory. Bulk copies required by a provider are
measured explicitly; per-element conversion is not an acceptable substitute.

## 4. Select before materializing

Queries are typed intermediate objects compiled by the shared owner. Entity metadata
validates fields, relationships, projection and ordering; values are bound parameters.
No SQL in application callers or repair utilities. Provider-specific operations stay
inside the ORM. No Python in this design, including the offline migration.

Filtering, joins, ordering, counts, aggregates and limits run in the database. Positron
badges request counts; lists request declared summary projections. Detail payloads are
requested explicitly by identity or through a bounded payload read. A count never
loads rows, and projection never means fetch-all then discard columns in application
code. There is no missing-projection fallback that returns every body.

Each page requires a positive size and stable cursor order with a unique tie-breaker.
An example is `(created_at, id)` under an equality predicate on `owner_id` and `status`.
Declare the matching compound index in the entity, including direction and required
projection coverage; verify the actual query plan. Indexing every field separately or
marking a field as summary does not establish a covering index.

Cursor comparison must use the same types, collation, NULL ordering and direction as
the query. Joined pages specify their cardinality and stable identity. Concurrent
updates require stated snapshot/revision semantics; a cursor alone is not a snapshot.
Replay traverses bounded pages through the existing stream owner, preserving its
required causal order. Never fetch millions of bodies to clip or count afterward.

Cache compiled schema/projection mappings and prepared query shapes where the provider
supports them. Bind values separately. Decode selected scalars directly into typed
results; preserve payload ownership through the last consumer. Measure database work,
row materialization, typed decoding and transport separately to expose hidden CPU work.

## 5. Commit, notify, reclaim

The existing transaction owner retains a submitted write until commit or rollback,
including when the requesting task is cancelled. Publish revisions and data events
only after commit, through one shared mechanism. Events carry identity/revision and
the declared needed projection, not an automatic copy of the whole body. Domain events
remain with the domain owner. Deleting a record does not justify first hydrating its
payload merely to emit a deletion event.

Cache identities include the store, collection, record revision, representation and
producer identity required for compatibility. Committed writes invalidate the relevant
view. Retained readers hold immutable snapshots; they never observe a half-updated
payload/descriptor pair.

Connections, cursors, mapped pages and caches consume existing resource grants.
Waiting releases execution workers and MindTurn ownership. Pools and transient work
must not introduce arbitrary global caps or independent resource policies.

Entity metadata declares durability, reconstruction dependencies, permitted tiers,
retention conditions and relationship constraints. The storage owner reclaims only
eligible resources and reports bytes actually released after completion. Idle data
may move to a cheaper tier; authoritative knowledge is not deleted because it is cold.
Do not retain a second cleanup framework beside this owner.

AIRC remains authority for identities, trust, room membership and provenance. Local
rows are projections or publication obligations, not substitutes for that authority.
Preserve authenticated source identity, ordering, reply threading and unknown supported
extensions. Generic data access must not bypass a domain's protected write owner.

## 6. Offline migration and deletion

This is development: there is no compatibility requirement. Implement the one-time
migration in Rust, using the same complete entity registry and shared storage owner.
Any unavoidable provider-specific migration operation is contained in that owner's
migration module, not in runtime application code. No runtime old-format decoder,
dual backend, automatic fallback or feature flag selecting an obsolete implementation.

The migration takes explicit source/destination scope and an exact schema version.
Inspect active builds and supported service lifecycle first; quiesce the relevant
writers, preserve AIRC keys/identity and unrelated work, and produce a recoverable
copy before transforming authoritative data. Best-effort data preservation must not
block the redesign, but omissions or invalid rows require an explicit receipt.

Transform in bounded transactions. Validate row identities, exact scalar values,
NULL handling, relationships, payload byte identity, descriptor compatibility and
cursor order. Record source/destination schema digests, counts, rejected rows and
completion. Prove restart and interrupted-migration behavior. Switch the owner to the
validated destination through supported lifecycle, then delete superseded runtime
implementations and their callers. A rollback restores a known artifact/data pair;
it is not a permanently retained alternate runtime backend.

The former Python learned-NULL repair is not the new migration. Replace its required
operation in the Rust migration with an explicit, evidence-based rule; do not silently
substitute a progress value for unknown training state.

## 7. Proof and current implementation gaps

Use the same dataset, query shapes, concurrency and exact artifact revisions before
and after. Measure store operation p50/p99, queue/connection wait, database execution,
materialization/decoding CPU time, allocations, bytes copied/transferred, memory and
end-to-end act latency. Separate warm-cache, cold-read and restart results.

Prove joins, exact integers, NULLs, stable cursor pages, transaction rollback,
cancelled-request custody, descriptor/payload atomicity and restart. Prove byte-for-byte
retention for multiple native representations without payload interpretation. For
query improvements, inspect the provider plan and assert that counts/projections do
not fetch bodies. Integrate Fable's GPU search at 10k/100k vectors and measure latency
and recall against the stated reference; storage does not claim GPU quality itself.

A deployment requires the exact built/installed/running artifact and a 15-minute live
comparison with neighboring residents progressing. Tests or a process health label
are not adoption, performance recovery or learned behavior.

As of 2026-11-13, PR #5954 is draft and undeployed. Its branch has the SeaORM provider,
typed schema equality, bounded caller migrations and database-side counts, but the
native binary boundary, complete typed caller conversion, Rust offline migration,
independent review and live proof are unfinished. The uncommitted f64-specific codec
was rejected; its per-element conversions must be deleted, not published. Existing
JSON row hydration, optional unbounded query surfaces and static resource caps remain
implementation gaps. This document states the contract, not that those gaps are fixed.
