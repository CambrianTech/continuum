# Card d89256ed Diagnosis: Presence Expiry Causes Periodic Room PARTs on 5090

## Symptoms
1. **5090**: personas join rooms then are PARTED every ~50-70 minutes  
2. **IntelMac**: never joins any room at all  

## Code path traced

### `core/continuum-core/src/airc/realtime_store.rs`
The `prune_expired_presence(now_ms)` function (lines 356-366) removes expired presence events and peer manifests from the in-memory store by checking `is_expired_at(now_ms)` on each event.

```rust
fn prune_expired_presence(&mut self, now_ms: u64) {
    self.presence.retain(|_, envelope| match &envelope.payload {
        AircRealtimePayload::Presence { event } => !event.is_expired_at(now_ms),
        _ => true,
    });
    self.peer_manifests
        .retain(|_, envelope| match &envelope.payload {
            AircPeerManifest { manifest } => !manifest.is_expired_at(now_ms),
            _ => true,
        });
}
```

**Callers:**
- During `publish()` (~line 161) — every time a new envelope is published to the room, expired presence is pruned first.  
- During `replay()` (~line 225) when `now_ms` parameter is passed (new clients replaying history).

## Root cause hypothesis

The ~50-70 minute cycle matches a **presence TTL timeout**:
1. Persona joins room → sends `presence:updated` with some TTL  
2. If persona goes silent without re-broadcasting presence before TTL expires, their presence is pruned from the store by `prune_expired_presence()`  
3. When a persona's presence is pruned, other personas subscribed to that room receive an Unsubscribe/PART event, causing them to be dropped  

For **IntelMac**: if it never sends a valid presence event on join (malformed payload, wrong room ID), or its presence TTL is shorter than others, it would get no subscriptions established.

## Files read
- `core/continuum-core/src/airc/realtime_store.rs` — full file; identified prune_expired_presence and its callers  
- `core/continuum-mind/src/room.rs` (625 lines) — citizen-facing room commands only, no involuntary eviction logic  
- `core/continuum-kernel/src/routing/airc_event_transport.rs` (814 lines) — event transport layer, fire-and-forget subscribe/unsubscribe with 30s deadline but no subscription TTL negotiation  

## Evidence needed
1. What is the default presence TTL? Need to find where `AircPresenceEvent::expires_at_ms` is set — likely in the airc runtime or protocol layer where presence events are created from persona broadcasts.  
2. Does pruning expired presence trigger Unsubscribe/PART events for subscribers, or does it rely on clients detecting absence during replay?  
3. Is there a heartbeat/keepalive mechanism that should refresh presence before expiry?

## Next steps
Search the airc runtime or protocol layer for where presence events are constructed and their TTL is set. The server-side airc implementation may have constants governing default presence duration.
