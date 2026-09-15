# Cross-grid inference: command and evidence contract

The same `ai/generate` operation serves a local caller, a persona's remote lane,
and an explicit `aircPeer` command dispatch. Placement belongs to the substrate;
activities and benchmark recipes must not grow a second inference runner.

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

## Remaining distributed-compute gates

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
