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

String-backed enums used in predicates declare `#[entity(text)]`: their stored value
is the scalar string, not a quoted JSON document. The shared write codec refuses
nonstrings; indexes and query bindings operate on the same declared representation.
Predicates and cursors bind through the declared scalar write codec. Wrong-type
values and scalar comparisons against opaque JSON/BLOB fields fail before execution,
including count/delete on empty collections; they must never masquerade as zero
matches. NULL/existence predicates remain available for opaque fields.

Integers stay exact; NULL is distinct from missing, empty and zero. A creation default
applies only where the declaration explicitly permits it. A default never repairs an
invalid persisted value on read. Join nullability remains explicit, including an
unmatched outer-join row. A projection has its own result type: do not deserialize a
partial row into a full entity and manufacture missing fields.

## 3. Native binary boundary with the engine

The existing migration engine reads bounded ID-keyset pages on the shared owner's
snapshot and transfers each row's native fields through `create_binary`. Payloads
never enter the scalar record envelope; each descriptor and byte allocation moves
unchanged into the destination transaction. Optional NULL pairs remain absent.
The whole source page validates before its first destination write. Destination
commits remain per row, so later failure can leave an explicitly incomplete target.
Verification now requires completed write receipts, matching declarations and exact
scalar values, native descriptors and bytes using bounded pinned pages on both stores.
It detects equal-count payload or producer changes. Writers must remain quiescent:
per-collection snapshots do not certify a globally atomic database cutover. This
transfer mechanism does not yet provide legacy-schema conversion, durable resume
or the offline cutover command. Those remain required before deployment.

The producer supplies bytes and a typed descriptor. The descriptor identifies the
element representation, shape, byte order, layout and producer/model identity.
Normalization, quantization parameters and precision/error guarantees must be explicit
when the consumer depends on them. Opaque nonnumeric payloads identify their format;
the storage contract is not an f32/f64-vector special case.

The GPU consumer contract (Fable, 2026-11-13, PR #5954 comment 6866576813) adds
`unit: bool` as the producer's assertion of L2 normalization and `scale: Option<f32>`
for symmetric I8 quantization. Absence of scale does not imply a scale of one;
consumers must reject representations they cannot compute on. A supplied scale is
finite and positive and applies only to I8. Storage preserves the assertion and
scale; it does not establish normalization by scanning the bytes. The current
embedding provider interface does not certify normalization, so backfill records
`unit=false` until its producer provides that guarantee.

`ElementType::Bit` describes packed sign sketches: low bit first within each byte,
with shape measured in bits and trailing bits beyond the shape ignored by consumers.
The vector and its sketch are separate declared binary fields committed by the same
existing transaction. Descriptor format 2 carries these facts; format 1 is refused,
not decoded through a compatibility path. Existing persisted format-1 data requires
the explicit offline migration before adoption. Bulk reads, revision-watermarked
delta replay and the GPU consumer remain separate implementation obligations; these
descriptor fields alone do not establish GPU search or its quality.

The bulk reader appends native rows to caller-owned contiguous bytes and parallel
UUIDs. A BinarySnapshot pins the collection and native revision in the existing
SeaORM read transaction, including when the native head is absent. All pages on that
handle see the same database state. WAL writers can commit with available pool capacity;
each snapshot leases a connection, so retaining all connections can delay writers. Drop releases
the transaction; consumers must release it after extraction, before waiting on GPU work.
Its borrowed BinaryPageRequest groups field, cursor and a required
NonZeroUsize page size; BinaryPageOutput groups the caller's mutable buffer views.
Only the cursor is optional because beginning traversal is a valid state. The old
positional signature and runtime zero-size refusal are removed.
It borrows SQLite payload/header views while their query results remain
owned, decodes one common descriptor, validates the whole page, and then copies each
payload once into the destination. There is no per-row payload/header allocation and
no numeric materialization. NULL pairs are excluded in storage before LIMIT; a partial
NULL pair or mixed descriptor refuses the page without appending output. Capacity may
be reserved on refusal, but existing buffer contents and lengths remain unchanged.
This is not mmap/Metal zero-copy: SQLite owns its result buffers. The current cursor
is ID-ordered within that pinned snapshot. The native revision identifies payload
write/delete state, not scalar-filter invalidations or GPU adoption. IDs and cursors are typed UUIDs; native identity text
must be canonical lowercase hyphenated UUIDs so database ordering matches the cursor.
Native snapshot changes_into traverses coalesced native changes by descending revision
and UUID, with an exclusive since revision and composite continuation key. It appends
typed Upsert (descriptor plus absolute byte range) or Remove entries and native bytes
into caller-owned buffers. Tombstones and absent optional native fields produce Remove;
partial pairs or missing live rows refuse before either buffer changes. Continue on the
same snapshot with unchanged field and since; cursors are not independently bound.
End-of-interval means no further database page, not consumer/GPU adoption. Advance an
applied watermark only after all changes and downstream transfers complete. Scalar-filter
invalidations and GPU integration remain unfinished. Migration must populate native
revision receipts; arbitrary preexisting payloads are not inferred.

Native create/update now record an internal per-row revision in the same committing
transaction as the bytes and descriptor. The existing sequence-head owner allocates
revisions in a typed native domain, separate from append sequence custody; a single
UPSERT RETURNING advances a head and refuses overflow. Failed transactions roll back
both head and row receipt, and an unchanged conditional write consumes no revision.
Direct predicate deletes, batch deletes and owner clear now mark matching native
receipts deleted in the same transaction. A database-side identity subquery selects
victims without materializing bodies or a CPU victim list; no matching receipt means
no new revision. Delete primitives require a transaction rather than an arbitrary
connection. Rollback and recreation preserve the head and tombstone contracts.
Declared foreign-key cascades now contribute their transitive identity closure through
one recursive ORM query, using UNION to deduplicate cycles and converging paths. Only
reachable native collections receive tombstones; SET NULL survivors stay live and a
RESTRICT failure rolls back every receipt. The owner restores and physically verifies
its persisted schema catalog once at open using primary-key receipt pages (64 per
transfer, with no collection-count cap), so children participate even before their
caller re-admits a declaration. A new external declaration requires admission or a
new owner before deletion; an incomplete catalog cannot silently miss descendants.
Declaration types reject unknown fields at every level. Collection index lists, field
nullability/index/unique flags, index direction and foreign-key actions are required;
optional insertion defaults, length limits and references retain valid absence.
Restoration decodes each receipt once and verifies physical metadata without rebuilding
a JSON declaration. Re-admission compares typed schemas using the same shared equality
as registration. Physical metadata comparison remains exact. Incomplete old declarations
are refused; migration must supply an explicit complete declaration, never a fallback.
The existing committing owner publishes typed domain/collection revision notices only
after commit. Append and native receivers share that owner but never share a revision
domain. Native create/update, direct deletes, batch deletes, cascades and clear return
their notices through the transaction result; rollback and conditional no-op emit none.
Persisted heads seed receivers on owner restart. These are wakeup hints backed by
durable heads, not a consumer adoption proof: downstream delta application and completion
semantics must land before consumers may use a high-water mark as processed work.
The current receipt table coalesces each identity to its latest revision. A fixed
upper revision with live paged queries is therefore eventual reconciliation, not an
as-of snapshot: a row can move past that bound before its page is read. Exact snapshot
completion must retain one existing-owner read transaction across head capture,
metadata and payload pages. Scalar changes and FK-driven updates currently do not
advance native revisions; a filtered corpus must include their invalidations before
this receipt stream can claim membership completeness.
The sequence-head declaration now has a composite domain/collection identity; any
previous development database needs the offline schema migration before adoption.

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
Native updates accept a nonempty map of declared payload fields, binding all bytes and
descriptors in one UPDATE with one revision. Conditional initialization succeeds only
when every supplied pair is NULL; a populated member makes the whole group a no-op.
Unknown fields or invalid descriptors refuse before dispatch. There is no retained
single-field update path.

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

The former Python learned-NULL repair and its tests are deleted. Its narrow replacement
is the Rust `migrate-training-progress` executable; its SeaORM implementation lives in
the storage owner's offline module and is not included in the runtime ORM. Run
`cargo run -p continuum-kernel --bin migrate-training-progress -- OFFLINE_DATABASE`
on an explicitly prepared offline copy for a read-only inventory. After establishing
from provenance that the historical NULLs mean unstarted, add
`--apply-historical-unstarted`. This explicit operation changes only those NULLs to
zero, refuses invalid storage classes/ranges before mutation, and verifies cardinality
inside one transaction. It never interprets unknown progress as unstarted automatically.
The receipt records rows, historical NULLs, changed rows and whether changes applied.
This narrow repair does not perform the full schema/native-payload migration above.

### Runtime orchestration and deterministic completion

Publish work through its existing owner and await its typed result or completion
receipt. Elapsed time is not evidence of completion, cancellation, resource release
or physical death. Use event barriers for ordering and fault replay; do not use sleeps
or arbitrary timeouts to establish correctness. Model absence with Option, failure
with Result and explicit error variants, and admissible persisted states in schemas.
Required inputs are required fields; Option represents meaningful absence, never a
required input deferred to a missing-parameter error. Group cohesive arguments into
named structs, borrow requests and views where possible, and move native buffers only
when ownership transfers. Types own validation, behavior and lifecycle through
composition. Unwrap/expect require a rare, proven invariant; recoverable states use
typed errors rather than panic or defaults.
Runtime and critical startup execute in Rust owners. Shell scripts or Python require
a concrete exceptional necessity or appropriateness; they are not orchestration
shortcuts. Development build tooling is separate from the installed runtime.
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
typed schema equality, bounded caller migrations and database-side counts. The
native binary owner and backfill byte path are implemented, but complete typed caller
conversion, full schema/native-payload migration, independent review and live proof
are unfinished. The f64-specific codec was rejected and deleted. Existing
JSON row hydration, optional unbounded query surfaces and static resource caps remain
implementation gaps. This document states the contract, not that those gaps are fixed.

Measured isolated native-sync replay (2026-11-14): 10,003 initial 768-f32 rows,
28 native updates and four deletions. A warm full read selected 9,999 live rows and
30,716,928 payload bytes in 95.561/98.267 ms; the native delta selected 32 changes
and 86,016 payload bytes in 0.958/0.457 ms. Alternating runs include validation and
exclude snapshot acquisition. This is a synthetic sparse-update storage comparison
with an assumed previously cached corpus, not bootstrap, CPU/RSS measurement,
GPU transfer/quality, resident latency or deployment proof. Exact changed identities,
descriptors, bytes and removals are checked; cold/restart load remains to be measured.

## 8. Simulation is part of every iteration

Before changing a data path, capture a reproducible workload and a failing or costly
case through the existing test/replay machinery. Derive the scenario from observed
faults, measured workload distributions and architectural failure modes. An isolated
happy-path microbenchmark is insufficient. Simulation supplements the live comparison;
it never substitutes for runtime adoption or genuine resident learning.

Use Rust fixtures and isolated databases, never the serving home or AIRC identity
store. A workload manifest records source revision, schema, seed, hardware/capabilities,
row counts, payload-size distribution, NULL fraction, query mix, arrival rate,
concurrency, cache state and injected delays/faults. Sanitize captured traces: retain
timing, shape and causality without copying credentials or private bodies. Synthetic
payload bytes should match observed sizes and compressibility. Mark extrapolated
loads as extrapolated, not measured production traffic.

| Observed fault or architectural risk | Required simulation | Invariant and performance evidence |
|---|---|---|
| Multi-GB staged credit bodies loaded during replay | Observed skewed body sizes, metadata-only replay, concurrent readers and restart | Bounded transient memory; exact replay order; unrelated bodies never fetched |
| A badge/list materializes rows to count or clips after 10,000 | Collections above the former cutoff with large opaque payloads; count and summary-page mix | Exact cardinality; no payload decoding; rows/bytes returned proportional to requested projection |
| Native payload converted per element | Multiple producer representations and large buffers; cold/warm/restarted reads | Exact bytes and descriptor; no numeric conversion; account for every bulk copy |
| Paging waits or stalls compete with useful work | Inject slow read/transfer completion while neighbors submit ready operations | Waiting holds no execution worker/MindTurn; source custody retained; neighbor progress measured |
| Requester cancellation or shutdown during a write | Cancel before dispatch, during transaction and around commit; restart | One committed outcome or rollback; no mixed payload/descriptor; no premature revision event |
| Concurrent writes disturb paged traversal | Duplicate sort keys, inserts/updates/deletes at cursor boundaries and outer joins | Declared snapshot semantics, stable ordering/cardinality and exact NULL handling |
| Relay refusal with more than one page of pending publications | 103 committed lines, close/reopen, ID-only indexed pages, exact retry and duplicate-import cases | Every pending line recovered; no body scan to select work; no fabricated sender fields |
| Local database handle bypasses the shared home owner | Explicit isolated home, main/metrics/persona/node handles, restart | All local handles resolve under the same owner; existing unmanaged schema remains refused |
| Unknown/corrupt metadata, schema drift or interrupted migration | Truncated headers, overflow shapes, wrong producer, interrupted bounded migration and reopen | Explicit refusal; no guessed type, fallback, silently dropped rows or identity substitution |
| A query is fast alone but queues under resident load | Replay measured inter-arrival times with bursts, mixed readers/writers and shared resource pressure | Queue wait and end-to-end p50/p90/p99, backlog drain, throughput and fairness |

Run the same manifest against the baseline and candidate. Keep both arrival-to-complete
latency and execution latency; a stalled system must not look fast because the load
generator stopped submitting while it waited. Record offered, admitted, completed,
failed and cancelled operations, outstanding work and useful completions. Test measured
normal load, observed bursts and saturation with available compatible capacity. Do not
mask overload with an arbitrary cap or by shrinking the tested workload.

Choose numerical performance/quality thresholds before the candidate run, based on the
measured baseline and the affected engine budget. Correctness and custody invariants
have zero tolerated violations. Inspect database plans, query counts, materialized
bytes, allocations, CPU time and transfer receipts to attribute any improvement.
Simulation must expose regressions in neighboring workloads as well as the target query.

For every iteration: baseline -> reproduce -> change the shared owner -> replay the
same workload/fault schedule -> compare -> independent review -> live comparison.
Save the manifest, exact heads, raw measurements and verdict with the change. A new
production failure becomes a retained regression scenario. A discovered unmodeled
queue or transfer updates the simulation before another performance claim. Use the
existing owners and harnesses; do not create a parallel simulator scheduler or bypass
resource, causality, identity or completion contracts to make a benchmark pass.
