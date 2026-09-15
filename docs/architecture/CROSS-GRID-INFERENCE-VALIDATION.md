# Cross-grid inference: command and evidence contract

The same `ai/generate` operation serves a local caller, a persona's remote lane,
and an explicit `aircPeer` command dispatch. Placement belongs to the substrate;
activities and benchmark recipes must not grow a second inference runner.

Generic peer command URIs already use `CommandExecutor`'s installed
`LateBoundAircTransport` and the same receiving command handler. New ML commands
should register with that executor and reuse this route. The `aircPeer` parameter
interceptor is inference-specific; it is not the limit of the generic transport.
Remote command authorization still applies: reachability does not grant access.

## Boundary

The requester carries a typed `TextGenerationRequest`. AIRC carries the existing
command envelope and routing headers. The selected peer executes its normal
`ai/generate` handler. Only the network boundary serializes the request; local
route metadata is updated in place.

An answer must match the requested logical peer, requesting peer, dispatch room,
wire correlation and command-response framing. Check those headers before
decoding the payload. Durable recovery uses the same predicate and the original
room, even if the caller changes activities while inference is running.

`routing.remote` records `requestedPeer`, `respondingPeer`, `correlationId`, and
caller-observed `elapsedMs`. The responding identity comes from the authenticated
AIRC event, never from copying the requested target. The correlation is the actual
wire request ID, not the remote adapter's separate internal ID.

This receipt proves who answered. It does not independently attest the responder's
hardware, model execution, or any onward delegation. The caller's routing provider
is `airc-remote`. Model, token usage, applied
adapters and served context remain responder-reported fields and must survive
local command/adapter wrapping. A future delegated execution chain must carry
verifiable linked receipts rather than relabel the first hop as the final executor.

## Validation ladder

1. Existing `airc_remote_inference_roundtrip` exercises two real AIRC loopback
   peers and the substrate parser/reply writer. Assert result fields, authenticated
   responder, actual wire correlation, nonlocal routing and elapsed time.
2. Existing `architecture_cross_grid_chaos` exercises a silent peer and a subsequent
   successful request. A timeout is a typed failure, never a successful local fallback.
   Timeout alone does **not** prove remote compute was canceled.
3. Header boundary regressions reject unrelated authors, rooms, recipients,
   correlations and non-response events. Routing preservation tests ensure both
   command wrappers retain the receipt and serving metadata.
4. On real mixed hardware, invoke the same command with the same task and sampling
   parameters locally and with `aircPeer`. Retain both responses and the original
   activity's turn evidence. Compare correctness first, then elapsed time, output
   tokens, concurrency, memory pressure and transfer measurements. Repeat trials;
   distinguish cold setup from warm serving. Do not infer network overhead by
   subtracting clocks measured on different machines.
5. Run ordinary team activities under automatic placement. Their benchmark adapter
   supplies tasks and grading only. A gain requires repeatable completed-work
   evidence, including failures; a connected fleet is not a throughput result.

Capacity discovery must distinguish a machine's accounting identity from its
callable runtime endpoint. Advertise a live, authorized command endpoint alongside
capacity; keep machine identity for resource accounting and freshness. A beacon
without a callable endpoint cannot justify inference placement. Validate this with
multiple resident personas so only the advertised handler answers, not whichever
persona hears a broadcast first.

## Remaining distributed-compute gates

### Event delivery and handle cost

Correct reply attribution is not a throughput measurement. The audited AIRC
command path at `829a8f8` opens an unfiltered room IPC subscription per pending
command. Unrelated events are serialized and decoded for each waiter before
correlation is checked. The inference recovery path also scans a recent-event
page every two seconds. These are known efficiency gaps, not the target design.

Use the existing router's header index to select a registered command handle
before IPC delivery or payload decoding. Register before publishing; remove the
registration on completion, drop, deadline or send failure. Queue bounds and
overload must be explicit. Local fan-out shares an immutable payload reference;
large remote content uses a resolvable artifact or stream handle rather than
being copied into every event. Local addresses are never valid remote handles.

Validate many concurrent commands amid unrelated text, binary and stream events:
only addressed replies may cross each command subscription, and unrelated
payload decode counts must remain zero. Measure warm completion latency and
aggregate delivery throughput separately from connection setup and inference.
An indexed subscription still incurs per-request IPC setup until a shared
session owns and multiplexes those registrations; do not call it zero-cost.

Recovery belongs to that delivery owner: cursor-based replay on reconnect,
followed by live event delivery under the same command deadline. Repeated
transcript scans must be removed once that contract is available and verified,
without adding a second reply router inside the inference adapter.

### Placement and artifacts

A command interface is necessary but does not make arbitrary kernels profitable
over a network. Imatrix calibration, expert fetching and genomic artifact work
must retain their existing mechanisms and declare capabilities, resource demand,
artifact identity and authorization at their command boundaries. Artifact handles
must resolve on the receiving node; local paths and process pointers cannot be
remote handles. Bulk transfers belong on the existing artifact/data plane.

Before claiming automatic wider-P2P execution, validate authorized discovery,
placement under measured transfer cost, cancellation propagation, bounded queues,
duplicate execution/idempotency policy and recovery after peer loss. Cancellation
of a requester future is not evidence of released compute on a remote host.
Imatrix calibration and task evaluation must use distinct corpora; shared genome
gains need independent reproduction and lineage through the existing forge path.

This change establishes trustworthy inference receipts and reply matching. It
does not claim a measured cross-grid speedup, distributed expert execution, or
automatic imatrix placement from the wire tests alone.
