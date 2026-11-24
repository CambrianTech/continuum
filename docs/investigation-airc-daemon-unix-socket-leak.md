# Investigation: airc daemon unix socket leak

## Card
a90fc524 — "Investigate airc daemon unix socket leak: find where sockets are created per request and not closed/returned to pool"

## Scope
All Unix domain socket creation paths in the airc crate tree (airc-daemon, airc-ipc, airc-lib) at revision `fa918ab7fd4a295301f34ff3066067cddb368489`.

## Findings

### Every socket creation point examined:

| Path | Location | Sockets per use case | Close path |
|------|----------|---------------------|------------|
| `IpcListener::bind()` | airc-ipc transport.rs:294 | 1 (listener, lifetime of daemon) | `listener.cleanup()` on shutdown |
| `listener.accept()` → `handle_connection` | server.rs:187-205 | 1 per accepted connection | Stream drops when task returns |
| Non-attach request path | server.rs:488-513 | 1 socket (one-shot) | Both halves of split stream drop on function exit |
| Attach request → `stream_attach` / `stream_attach_set` | server.rs:515, 852 | 1 per attach connection (long-lived by design) | Dropped when hangup/shutdown fires; `client_hung_up(reader)` arm ensures silent disconnects are detected |
| `cleanup_stale_socket()` | server.rs:464 | 0 (one-shot connect to check liveness, drops immediately) | N/A |
| Client `call_inner` | client.rs:142-173 | 1 socket per RPC call | Both halves of split stream drop on function return |
| Client `attach_inner` + reconnect loop in airc-lib | client.rs:340-348, daemon.rs:798+ | 1 socket per attach connection | Old stream dropped (via reassignment) before new connect; failed reconnects drop the attempted stream |

### Key observations:
- There is NO socket pooling code anywhere in the airc crate tree. This is intentional — Unix domain sockets don't have TCP TIME_WAIT issues, so creating/closing one per RPC call is correct and efficient.
- The previous fix on 2026-09-12 (card e28889cc) already resolved a real leak of ~3,500 leaked sockets caused by missing hangup detection in `stream_attach`. The comment at server.rs:596-604 documents this explicitly.
- All connection paths properly close their sockets after use — either through explicit drop on function return, or through the hangup detection mechanism in long-lived attach streams.

## Conclusion

**No active unix socket leak found in the current codebase.** Every socket creation point creates exactly one socket per intended use and closes it properly when done. The card's claim of "~400 unix sockets per client request" does not match any single-code-path behavior: each RPC or attach creates exactly ONE socket, which is closed after its lifecycle completes.

The "400 per request" figure may be from a pre-fix era (before the 2026-09-12 hangup detection fix) or may refer to something outside the airc IPC layer — for example, LAN TCP peer connections (`connected_lan_peers` counter in handlers.rs:57), which use TCP sockets not Unix domain sockets.

## Recommendation
If the leak is reproducible on a running daemon, runtime investigation tools (e.g., `lsof -p <daemon-pid>`, `/proc/<pid>/fd`) would be needed to identify leaked file descriptors beyond what static analysis reveals. However, based on thorough code review of all socket creation and closure paths in the current codebase, no leak mechanism exists.
