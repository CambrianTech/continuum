# P2P Mesh Architecture - Decentralized Continuum Network

**Status**: Design Phase (Phase 4)
**Created**: 2025-11-23
**Vision**: Unstoppable decentralized network for AI ecosystem, social media, marketplace, and economic system

---

## Overview

Continuum's P2P mesh network enables **censorship-resistant discovery and exchange** of:
- **Genomic layers** (LoRA adapters, skills)
- **Personas** (AI agents)
- **Content** (messages, media, artifacts)
- **Services** (compute, storage, inference)
- **Value** (alt coin rewards for contributions)

**Key Insight**: "We care little for extreme data integrity. We just want the best match that is available now, and likely nearby."

This is NOT a blockchain (no consensus needed). This IS a gossip network with semantic search.

---

## Core Problems Solved

### 1. Discovery Without Central Authority
- **Problem**: How do peers find genomic layers without a central registry?
- **Solution**: Gossip protocol + bounded flood + semantic vector search
- **Result**: Each peer maintains local cache of 10,000+ remote adapters

### 2. Bootstrap Without Seed Nodes
- **Problem**: How do new peers join without hardcoded servers?
- **Solution**: Multi-strategy bootstrap (mDNS, cached peers, public announcements, peer exchange)
- **Result**: Network survives even if all bootstrap nodes die

### 3. Patience = Accuracy Tradeoff
- **Problem**: Exhaustive search across 10,000 peers is slow
- **Solution**: Progressive search refinement - user controls timeout
- **Result**: Fast approximate results (1s) or exhaustive search (30s) - user decides

### 4. Adversarial Resilience
- **Problem**: Powerful actors may try to shut down network
- **Solution**: No single point of failure, content-addressed storage, plausible deniability
- **Result**: Takedown one node? 999 others still work

---

## Semantic Search: The Key Innovation

### Why This Beats BitTorrent

**BitTorrent**: Exact hash match or keyword search
- Search for "bartending.torrent"
- Get exact file if it exists
- Rigid naming, no understanding of meaning

**Continuum Mesh**: Semantic similarity via vector embeddings
- Describe what you're building: "I need wine fermentation knowledge for vineyard management"
- Get best semantic matches even if differently named
- Understands INTENT, not just keywords

### The Breakthrough: Intent-Based Discovery

**Traditional keyword search** (BitTorrent, file systems):
```bash
# You search: "fermentation"
# Results:
# - brewing-beer-2023.lora (wrong domain)
# - kombucha-fermentation.lora (wrong domain)
# - wine-basics.lora (too general)

# You must know exact terminology to find what you need
```

**Semantic search** (Continuum):
```bash
# You describe what you're building:
./jtag genome/search "I'm building a vineyard management AI that needs to understand grape cultivation, fermentation chemistry, and wine production" --network

# Mesh finds SEMANTICALLY SIMILAR adapters:
# 1. viticulture-expert-v3 (0.91 similarity)
#    ├─ Specializations: grape-growing, pest-management, harvest-timing
#    └─ "Trained on 10 years of vineyard data"
#
# 2. fermentation-chemistry-v2 (0.87 similarity)
#    ├─ Specializations: wine-fermentation, yeast-biology, pH-control
#    └─ "Molecular understanding of fermentation"
#
# 3. agricultural-crop-optimization (0.84 similarity)
#    ├─ Specializations: soil-analysis, irrigation, crop-yield
#    └─ "Perennial crop management"
#
# 4. botany-advanced (0.79 similarity)
#    ├─ Specializations: plant-physiology, growth-cycles
#    └─ "Deep plant biology knowledge"

# You didn't need to know the term "viticulture" - embeddings understood your INTENT
```

### How Embeddings Flow Through the Network

**Every layer uses semantic vector search** (not keyword matching):

#### 1. Gossip Layer (Phase 1)
```typescript
// Peers exchange catalog metadata INCLUDING embeddings
export interface CatalogGossipMessage {
  adapters: Array<{
    id: UUID;
    name: string;
    description: string;
    embedding: number[];  // ← 384-dim semantic vector!
    capabilities: string[];
    specializations: string[];
  }>;
}

// After 10 minutes, your local cache has 10,000 adapter embeddings
// Search is LOCAL and SEMANTIC (no network I/O)
```

#### 2. Bounded Flood Layer (Phase 2)
```typescript
// Query carries embedding, not keywords
export interface MeshSearchRequest {
  query: {
    embedding: number[];  // ← Your intent as vector
    keywords: string[];   // ← Optional hybrid search
  };
  ttl: number;
}

// Each peer runs VECTOR SEARCH on their genome
// Results ranked by COSINE SIMILARITY (semantic meaning)
```

#### 3. DHT Layer (Phase 3)
```typescript
// Even DHT routing uses semantic hints
// Forward to peers with high specialization overlap
// Not just XOR distance - semantic distance matters
```

### The "Building vs. Searching" Paradigm Shift

**You're not searching for an adapter name** - you're searching for **"what will help me build X"**:

| What You're Building | Traditional Search (Fails) | Semantic Search (Works) |
|---------------------|---------------------------|------------------------|
| **Vineyard management bot** | Search "fermentation" → beer brewing ❌ | "Grape cultivation + wine chemistry" → viticulture ✓ |
| **Cocktail recommendation AI** | Search "drinks" → soft drinks ❌ | "Mixology techniques + flavor profiles" → bartending ✓ |
| **Async debugging assistant** | Search "debugging" → wrong domain ❌ | "Race conditions + concurrency" → async experts ✓ |
| **Vine Diesel (pun bot)** | Search "vines" → climbing plants ❌ | "Viticulture + bartending + agriculture" → closest matches ✓ |

### Performance: Fast Enough to Be Practical

**Embeddings are compact and searchable**:

```
Embedding size: 384 floats × 4 bytes = 1.5KB per adapter
Gossip bandwidth: 100 adapters × 1.5KB = 150KB per round (every 30s)
  ↓
Total bandwidth: 5KB/second (negligible!)

Local cache: 10,000 adapters × 1.5KB = 15MB (trivial!)

Vector search: 10,000 embeddings in 10-50ms (cosine similarity is fast)
  ↓
Cache hit rate: 80% after 10 minutes online
  ↓
Most searches NEVER touch the network (local vector search only)
```

### The Killer Combo

| Component | What It Does | Performance |
|-----------|-------------|-------------|
| **Embeddings** (384-dim) | Capture semantic meaning of adapters | 1.5KB per adapter |
| **Gossip** (30s intervals) | Distribute embeddings passively | 5KB/s bandwidth |
| **Local cache** (10K adapters) | Index all gossiped embeddings | 15MB storage |
| **Vector search** (cosine similarity) | Find best semantic matches | 10-50ms latency |
| **Bounded flood** (TTL-based) | Fill cache misses | 1-5s for 20% of queries |

**Result**: Intent-based discovery that feels instant while understanding what you're trying to build.

### Why This Changes Everything

1. **No need to know terminology**: "Async debugging" finds concurrency experts even if not explicitly labeled
2. **Cross-domain discovery**: Building vineyard bot finds fermentation + agriculture + botany adapters
3. **Multilingual**: Embeddings work across languages (describe in Spanish, find English adapters)
4. **Approximate is fine**: "Good enough now" matches can be fine-tuned later
5. **User controls accuracy**: Search 1s (fast approximate) or 30s (exhaustive) - patience = accuracy

**Bottom line**: Continuum mesh isn't just a file-sharing network - it's a **semantic skill discovery network** for AI agents. That's the revolution beyond BitTorrent.

---

## Architecture Layers

```
┌─────────────────────────────────────────────┐
│  Application Layer                          │
│  (Genome Marketplace, Social Media, etc)    │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│  Semantic Search Layer                      │
│  (Vector embeddings, hybrid search)         │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│  Discovery Layer (THIS DOC)                 │
│  • Gossip Protocol (catalog sync)           │
│  • Bounded Flood (multi-hop search)         │
│  • DHT (Kademlia routing)                   │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│  Transport Layer                            │
│  (WebRTC, WebSocket, TCP, encrypted)        │
└─────────────────────────────────────────────┘
```

---

## Phase 1: Gossip Protocol (Local + Direct Neighbors)

### Overview
Every 30 seconds, peers exchange catalogs with random neighbors. After 10 minutes online, each peer has a **cached index** of ~10,000 adapters from 100 peers.

**Effect**: Search is **local-first** (1-10ms) with **network fallback** only if cache misses.

### Implementation

```typescript
// mesh/GossipCatalogSync.ts

export interface CatalogGossipMessage {
  type: 'catalog-gossip';
  sender: PeerId;
  timestamp: number;
  adapters: Array<{
    id: UUID;
    contentHash: string;
    name: string;
    description: string;
    capabilities: string[];
    specializations: string[];
    embedding: number[];  // 384-dim vector
    metrics: {
      accuracy: number;
      downloads: number;
      avgRating: number;
    };
  }>;
}

export class GossipCatalogSync {
  private localCatalog: GenomicLayerCatalog;
  private remoteCache: Map<PeerId, CatalogEntry[]>;
  private gossipInterval: number = 30_000;  // 30s

  async start(): Promise<void> {
    setInterval(() => this.gossipRound(), this.gossipInterval);
  }

  /**
   * Gossip round: Exchange catalogs with 3 random neighbors
   */
  private async gossipRound(): Promise<void> {
    const neighbors = this.mesh.getRandomNeighbors(3);

    for (const neighbor of neighbors) {
      try {
        // Send our top 100 adapters (just metadata, not weights)
        const myTop100 = this.localCatalog.getTop(100, {
          sortBy: 'popularity',  // Or 'quality', 'recency'
        });

        const request: CatalogGossipMessage = {
          type: 'catalog-gossip',
          sender: this.mesh.myPeerId,
          timestamp: Date.now(),
          adapters: myTop100.map(a => this.serializeMetadata(a))
        };

        // Exchange: send ours, receive theirs
        const response = await this.mesh.sendAndWait<CatalogGossipMessage>(
          neighbor,
          request,
          { timeout: 5000 }
        );

        // Merge their catalog into our cache
        this.remoteCache.set(neighbor, response.adapters);

        // Index their embeddings locally (for fast vector search)
        await this.indexRemoteAdapters(neighbor, response.adapters);

        console.log(`📡 Gossip: Exchanged catalogs with ${neighbor} (${response.adapters.length} adapters)`);
      } catch (error) {
        console.warn(`⚠️ Gossip failed with ${neighbor}:`, error);
      }
    }
  }

  /**
   * Index remote adapters in local vector search DB
   */
  private async indexRemoteAdapters(
    peerId: PeerId,
    adapters: CatalogGossipMessage['adapters']
  ): Promise<void> {
    for (const adapter of adapters) {
      // Store in local DB with 'remote' flag
      await Commands.execute('data/index-vector', {
        collection: 'genomic_layers',
        id: adapter.id,
        embedding: adapter.embedding,
        metadata: {
          ...adapter,
          location: 'remote',
          availableFrom: [peerId],
          cachedAt: Date.now()
        }
      });
    }
  }
}
```

### Search Flow (Local-First)

```typescript
// User searches for "bartender skills"
const results = await Commands.execute('data/vector-search', {
  collection: 'genomic_layers',
  queryText: 'bartender skills',
  k: 10
});

// Results include:
// - Local adapters (location: 'local')
// - Cached remote adapters (location: 'remote', availableFrom: [peer1, peer2])
```

**Performance**:
- Local search: 1-10ms
- No network I/O needed for cached results
- Cache hit rate: ~80% after 10 minutes online

---

## Phase 2: Bounded Flood (Multi-Hop Search)

### Overview
When local cache misses or user wants exhaustive search, flood request through mesh with TTL limit.

**User controls accuracy via patience**: "Keep searching for 10 more seconds" → more hops → better results.

### Implementation

```typescript
// mesh/BoundedFloodSearch.ts

export interface MeshSearchRequest {
  requestId: UUID;
  sender: PeerId;          // Originator
  query: {
    embedding: number[];   // Query vector
    keywords: string[];    // For hybrid search
    filters: {
      minQuality?: number;
      domains?: string[];
      capabilities?: string[];
    };
  };
  ttl: number;             // Hops remaining (start at 10)
  responsePath: PeerId[];  // Breadcrumb trail back to origin
  timestamp: number;
}

export interface MeshSearchResponse {
  type: 'search-response';
  requestId: UUID;
  sender: PeerId;
  results: Array<{
    adapter: GenomicLayerMetadata;
    similarity: number;
  }>;
  hopsFromOrigin: number;
}

export class BoundedFloodSearch {
  private activeSearches: Map<UUID, SearchContext>;

  /**
   * Initiate mesh search with progressive timeout
   */
  async searchMesh(
    query: string,
    options: {
      timeout: number;       // User patience (1-30s)
      minResults: number;    // Stop early if N good results found
      maxHops: number;       // Limit network flood (1-10)
      minSimilarity: number; // Quality threshold (0.6-0.9)
    }
  ): Promise<Stream<SearchResult>> {
    const requestId = generateUUID();
    const queryEmbedding = await this.generateEmbedding(query);
    const resultsStream = new Stream<SearchResult>();

    // Create search context
    const ctx: SearchContext = {
      requestId,
      startTime: Date.now(),
      options,
      results: new PriorityQueue<SearchResult>('similarity', 'desc'),
      peersQueried: new Set<PeerId>(),
    };
    this.activeSearches.set(requestId, ctx);

    // Build request
    const request: MeshSearchRequest = {
      requestId,
      sender: this.mesh.myPeerId,
      query: {
        embedding: queryEmbedding,
        keywords: await this.extractKeywords(query),
        filters: {
          minQuality: options.minSimilarity,
        }
      },
      ttl: options.maxHops,
      responsePath: [],
      timestamp: Date.now()
    };

    // Broadcast to direct neighbors
    await this.broadcastRequest(request);

    // Collect responses as they arrive (progressive)
    this.mesh.on('search-response', (response: MeshSearchResponse) => {
      if (response.requestId === requestId) {
        this.handleSearchResponse(response, ctx, resultsStream);
      }
    });

    // Timeout handler
    setTimeout(() => {
      this.finalizeSearch(requestId, resultsStream);
    }, options.timeout);

    return resultsStream;
  }

  /**
   * Handle incoming search request from peer
   */
  async handleSearchRequest(request: MeshSearchRequest): Promise<void> {
    // 1. Search local genome
    const localResults = await Commands.execute('data/vector-search', {
      collection: 'genomic_layers',
      queryVector: request.query.embedding,
      k: 20,
      similarityThreshold: 0.6,
      filter: {
        'metrics.accuracy': { $gte: request.query.filters.minQuality || 0.7 }
      }
    });

    // 2. Send results back along response path
    const responseDest = request.responsePath.length > 0
      ? request.responsePath[request.responsePath.length - 1]
      : request.sender;

    const response: MeshSearchResponse = {
      type: 'search-response',
      requestId: request.requestId,
      sender: this.mesh.myPeerId,
      results: localResults.results.map(r => ({
        adapter: r.data as GenomicLayerMetadata,
        similarity: r.score
      })),
      hopsFromOrigin: request.responsePath.length
    };

    await this.mesh.sendTo(responseDest, response);

    // 3. Forward request to neighbors (if TTL remaining)
    if (request.ttl > 0) {
      const forwardTargets = this.selectForwardTargets(request);

      for (const neighbor of forwardTargets) {
        await this.mesh.sendTo(neighbor, {
          ...request,
          ttl: request.ttl - 1,
          responsePath: [...request.responsePath, this.mesh.myPeerId]
        });
      }
    }
  }

  /**
   * Smart forwarding: select neighbors likely to have relevant content
   */
  private selectForwardTargets(request: MeshSearchRequest): PeerId[] {
    // Strategy: Forward to neighbors with high specialization overlap
    const keywords = request.query.keywords;
    const neighbors = this.mesh.getConnectedPeers();

    // Score neighbors by keyword overlap with their known specializations
    const scored = neighbors.map(neighbor => {
      const specializations = this.remoteCache.get(neighbor)?.specializations || [];
      const overlap = keywords.filter(kw => specializations.includes(kw)).length;
      return { neighbor, score: overlap };
    });

    // Forward to top 3 most relevant neighbors
    return scored
      .sort((a, b) => b.score - a.score)
      .slice(0, 3)
      .map(s => s.neighbor);
  }

  /**
   * Handle incoming search response
   */
  private handleSearchResponse(
    response: MeshSearchResponse,
    ctx: SearchContext,
    stream: Stream<SearchResult>
  ): void {
    // Add results to priority queue
    for (const result of response.results) {
      ctx.results.push({
        ...result,
        hops: response.hopsFromOrigin,
        source: response.sender
      });
    }

    // Emit new results to stream (user sees them immediately)
    stream.emit('results', response.results);

    // Check if we've found enough good results to stop early
    if (ctx.results.size() >= ctx.options.minResults) {
      const best = ctx.results.peek();
      if (best && best.similarity > 0.9) {
        console.log(`✅ Found excellent match (${best.similarity}), stopping search early`);
        this.finalizeSearch(ctx.requestId, stream);
      }
    }
  }

  /**
   * Finalize search and close stream
   */
  private finalizeSearch(requestId: UUID, stream: Stream<SearchResult>): void {
    const ctx = this.activeSearches.get(requestId);
    if (!ctx) return;

    const elapsed = Date.now() - ctx.startTime;
    const totalResults = ctx.results.size();

    console.log(`🔍 Search complete: ${totalResults} results in ${elapsed}ms`);

    stream.emit('complete', {
      totalResults,
      elapsed,
      peersQueried: ctx.peersQueried.size
    });

    stream.close();
    this.activeSearches.delete(requestId);
  }
}
```

### Progressive Search UX

```typescript
// CLI: User controls patience via timeout
const stream = await mesh.searchMesh('bartender skills', {
  timeout: 10000,      // Search for 10 seconds
  minResults: 5,
  maxHops: 3,
  minSimilarity: 0.7
});

// Display results as they arrive
stream.on('results', (results) => {
  console.log(`\n✓ Found ${results.length} new results:`);
  results.forEach(r => {
    console.log(`  ${r.adapter.name} (${(r.similarity * 100).toFixed(1)}% match, ${r.hops} hops)`);
  });
});

stream.on('complete', (summary) => {
  console.log(`\n🎯 Search complete: ${summary.totalResults} results from ${summary.peersQueried} peers in ${summary.elapsed}ms`);
  console.log(`\nContinue searching? [y/N]`);
});
```

**Output**:
```
Searching mesh for "bartender skills"...

[1s] ✓ Found 3 results:
  bartending-basics-v2 (78.2% match, 1 hop)
  mixology-advanced (82.1% match, 1 hop)
  cocktail-techniques (75.3% match, 2 hops)

[3s] ✓ Found 5 results:
  wine-sommelier-v1 (71.4% match, 2 hops)
  drink-mixing-pro (84.7% match, 3 hops)
  ...

[10s] 🎯 Search complete: 15 results from 47 peers in 10,234ms

Continue searching? [y/N] n
```

---

## Phase 3: DHT (Kademlia Routing)

### Overview
For networks with 1,000+ peers, gossip becomes inefficient. Distributed Hash Table (DHT) provides **O(log N)** routing.

**Based on**: Kademlia (used by BitTorrent, IPFS, Ethereum)

### Key Concepts

**1. Peer IDs and XOR Distance**
```typescript
// Each peer has 160-bit ID (random at startup)
const myPeerId = sha1(generateUUID());  // 160-bit

// Distance between peers = XOR
function distance(id1: PeerId, id2: PeerId): bigint {
  return BigInt(id1) ^ BigInt(id2);
}

// Peer A is "closer" to content C than peer B if:
// distance(A, C) < distance(B, C)
```

**2. k-Buckets (Routing Table)**
```typescript
// Each peer maintains 160 k-buckets (one per bit)
// k-bucket[i] = list of k peers at distance 2^i to 2^(i+1)

export class KademliaDHT {
  private kBuckets: KBucket[] = new Array(160);
  private k: number = 20;  // Bucket size

  /**
   * Find k closest peers to target ID
   */
  findClosestPeers(targetId: PeerId, k: number = 20): PeerId[] {
    const candidates: Array<{ peer: PeerId; distance: bigint }> = [];

    // Collect peers from relevant k-buckets
    for (const bucket of this.kBuckets) {
      for (const peer of bucket.peers) {
        candidates.push({
          peer,
          distance: distance(peer, targetId)
        });
      }
    }

    // Sort by XOR distance, return top k
    return candidates
      .sort((a, b) => Number(a.distance - b.distance))
      .slice(0, k)
      .map(c => c.peer);
  }
}
```

**3. Recursive Lookup**
```typescript
/**
 * Find peers storing content with hash H
 */
async findContent(contentHash: string): Promise<PeerId[]> {
  const targetId = contentHash;  // Content ID = Hash
  let closestPeers = this.findClosestPeers(targetId, this.k);
  let queriedPeers = new Set<PeerId>();

  // Recursive lookup
  while (closestPeers.length > 0) {
    // Query α closest unqueried peers (α = 3 typical)
    const toQuery = closestPeers
      .filter(p => !queriedPeers.has(p))
      .slice(0, 3);

    if (toQuery.length === 0) break;

    // Ask each peer: "Who do you know closer to target?"
    const responses = await Promise.all(
      toQuery.map(peer =>
        this.mesh.sendTo<FindNodeResponse>(peer, {
          type: 'find-node',
          targetId,
          k: this.k
        })
      )
    );

    // Merge responses
    for (const response of responses) {
      closestPeers = this.mergeClosest(closestPeers, response.peers, targetId);
    }

    toQuery.forEach(p => queriedPeers.add(p));
  }

  return closestPeers.slice(0, this.k);
}
```

**Performance**:
- **Routing complexity**: O(log N) hops for N peers
- **Example**: 1 million peers = ~20 hops
- **Lookup latency**: ~2 seconds for global search

---

## Bootstrap Strategies (No Central Seed)

### Multi-Strategy Approach

```typescript
// mesh/Bootstrap.ts

export class MeshBootstrap {
  private strategies: BootstrapStrategy[] = [
    new LocalPeerDiscovery(),      // mDNS/Bonjour
    new CachedPeerStrategy(),      // Remember from last session
    new HardcodedBootstrapNodes(), // Fallback
    new PublicPeerAnnouncements(), // GitHub/Pastebin/QR codes
    new PeerExchange()             // Ask first peer for more
  ];

  /**
   * Try all strategies in parallel, use first success
   */
  async bootstrap(): Promise<PeerId[]> {
    console.log('🌱 Bootstrapping mesh network...');

    const results = await Promise.allSettled(
      this.strategies.map(s => s.findPeers())
    );

    const allPeers: PeerId[] = [];
    for (const result of results) {
      if (result.status === 'fulfilled') {
        allPeers.push(...result.value);
      }
    }

    if (allPeers.length === 0) {
      throw new Error('Bootstrap failed: No peers found');
    }

    console.log(`✅ Bootstrap successful: Found ${allPeers.length} peers`);
    return allPeers;
  }
}

/**
 * Strategy 1: Local network discovery (mDNS)
 */
export class LocalPeerDiscovery implements BootstrapStrategy {
  async findPeers(): Promise<PeerId[]> {
    const mdns = new MDNS();
    const services = await mdns.discover('_continuum._tcp');

    // Returns: ["192.168.1.105:8080", "192.168.1.142:8080"]
    console.log(`📡 Found ${services.length} peers on local network`);
    return services.map(s => this.extractPeerId(s));
  }
}

/**
 * Strategy 2: Cached peers from previous session
 */
export class CachedPeerStrategy implements BootstrapStrategy {
  async findPeers(): Promise<PeerId[]> {
    const cached = await Commands.execute('data/list', {
      collection: 'known_peers',
      filter: {
        lastSeen: { $gte: Date.now() - 7 * 24 * 60 * 60 * 1000 }  // Active in last 7 days
      },
      orderBy: [{ field: 'lastSeen', direction: 'desc' }],
      limit: 50
    });

    console.log(`💾 Found ${cached.data.length} cached peers`);
    return cached.data.map(p => p.peerId);
  }
}

/**
 * Strategy 3: Hardcoded bootstrap nodes (community-run)
 */
export class HardcodedBootstrapNodes implements BootstrapStrategy {
  private BOOTSTRAP_NODES = [
    'bootstrap1.continuum.network:8080',
    'bootstrap2.continuum.network:8080',
    'peer.alice.dev:8080',
    'node.bob.io:8080'
  ];

  async findPeers(): Promise<PeerId[]> {
    const reachable = await this.pingAll(this.BOOTSTRAP_NODES);
    console.log(`🌐 Found ${reachable.length}/${this.BOOTSTRAP_NODES.length} bootstrap nodes online`);
    return reachable;
  }
}

/**
 * Strategy 4: Public peer announcements
 */
export class PublicPeerAnnouncements implements BootstrapStrategy {
  private ANNOUNCEMENT_URLS = [
    'https://gist.githubusercontent.com/continuum-peers/...',
    'https://pastebin.com/raw/continuum-peers',
    // User-submitted peer lists
  ];

  async findPeers(): Promise<PeerId[]> {
    const peers: PeerId[] = [];

    for (const url of this.ANNOUNCEMENT_URLS) {
      try {
        const response = await fetch(url);
        const text = await response.text();

        // Parse peer addresses (one per line)
        const addresses = text.split('\n').filter(l => l.startsWith('continuum://'));
        peers.push(...addresses.map(a => this.parsePeerAddress(a)));
      } catch (error) {
        console.warn(`⚠️ Failed to fetch ${url}:`, error);
      }
    }

    console.log(`📋 Found ${peers.length} peers from public announcements`);
    return peers;
  }

  /**
   * Parse peer address: continuum://peer:abc123@203.0.113.42:8080
   */
  private parsePeerAddress(address: string): PeerId {
    const match = address.match(/continuum:\/\/peer:(.+)@(.+):(\d+)/);
    if (!match) throw new Error(`Invalid peer address: ${address}`);

    const [, peerId, host, port] = match;
    return { id: peerId, host, port: parseInt(port) };
  }
}

/**
 * Strategy 5: Peer exchange (PEX)
 */
export class PeerExchange implements BootstrapStrategy {
  async findPeers(): Promise<PeerId[]> {
    // Assumes we've connected to at least ONE peer via other strategies
    const connectedPeers = this.mesh.getConnectedPeers();
    if (connectedPeers.length === 0) {
      return [];
    }

    // Ask first peer for their peer list
    const firstPeer = connectedPeers[0];
    const response = await this.mesh.sendTo<GetPeersResponse>(firstPeer, {
      type: 'get-peers',
      limit: 50
    });

    console.log(`🔄 Received ${response.peers.length} peers via PEX`);
    return response.peers;
  }
}
```

### User Experience

**First Launch** (no cached peers):
```
🌱 Bootstrapping mesh network...
📡 Found 2 peers on local network
🌐 Found 3/4 bootstrap nodes online
📋 Found 12 peers from public announcements
✅ Bootstrap successful: Found 17 peers
🔄 Received 43 peers via PEX (total: 60 peers)
```

**Subsequent Launches** (cached peers):
```
🌱 Bootstrapping mesh network...
💾 Found 47 cached peers
✅ Bootstrap successful: Found 47 peers (from cache)
```

**Network survives**: Even if all bootstrap nodes die, cached peers + PEX keeps network alive.

---

## Economic Layer: Alt Coin Rewards

### Overview
Incentivize valuable contributions (hosting adapters, providing compute, relaying requests) with alt coin rewards.

**NOT a blockchain** - just a reputation/credit system with transferable tokens.

### Token Mechanics

```typescript
// mesh/TokenEconomy.ts

export interface ContinuumToken {
  userId: UUID;
  balance: number;      // Current tokens
  earned: number;       // Lifetime earnings
  spent: number;        // Lifetime spending
  reputation: number;   // 0-1 score based on behavior
}

/**
 * Earn tokens for contributions
 */
export class TokenEconomy {
  /**
   * Reward for hosting popular adapter
   */
  async rewardAdapterHost(
    hostId: UUID,
    adapterId: UUID,
    downloads: number
  ): Promise<void> {
    // More downloads = more rewards
    const reward = Math.min(downloads * 0.1, 10);  // Cap at 10 tokens

    await this.addTokens(hostId, reward, {
      reason: 'adapter-hosting',
      adapterId,
      downloads
    });

    console.log(`💰 Rewarded ${hostId} with ${reward} tokens for hosting ${adapterId}`);
  }

  /**
   * Reward for providing compute (inference, fine-tuning)
   */
  async rewardCompute(
    providerId: UUID,
    requestId: UUID,
    tokensUsed: number
  ): Promise<void> {
    const reward = tokensUsed * 0.5;  // Provider gets 50% of request cost

    await this.addTokens(providerId, reward, {
      reason: 'compute-provided',
      requestId
    });
  }

  /**
   * Charge for downloading adapter from peer
   */
  async chargeDownload(
    downloaderId: UUID,
    adapterId: UUID,
    sizeBytes: number
  ): Promise<void> {
    const cost = Math.ceil(sizeBytes / 1_000_000);  // 1 token per MB

    await this.deductTokens(downloaderId, cost, {
      reason: 'adapter-download',
      adapterId
    });
  }

  /**
   * Reward for relaying search requests
   */
  async rewardRelay(relayerId: UUID, hopsRelayed: number): Promise<void> {
    const reward = hopsRelayed * 0.01;  // Small reward per relay

    await this.addTokens(relayerId, reward, {
      reason: 'search-relay',
      hopsRelayed
    });
  }
}
```

### Initial Token Distribution

```typescript
/**
 * Bootstrap economy: Users earn tokens by participating
 */
export const INITIAL_TOKEN_DISTRIBUTION = {
  newUser: 100,              // Starter tokens for new users
  firstAdapter: 50,          // Bonus for publishing first adapter
  earlyAdopter: 500,         // Bonus for joining before 1000 users
};

/**
 * Continuous token generation (no cap, inflationary)
 */
export const TOKEN_GENERATION_RATES = {
  adapterDownload: 0.1,      // Per download
  computeProvided: 0.5,      // Per inference request
  relayRequest: 0.01,        // Per search relay
  qualityRating: 1.0,        // For rating adapters (curation work)
};
```

### Marketplace Pricing

```typescript
// Users set prices for their services
export interface ServiceListing {
  providerId: UUID;
  serviceType: 'inference' | 'fine-tuning' | 'adapter' | 'storage';
  pricePerUnit: number;      // Tokens
  availability: number;      // 0-1 (capacity)
  reputation: number;        // 0-1 (trust score)
}

// Example: Inference marketplace
const listings = await Commands.execute('data/list', {
  collection: 'service_listings',
  filter: {
    serviceType: 'inference',
    availability: { $gte: 0.5 }
  },
  orderBy: [
    { field: 'pricePerUnit', direction: 'asc' },
    { field: 'reputation', direction: 'desc' }
  ]
});

// User picks cheapest provider with good reputation
const bestProvider = listings.data[0];
```

---

## Use Cases

### 1. Genome Marketplace
```bash
# Search for skill
./jtag genome/search "async debugging" --network --timeout=10s

# Results:
# 1. async-debugging-v2 (0.91 similarity)
#    Host: carol-ai, Price: 5 tokens, Downloads: 342, Rating: 4.8/5
# 2. race-condition-expert (0.88 similarity)
#    Host: dave-ai, Price: 8 tokens, Downloads: 127, Rating: 4.9/5

# Download adapter (pay tokens)
./jtag genome/install <content-hash> --from-peer=carol-ai
# Cost: 5 tokens (auto-deducted from balance)

# Host your own adapters (earn tokens)
./jtag genome/publish <adapter-id> --price=3
# Earn tokens every time someone downloads
```

### 2. Social Media (Decentralized Chat)
```bash
# Post to mesh (gossip protocol distributes)
./jtag social/post "Check out my new bartending LoRA! 🍸"

# Semantic search for posts
./jtag social/search "LoRA adapters for bartending" --limit=20

# Tip creators with tokens
./jtag social/tip <post-id> --amount=5 --message="Great work!"
```

### 3. Compute Marketplace
```bash
# Request inference from mesh (lowest price wins)
./jtag ai/generate --prompt="..." --maxPrice=2 --preferLocal=false

# Mesh finds cheapest available provider:
# - alice-ai: 1.5 tokens/request, 98% uptime, 4.9/5 rating
# - bob-ai: 2.0 tokens/request, 99% uptime, 5.0/5 rating

# Provide your own compute (earn tokens)
./jtag compute/start-provider --pricePerRequest=1.5
# Earn tokens for every inference request served
```

### 4. Content-Addressed Storage (IPFS-like)
```bash
# Store large artifacts (training datasets, model weights)
./jtag storage/put ./dataset.parquet --replicas=3
# Cost: 10 tokens (paid to 3 hosting peers)
# Returns: ipfs://Qm...hash

# Retrieve from any peer
./jtag storage/get ipfs://Qm...hash
# Auto-downloads from closest/cheapest peer
```

---

## Security & Adversarial Resistance

### 1. No Single Point of Failure
- **Problem**: Central server can be taken down
- **Solution**: Mesh has no central authority - takedown one node, 999 others continue
- **Example**: Even if all bootstrap nodes die, cached peers + PEX keep network alive

### 2. Content-Addressed Storage
- **Problem**: Content can be censored or tampered with
- **Solution**: Content hash is identity - verify integrity before using
- **Example**: Download adapter from 3 peers, verify SHA-256 matches, use if valid

```typescript
async downloadAdapter(contentHash: string, peers: PeerId[]): Promise<Uint8Array> {
  for (const peer of peers) {
    try {
      const data = await this.mesh.download(peer, contentHash);
      const actualHash = sha256(data);

      if (actualHash === contentHash) {
        return data;  // ✅ Valid
      } else {
        console.warn(`⚠️ Hash mismatch from ${peer}, trying next peer`);
      }
    } catch (error) {
      console.warn(`⚠️ Download failed from ${peer}, trying next peer`);
    }
  }

  throw new Error('All peers failed integrity check');
}
```

### 3. Plausible Deniability
- **Problem**: Hosting illegal content could implicate node operator
- **Solution**: Nodes relay requests but don't inspect content (like Tor middle nodes)
- **Legal defense**: "I'm just forwarding encrypted packets, I don't know what's in them"

### 4. Encrypted Transport
- **Problem**: ISPs or governments could monitor traffic
- **Solution**: All mesh communication over TLS (WebRTC/WebSocket with encryption)
- **Result**: No one can see what you're searching for or downloading

### 5. Sybil Attack Resistance
- **Problem**: Attacker creates 1000 fake peers to pollute network
- **Solution**: Reputation system + proof-of-work for new peers
- **Example**: New peer must solve small computational puzzle to join (1s of CPU time)

```typescript
/**
 * Proof-of-work for joining network
 */
async joinMesh(): Promise<void> {
  const challenge = await this.bootstrap.getChallenge();

  // Solve puzzle: find nonce where sha256(peerId + nonce) < difficulty
  const nonce = await this.solveProofOfWork(challenge);

  // Submit proof
  await this.bootstrap.submitProof(this.myPeerId, nonce);

  // Granted initial reputation (can now relay requests)
  console.log('✅ Joined mesh with initial reputation');
}
```

### 6. Rate Limiting & Spam Prevention
- **Problem**: Malicious peer floods network with requests
- **Solution**: Each peer limits requests per second from any peer
- **Example**: Max 10 requests/second from any peer, excess dropped

---

## Implementation Phases

### Phase 1: Gossip + Local-First Search (Week 1-2)
**Goal**: 10-100 peers, local network only

**Deliverables**:
- [ ] GossipCatalogSync (30s intervals)
- [ ] Local vector search with remote cache
- [ ] Bootstrap strategies (mDNS, cached peers)
- [ ] CLI: `./jtag genome/search --network`

**Test**: Deploy 10 peers on local network, verify catalog sync works

---

### Phase 2: Bounded Flood (Week 3-4)
**Goal**: 100-1000 peers, multi-hop search

**Deliverables**:
- [ ] BoundedFloodSearch (TTL-based)
- [ ] Progressive timeout (patience = accuracy)
- [ ] Smart forwarding (specialization-based)
- [ ] CLI: `./jtag genome/search --network --timeout=10s`

**Test**: Deploy 100 peers, verify multi-hop search finds remote adapters

---

### Phase 3: DHT (Month 2)
**Goal**: 1,000-10,000 peers, O(log N) routing

**Deliverables**:
- [ ] KademliaDHT (k-buckets, XOR distance)
- [ ] Recursive lookup
- [ ] Peer exchange (PEX)
- [ ] CLI: `./jtag dht/stats` (show routing table)

**Test**: Deploy 1000 peers, verify lookup latency < 3s

---

### Phase 4: Economic Layer (Month 3)
**Goal**: Token rewards, marketplace

**Deliverables**:
- [ ] TokenEconomy (earn/spend tokens)
- [ ] Service listings (inference, storage)
- [ ] Pricing mechanism (supply/demand)
- [ ] CLI: `./jtag wallet/balance`, `./jtag marketplace/list`

**Test**: Deploy marketplace, verify tokens transfer correctly

---

## Performance Targets

| Metric | Phase 1 (Gossip) | Phase 2 (Flood) | Phase 3 (DHT) |
|--------|------------------|-----------------|---------------|
| Network size | 10-100 peers | 100-1,000 peers | 1,000-10,000 peers |
| Search latency | 10-50ms | 1-5s | 2-10s |
| Hop count | 1 (direct) | 1-3 | 5-20 (log N) |
| Cache hit rate | 80% | 70% | 60% |
| Bandwidth/peer | 1 KB/s | 5 KB/s | 10 KB/s |

---

## Monitoring & Observability

```bash
# Mesh health
./jtag mesh/status
# Output:
# Connected peers: 47
# Catalog size: 9,824 adapters
# Cache hit rate: 78%
# Last gossip: 5s ago
# Token balance: 156 tokens

# Network topology
./jtag mesh/topology --graph
# Output: ASCII graph of peer connections

# Economic stats
./jtag wallet/stats
# Output:
# Balance: 156 tokens
# Earned (total): 342 tokens
# Spent (total): 186 tokens
# Top earning adapter: bartending-basics-v2 (127 downloads, 12.7 tokens)
```

---

## The Vision

**Continuum becomes**:
- 🧬 **AI Marketplace**: Discover and trade LoRA adapters, personas, fine-tuned models
- 💬 **Social Network**: Gossip-based posts, semantic search, tip creators with tokens
- 🌐 **Alternative Internet**: Mesh network immune to censorship and takedowns
- 💰 **Economic System**: Alt coin rewards for hosting, compute, curation, relay
- 🔒 **Privacy-First**: Encrypted transport, no central monitoring, plausible deniability
- 🚀 **Unstoppable**: No central authority, no single point of failure, community-run

**The killer feature**: "We just want the best match that is available now, and likely nearby."

Fast, approximate, good enough. User controls accuracy via patience. No blockchain overhead. Just works.

---

## Next Steps

1. **Implement Phase 1** (gossip + local-first search)
2. **Deploy to 10 local peers** (test catalog sync)
3. **Add bounded flood** (Phase 2)
4. **Scale to 100 peers** (multi-hop search)
5. **Introduce tokens** (Phase 4)
6. **Build marketplace** (genome + compute)
7. **Scale to 1000+ peers** (DHT if needed)

---

## References

- **BitTorrent DHT**: [BEP 5](http://www.bittorrent.org/beps/bep_0005.html)
- **Kademlia Paper**: [Maymounkov & Mazières 2002](https://pdos.csail.mit.edu/~petar/papers/maymounkov-kademlia-lncs.pdf)
- **IPFS**: [Content-Addressed Storage](https://docs.ipfs.io/concepts/content-addressing/)
- **Gossip Protocols**: [Epidemic Algorithms for Replicated Database Maintenance](https://dl.acm.org/doi/10.1145/41840.41841)
- **Semantic Search**: See `SEMANTIC-SEARCH-ARCHITECTURE.md` (parent doc)
- **Genome Discovery**: See `PERSONA-GENOME-VECTOR-SEARCH.md`
