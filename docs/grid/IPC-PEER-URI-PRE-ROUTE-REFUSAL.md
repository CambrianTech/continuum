# Peer URIs over IPC — the pre-route refusal, identified (card e710ed0f / 1b232595)

**Verdict:** the reported symptom — `airc://<peer>/ping` answered "Unknown command … did you mean: ping" — **cannot reproduce on this tree**. It was fixed by #4338 (`04437f2c94`, an ancestor of HEAD). What remains is a *design fact*, not a bug: the one pre-route ACL gate that sees the raw wire string treats peer URIs as unclassified commands and refuses them fail-closed for every non-Owner remote caller. That default is load-bearing, so it is now pinned by a regression test (`acl.rs`).

## Where the refusal actually lives

The card names `core_ipc_transport.rs`. No such file exists in this repo — IPC dispatch is:

| Path | File |
|---|---|
| Server dispatch loop (the one that used to say "Unknown command") | `core/continuum-core/src/ipc/mod.rs` |
| Client-side transport (CLI → socket) | `client/continuum-client/src/core_ipc.rs` |
| Remote grid-node accept loop (peer-to-peer, not IPC sockets) | `core/continuum-core/src/modules/grid/connection.rs` |

### The original bug (fixed, #4338)

Pre-#4338 the socket handler handed the wire name straight to `Runtime::route_command`, which is **name-based** — it never built a `CommandUri`. So the URI grammar (`airc://[peer[@node]][:env]/path`, `airc://room:<uuid>/path`) was reachable only by in-process callers. A peer URI over the local socket fell through every module, and the dispatch loop answered with its own discovery sentence:

- "Unknown command: '{cmd}'. {hint}" — `ipc/mod.rs` ~637–639 (the `route_command → None` arm).
- #4338's fix: `ServerState` now holds the one `Arc<CommandExecutor>`; `addressed_elsewhere(cmd)` (`ipc/mod.rs:176`) parses the name as a `CommandUri` and sends `Peer`/`Room` forms to `execute_with_caller`, which owns #3690's remote transport. Bare names, explicit-local spellings, and unparseable strings stay on the name route byte-for-byte.
- Pinned by test: `ipc::tests::a_peer_or_room_address_leaves_the_name_route_and_a_bare_name_stays` (`ipc/mod.rs` ~715–743).

## What remains: the remote-TCP pre-route gate

The dispatch loop has a boundary gate **before** routing, for callers with an identity (`caller != None`) — `ipc/mod.rs` ~539–551:

```rust
if let Some(ref c) = caller {
    let trust = crate::routing::caller_trust(Some(c));
    if !crate::modules::grid::acl::is_command_authorized(cmd, trust) { /* "forbidden …" */ }
}
```

It gates on the **raw wire string** — including peer URIs. The ACL resolves tier by prefix rules → declared `AccessLevel` sets → default-deny wildcard (`acl.rs`, `command_access_level`). A raw `airc://…` string matches no rule prefix and names no registered command, so it lands on the **Owner** tier. Consequences:

1. **Local callers never see it** — `caller == None` skips the gate entirely; the URI is parsed, not ACL'd as a name. (This is why #4338's fix works for the CLI.)
2. **Remote TCP/WS/observer callers can never run peer URIs.** `caller_trust` caps unauthenticated sockets at `Provisional` (`grid_trust_policy.rs`, `UNAUTHENTICATED_SOCKET_CEILING`; `CallerSource::Tcp | Ws | PositronObserver → Provisional`). Owner-tier refusal is therefore permanent until socket auth lands — a fail-closed default, not an accident.
3. **No legitimate in-repo path exercises it with URIs.** Grid peer-to-peer traffic (`grid/connection.rs`, `execute_incoming_request`) sends **bare names** and checks the same `is_command_authorized` against the node-registry trust (unknown source → `Blocked`). Nothing ships a URI string through that gate today.

### Why the default is load-bearing — not removable by "just parse URIs"

`CommandExecutor::execute_with_caller` performs **no per-command ACL of its own**: zero occurrences of `is_command_authorized` / `required_trust` / `command_access_level` in `core/continuum-core/src/runtime/command_executor.rs`. The pre-route gate above is the *only* authorization point a remote TCP caller ever passes. So if a URI string ever became Provisional/Trusted-reachable — an accidental `airc://` prefix rule, or `command_access_level` learning to parse URIs and tiering them by their verb — an unelevated remote peer could route **arbitrary verbs** through the executor's transport with no per-command check. The fail-closed Owner default is what makes that a non-event today; it must stay, which is why the fix for this card is a *pin*, not a change:

- `grid::acl::tests::raw_peer_and_room_uri_strings_are_owner_tier_default_deny` (this PR): asserts raw peer- and room-addressed URI strings authorize at Owner and **not** at Trusted or Provisional. If the default ever flips, CI trips before the hole opens.

## Disposition of the card's options ("fix or write up")

- **Fix:** nothing to fix on this tree — the local path works (pinned), and the remote behavior is fail-closed by design. Changing it to parse URIs in the gate would *remove* the only ACL a remote caller passes; that is the wrong direction, not a bug fix.
- **Write up:** this document + the regression test above.

## Open follow-up (out of scope here)

When socket auth lands (`CallerSource::Tcp` stops being `Provisional`), the gate should tier peer URIs by their *target verb* for authenticated callers — which requires per-command ACL at the executor, i.e. moving a check from "the string on the wire" to "the command that will actually run". That is a capability-discovery / socket-auth task, not this card's.
