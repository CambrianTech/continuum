# Grid Event Streaming — Real-Time Events Between Nodes

**Issue**: [#856](https://github.com/CambrianTech/continuum/issues/856)
**Status**: Design
**Priority**: CRITICAL — blocks open-eyes, factory live updates, OpenClaw, Hermes

---

## Problem

The grid currently uses request/response polling for inter-node communication. The Factory widget polls `grid/job-queue` every 10 seconds. That's fundamentally broken for:

| Application | Latency need | Current | Required |
|---|---|---|---|
| **open-eyes** | <500ms (security alert) | 10s poll | Real-time push |
| **Factory** | <1s (forge progress) | 10s poll | Real-time push |
| **OpenClaw** | <2s (search results) | N/A | Streaming |
| **Hermes** | <500ms (agent decisions) | N/A | Real-time push |

## Solution

Persistent WebSocket event channels between grid nodes. Events emit on one node → propagate to all connected nodes instantly. Same `Events.subscribe()` API — consumers don't know if events are local or remote.

---

## Architecture

```
Node A (BigMama)                    Node B (MacBook)
┌──────────────────┐               ┌──────────────────┐
│ Factory daemon   │               │ Factory widget    │
│ Events.emit(     │               │ Events.subscribe( │
│   'forge:step',  │               │   'forge:step',   │
│   {ppl: 7.81}   │               │   (d) => render() │
│ )                │               │ )                 │
│       │          │               │       ↑           │
│       ▼          │               │       │           │
│ GridEventRouter  │               │ GridEventRouter   │
│       │          │               │       ↑           │
└───────┼──────────┘               └───────┼───────────┘
        │                                  │
        └──── WebSocket (persistent) ──────┘
              Tailscale-encrypted
              Auto-reconnecting
```

### What Changes

| Layer | Current | New |
|---|---|---|
| `Events.emit()` | Local only | Local + broadcast to connected peers |
| `Events.subscribe()` | Local only | Receives local + remote events |
| Grid transport | Commands only (req/res) | Commands + event stream |
| Node connection | On-demand per command | Persistent WebSocket |

### What Stays The Same

- `Events.subscribe()` API — zero changes for consumers
- Event topic format — `domain:resource:action`
- Command routing — `GridInterceptor` unchanged
- Tailscale transport — same mesh, same encryption

---

## The GridEventRouter

New module in `system/grid/`. Each node runs one. It:

1. **Connects** to discovered peer nodes via WebSocket
2. **Subscribes** to local events that match remote interest
3. **Forwards** matching events to connected peers
4. **Receives** remote events and re-emits them locally
5. **Deduplicates** — events that originated locally aren't re-emitted
6. **Reconnects** automatically on connection loss

```typescript
// system/grid/shared/GridEventRouter.ts

class GridEventRouter {
    private peers: Map<string, WebSocket> = new Map();
    private localOrigins: Set<string> = new Set(); // dedup

    // Called by Events system when any local event fires
    onLocalEvent(topic: string, payload: unknown, eventId: string): void {
        this.localOrigins.add(eventId);
        
        // Broadcast to all connected peers
        const message = {
            type: 'event',
            topic,
            payload,
            eventId,
            sourceNode: this.nodeId,
            timestamp: Date.now(),
        };
        
        for (const [peerId, ws] of this.peers) {
            if (ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify(message));
            }
        }
    }

    // Called when a peer sends us an event
    onRemoteEvent(message: GridEventMessage): void {
        // Dedup: don't re-emit events we originated
        if (this.localOrigins.has(message.eventId)) return;
        
        // Re-emit locally so subscribers see it
        Events.emit(message.topic, {
            ...message.payload,
            _remoteNode: message.sourceNode,
            _eventId: message.eventId,
        });
    }
}
```

### Event ID Generation

Every `Events.emit()` call generates a unique event ID (UUID or monotonic counter + node ID). This ID:
- Prevents dedup loops (event echoing back to origin)
- Enables exactly-once delivery
- Links to attestation chain (the event ID IS the Merkle leaf)

### Connection Management

```typescript
class GridPeerConnection {
    private ws: WebSocket | null = null;
    private reconnectDelay = 1000;
    private maxReconnectDelay = 30000;

    connect(url: string): void {
        this.ws = new WebSocket(url);
        
        this.ws.onopen = () => {
            this.reconnectDelay = 1000; // reset backoff
            this.sendSubscriptions();   // tell peer what events we want
        };
        
        this.ws.onmessage = (msg) => {
            const data = JSON.parse(msg.data);
            if (data.type === 'event') {
                this.router.onRemoteEvent(data);
            }
        };
        
        this.ws.onclose = () => {
            // Exponential backoff reconnection
            setTimeout(() => this.connect(url), this.reconnectDelay);
            this.reconnectDelay = Math.min(
                this.reconnectDelay * 2,
                this.maxReconnectDelay
            );
        };
    }
}
```

---

## Topic-Based Filtering

Nodes don't forward ALL events — only topics that remote peers have subscribers for. This prevents flooding the network with events nobody cares about.

```typescript
// When a new subscription is created locally:
Events.subscribe('camera:motion:detected', handler);
// → GridEventRouter sends to all peers:
//   { type: 'subscribe', topic: 'camera:motion:detected' }

// Peer adds to its forward filter:
//   "Node B wants camera:motion:detected events"

// When the event fires locally on the peer:
Events.emit('camera:motion:detected', data);
// → GridEventRouter checks forward filter
// → Node B wants this → forward
// → Node C doesn't → skip
```

This is the same model as MQTT topic subscriptions. Subscribe tells the source what to send. Unsubscribe removes the filter. Events only cross the wire if someone wants them.

### Wildcard Subscriptions

The existing Events system supports wildcards (`camera:*`, `data:*:created`). These propagate to peers:

```typescript
Events.subscribe('camera:*', handler);
// → tells all peers: forward any event matching camera:*
```

---

## Transport Layers — Two Tiers

Not all events are equal. Forge completion is different from 30fps optical flow vectors. Two tiers, same mesh:

### Tier 1: TCP/WebSocket (reliable, ordered)

For events where loss is unacceptable:
- Forge step/phase/complete
- Entity entered/left zone
- Threat assessments
- Camera connected/disconnected
- Battery critical alerts
- Coverage gap notifications

```
Local network:    ws://peer-ip:9002/grid-events
Tailscale mesh:   wss://peer-hostname:9002/grid-events  (TLS via Tailscale)
Reticulum:        custom framing over Reticulum link    (future)
```

Latency: ~50ms acceptable. Delivery: guaranteed. Ordered.

### Tier 2: UDP (lossy, fast)

For high-frequency sensor data where freshness beats completeness:
- Optical flow vectors (30fps per camera)
- Audio levels / spectrograms
- GPU utilization telemetry
- Live camera frame metadata (not the frames themselves — those go via RTSP/WebRTC)
- Real-time entity positions (smooth interpolation on receiver)

```
Local network:    udp://peer-ip:9003
Tailscale mesh:   udp://peer-tailscale-ip:9003  (WireGuard-encrypted)
```

Latency: <10ms target. Delivery: best-effort. Unordered. Drop stale packets.

**This is the same split as WebRTC**: signaling over TCP, media over UDP. And it's why live video broke in Docker — Docker's bridge network mangles UDP. Tailscale bypasses Docker networking (sidecar has its own network namespace), so UDP between Tailscale nodes just works.

### Why Both

| Data | TCP overhead | UDP packet loss | Right choice |
|---|---|---|---|
| "Person entered zone" | Fine (50ms) | Unacceptable (missed alert) | **TCP** |
| "Flow vector [0.3, 0.1] at camera 2" | Too slow (retransmit jitter) | Fine (next vector in 33ms) | **UDP** |
| "Forge PPL = 8.42" | Fine | Unacceptable | **TCP** |
| "GPU temp 72C" | Fine but wasteful | Fine | **UDP** |

Port 9002 for TCP event streaming. Port 9003 for UDP sensor data. Both separate from the main WebSocket (9001) that carries browser ↔ server traffic.

---

## Rust Integration

For Rust-based nodes (open-eyes cameras, forge workers), the event channel uses the same IPC socket that continuum-core already provides. The Rust side emits events to the IPC socket, the Node.js GridEventRouter picks them up and forwards to peers.

```
Rust process (open-eyes, forge-worker)
    │
    ├─ IPC socket: /root/.continuum/sockets/continuum-core.sock
    │   └─ Event: { topic: "camera:motion:detected", payload: {...} }
    │
    ▼
Node.js GridEventRouter
    │
    ├─ Re-emits locally: Events.emit('camera:motion:detected', ...)
    ├─ Forwards to peers: ws.send({type: 'event', topic: ..., payload: ...})
    │
    ▼
Remote Node.js (MacBook)
    │
    ├─ Events.emit('camera:motion:detected', ...) — local re-emit
    ├─ Factory widget / Persona inbox receives it
```

No new protocol for Rust. Same IPC socket, same event format. The GridEventRouter is the bridge from IPC to WebSocket to remote nodes.

---

## Backpressure

If a peer can't keep up with events (slow network, overloaded node):

1. **WebSocket buffer fills** — `ws.bufferedAmount` grows
2. **Router detects** — checks `bufferedAmount` before each send
3. **Drops low-priority events** — heartbeats, telemetry
4. **Keeps high-priority events** — motion alerts, threat assessments, forge errors
5. **Logs** — "dropped N events for peer X (backpressure)"

Events have a priority field (derived from topic):

```typescript
const PRIORITY: Record<string, number> = {
    'camera:threat:assessed': 10,    // never drop
    'camera:motion:detected': 8,
    'forge:complete': 8,
    'forge:error': 10,               // never drop
    'forge:step': 3,                 // drop under pressure
    'camera:heartbeat': 1,           // first to drop
};
```

---

## Implementation Plan

### Phase 1: Local Event Bridge (smallest useful step)
Wire GridEventRouter into the existing Events system. Events emitted locally check if any peer connections exist and forward if so. No new transport — uses existing WebSocket infrastructure between browser and server.

**Result**: Events on the server reach the browser without polling. Factory widget gets live updates.

### Phase 2: Peer-to-Peer WebSocket
Add `ws://peer:9002/grid-events` endpoint. Nodes connect to discovered peers. Subscription-based forwarding. Auto-reconnect.

**Result**: BigMama's forge events stream to your MacBook in real time. open-eyes camera events flow between nodes.

### Phase 3: Topic Filtering + Backpressure
Subscription-based forwarding (only send what peers subscribed to). Priority-based dropping under backpressure.

**Result**: Efficient mesh — events only cross the wire if someone wants them.

### Phase 4: Reticulum Transport
Add Reticulum as an event transport alongside WebSocket. For LoRa-connected cameras that can't do WebSocket.

**Result**: Solar cameras on a farm 2 miles away send events over radio mesh.

---

## What This Enables

Once Phase 2 is complete:

| Application | Before | After |
|---|---|---|
| **Factory widget** | Polls every 10s, misses progress | Live forge step/phase/complete events |
| **open-eyes** | Can't deliver camera events to personas | <500ms motion events → persona inbox |
| **OpenClaw** | N/A | Streaming search results from grid nodes |
| **Hermes** | N/A | Agent decisions propagate across mesh instantly |
| **Persona reasoning** | Local events only | Personas react to events from any node |

## Files to Create/Modify

```
NEW:
  system/grid/shared/GridEventRouter.ts       (~200 lines)
  system/grid/server/GridEventServer.ts       (~100 lines — WS endpoint)
  system/grid/shared/GridEventTypes.ts        (~50 lines)

MODIFY:
  system/core/shared/Events.ts                (add eventId, hook for router)
  system/grid/server/GridInterceptor.ts       (register router at startup)
  server/docker-entrypoint.ts                 (start event server)
  docker-compose.yml                          (expose port 9002)
```

Estimated: ~400 lines of new code. The Events system and grid transport already exist — this wires them together.

---

## See Also

- [CONTINUUM-INTEGRATION.md](../../open-eyes/docs/CONTINUUM-INTEGRATION.md) — open-eyes event types
- [PROGRESSIVE-ATTESTATION.md](../../sentinel-ai/docs/PROGRESSIVE-ATTESTATION.md) — forge events
- [FactoryWidget.ts](../../src/widgets/factory/FactoryWidget.ts) — already subscribes to forge events
- [Events.ts](../../src/system/core/shared/Events.ts) — the event system being extended
