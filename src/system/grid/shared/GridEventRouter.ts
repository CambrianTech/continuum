/**
 * GridEventRouter — forwards events between grid nodes.
 *
 * Hooks into Events.emit() to transparently forward local events
 * to connected peer nodes. Receives remote events and re-emits
 * them locally. Subscribers don't know if events are local or remote.
 *
 * Architecture:
 *   Events.emit('camera:motion', data)
 *     → local dispatch (existing behavior, unchanged)
 *     → GridEventRouter.onLocalEvent()
 *       → forwards to all peers with matching subscriptions
 *
 *   Peer sends event via WebSocket
 *     → GridEventRouter.onRemoteEvent()
 *       → Events.emit() locally (with _remote flag to prevent echo)
 *
 * Transport: WebSocket for TCP tier, raw UDP for sensor tier.
 * Both encrypted via Tailscale between nodes.
 */

import { Events } from '../../core/shared/Events';

// ── Types ──────────────────────────────────────────────────────

export interface GridEventMessage {
  type: 'event';
  topic: string;
  payload: unknown;
  eventId: string;
  sourceNode: string;
  timestamp: number;
}

export interface GridSubscribeMessage {
  type: 'subscribe';
  topics: string[];
}

export interface GridUnsubscribeMessage {
  type: 'unsubscribe';
  topics: string[];
}

type GridMessage = GridEventMessage | GridSubscribeMessage | GridUnsubscribeMessage;

export interface PeerConnection {
  nodeId: string;
  ws: WebSocket;
  subscribedTopics: Set<string>;
  connected: boolean;
  lastSeen: number;
}

// ── Event priority (for backpressure) ──────────────────────────

const PRIORITY: Record<string, number> = {
  // Never drop (10)
  'camera:threat:assessed': 10,
  'forge:error': 10,
  'forge:complete': 10,
  'camera:battery:critical': 10,
  // High (8)
  'camera:motion:detected': 8,
  'camera:entity:entered': 8,
  'camera:entity:left': 8,
  'forge:phase': 8,
  // Medium (5)
  'camera:zone:crossing': 5,
  'camera:drift:detected': 5,
  'scene:updated': 5,
  // Low (3)
  'forge:step': 3,
  'camera:coverage:gap': 3,
  // Droppable (1)
  'camera:heartbeat': 1,
};

function eventPriority(topic: string): number {
  // Exact match first
  if (topic in PRIORITY) return PRIORITY[topic];
  // Prefix match (camera:* defaults to 5)
  for (const [pattern, pri] of Object.entries(PRIORITY)) {
    if (topic.startsWith(pattern.replace(':*', ':'))) return pri;
  }
  return 5; // default medium
}

// ── Router ─────────────────────────────────────────────────────

let _instance: GridEventRouter | null = null;

export class GridEventRouter {
  private peers: Map<string, PeerConnection> = new Map();
  private localOrigins: Set<string> = new Set();
  private nodeId: string;
  private eventCounter = 0;
  private maxBufferedBytes = 1024 * 1024; // 1MB backpressure threshold

  // Local topic subscriptions (what this node's consumers want)
  private localSubscriptions: Set<string> = new Set();

  constructor(nodeId: string) {
    this.nodeId = nodeId;
  }

  static instance(): GridEventRouter | null {
    return _instance;
  }

  static initialize(nodeId: string): GridEventRouter {
    _instance = new GridEventRouter(nodeId);
    return _instance;
  }

  // ── Peer Management ────────────────────────────────────────

  /**
   * Connect to a peer node via WebSocket.
   * Auto-reconnects on disconnect with exponential backoff.
   */
  connectPeer(nodeId: string, url: string): void {
    if (this.peers.has(nodeId)) return;

    const ws = new WebSocket(url);
    const peer: PeerConnection = {
      nodeId,
      ws,
      subscribedTopics: new Set(),
      connected: false,
      lastSeen: Date.now(),
    };

    ws.onopen = () => {
      peer.connected = true;
      peer.lastSeen = Date.now();
      console.log(`[GridEventRouter] Connected to peer ${nodeId}`);

      // Tell peer what topics we want
      const msg: GridSubscribeMessage = {
        type: 'subscribe',
        topics: [...this.localSubscriptions],
      };
      ws.send(JSON.stringify(msg));
    };

    ws.onmessage = (event) => {
      try {
        const msg: GridMessage = JSON.parse(event.data as string);
        this.handlePeerMessage(nodeId, msg);
      } catch (e) {
        console.warn(`[GridEventRouter] Bad message from ${nodeId}:`, e);
      }
    };

    ws.onclose = () => {
      peer.connected = false;
      console.log(`[GridEventRouter] Disconnected from peer ${nodeId}`);
      this.peers.delete(nodeId);
      // Reconnect after delay
      setTimeout(() => this.connectPeer(nodeId, url), 5000);
    };

    ws.onerror = (err) => {
      console.warn(`[GridEventRouter] Error with peer ${nodeId}:`, err);
    };

    this.peers.set(nodeId, peer);
  }

  disconnectPeer(nodeId: string): void {
    const peer = this.peers.get(nodeId);
    if (peer) {
      peer.ws.close();
      this.peers.delete(nodeId);
    }
  }

  // ── Event Forwarding (local → remote) ──────────────────────

  /**
   * Called when a local event fires. Forwards to peers that subscribed.
   * This is the hook that Events.emit() calls.
   */
  onLocalEvent(topic: string, payload: unknown): void {
    const eventId = `${this.nodeId}-${++this.eventCounter}`;
    this.localOrigins.add(eventId);

    // Clean old origins (prevent memory leak)
    if (this.localOrigins.size > 10000) {
      const arr = [...this.localOrigins];
      this.localOrigins = new Set(arr.slice(-5000));
    }

    const message: GridEventMessage = {
      type: 'event',
      topic,
      payload,
      eventId,
      sourceNode: this.nodeId,
      timestamp: Date.now(),
    };

    const json = JSON.stringify(message);
    const priority = eventPriority(topic);

    for (const [peerId, peer] of this.peers) {
      if (!peer.connected) continue;

      // Check if peer subscribed to this topic
      if (!this.peerWantsTopic(peer, topic)) continue;

      // Backpressure: check WebSocket buffer
      const ws = peer.ws as any;
      const buffered = ws.bufferedAmount ?? 0;
      if (buffered > this.maxBufferedBytes && priority < 8) {
        // Drop low-priority events under pressure
        continue;
      }

      peer.ws.send(json);
    }
  }

  /**
   * Register a local subscription. Tells all peers we want this topic.
   * Called by Events.subscribe() hook.
   */
  onLocalSubscribe(topic: string): void {
    if (this.localSubscriptions.has(topic)) return;
    this.localSubscriptions.add(topic);

    // Tell all connected peers
    const msg: GridSubscribeMessage = {
      type: 'subscribe',
      topics: [topic],
    };
    const json = JSON.stringify(msg);

    for (const [, peer] of this.peers) {
      if (peer.connected) {
        peer.ws.send(json);
      }
    }
  }

  // ── Event Receiving (remote → local) ───────────────────────

  private handlePeerMessage(peerId: string, msg: GridMessage): void {
    switch (msg.type) {
      case 'event':
        this.onRemoteEvent(msg);
        break;
      case 'subscribe':
        this.onPeerSubscribe(peerId, msg.topics);
        break;
      case 'unsubscribe':
        this.onPeerUnsubscribe(peerId, msg.topics);
        break;
    }
  }

  /**
   * Receive an event from a peer. Re-emit locally so subscribers see it.
   */
  private onRemoteEvent(msg: GridEventMessage): void {
    // Dedup: don't re-emit events we originated
    if (this.localOrigins.has(msg.eventId)) return;

    // Re-emit locally with remote metadata
    Events.emit(msg.topic, {
      ...(msg.payload as Record<string, unknown>),
      _remoteNode: msg.sourceNode,
      _eventId: msg.eventId,
      _remoteTimestamp: msg.timestamp,
    });
  }

  private onPeerSubscribe(peerId: string, topics: string[]): void {
    const peer = this.peers.get(peerId);
    if (!peer) return;
    for (const topic of topics) {
      peer.subscribedTopics.add(topic);
    }
  }

  private onPeerUnsubscribe(peerId: string, topics: string[]): void {
    const peer = this.peers.get(peerId);
    if (!peer) return;
    for (const topic of topics) {
      peer.subscribedTopics.delete(topic);
    }
  }

  // ── Topic Matching ─────────────────────────────────────────

  private peerWantsTopic(peer: PeerConnection, topic: string): boolean {
    // Exact match
    if (peer.subscribedTopics.has(topic)) return true;

    // Wildcard match: 'camera:*' matches 'camera:motion:detected'
    for (const sub of peer.subscribedTopics) {
      if (sub.endsWith(':*')) {
        const prefix = sub.slice(0, -1); // 'camera:'
        if (topic.startsWith(prefix)) return true;
      }
      if (sub === '*') return true;
    }

    return false;
  }

  // ── Status ─────────────────────────────────────────────────

  connectedPeers(): string[] {
    return [...this.peers.entries()]
      .filter(([, p]) => p.connected)
      .map(([id]) => id);
  }

  peerCount(): number {
    return this.peers.size;
  }

  connectedCount(): number {
    return [...this.peers.values()].filter(p => p.connected).length;
  }
}
