# PersonaGenome Vector Search Architecture

**Status**: Design Phase (Phase 3)
**Created**: 2025-11-23
**Parent Doc**: `system/search/SEMANTIC-SEARCH-ARCHITECTURE.md`

## Overview

PersonaGenome uses semantic vector search to discover and activate LoRA adapters based on task requirements. This enables:
- **Local discovery**: Find adapters in persona's local genome
- **Network discovery**: Search across P2P mesh for specialized skills
- **Automatic activation**: Load best-matching adapter for current task
- **Skill marketplace**: Share and discover adapters across network

---

## Architecture

```
Task: "Debug async race conditions"
    ↓
PersonaGenome.findRelevantAdapters(taskDescription)
    ↓
Commands.execute('data/vector-search', {
  collection: 'genomic_layers',
  queryText: taskDescription,
  filter: { quality: { $gte: 0.7 } }
})
    ↓
DataDaemon.vectorSearch() [via adapter strategy]
    ↓
SQLite/PostgreSQL/Elasticsearch backend
    ↓
Returns ranked adapters by semantic similarity
    ↓
PersonaGenome.activateAdapter(bestMatch)
```

---

## Data Model

### Genomic Layer Entity

```typescript
// system/user/server/modules/PersonaGenome.ts

export interface LoRAAdapterEntity extends BaseEntity {
  id: UUID;
  createdAt: ISOString;
  updatedAt: ISOString;
  version: number;

  // Identity
  name: string;
  description: string;           // Searchable description
  contentHash: string;           // SHA-256 of weights (for integrity)

  // Discovery (KEY FIELDS FOR VECTOR SEARCH)
  capabilities: string[];        // ["debugging", "async", "concurrency"]
  specializations: string[];     // ["race-conditions", "websockets"]
  trainingDomains: string[];     // ["backend", "real-time-systems"]

  // Quality metrics
  metrics: {
    accuracy: number;            // 0-1
    trainingExamples: number;
    validationLoss: number;
    epochs: number;
  };

  // Network sharing
  creator: UUID;                 // Who trained it
  peers: string[];               // P2P addresses where available
  downloads: number;
  ratings: Array<{
    userId: UUID;
    rating: number;              // 1-5
    comment?: string;
  }>;

  // Storage
  weightsPath: string;           // Local path to weights
  sizeBytes: number;
}
```

### Searchable Content Structure

When indexing for vector search, combine fields into searchable text:

```typescript
function buildSearchableContent(adapter: LoRAAdapterEntity): string {
  return `
${adapter.name}

Description: ${adapter.description}

Capabilities: ${adapter.capabilities.join(', ')}

Specializations: ${adapter.specializations.join(', ')}

Trained on: ${adapter.trainingDomains.join(', ')}

Quality: ${(adapter.metrics.accuracy * 100).toFixed(1)}% accuracy
Trained with ${adapter.metrics.trainingExamples} examples

Created by: ${adapter.creator}
Downloads: ${adapter.downloads}
Average rating: ${calculateAvgRating(adapter.ratings)}
  `.trim();
}
```

---

## Implementation

### PersonaGenome Integration

```typescript
// system/user/server/modules/PersonaGenome.ts

export class PersonaGenome {
  private adapters: Map<UUID, LoRAAdapter>;
  private activeAdapters: Map<string, LoRAAdapter>;  // domain -> adapter

  /**
   * Find relevant adapters for a task (local + network)
   */
  async findRelevantAdapters(
    taskDescription: string,
    options: {
      searchNetwork?: boolean;     // Search P2P mesh
      minQuality?: number;         // Minimum quality threshold
      maxResults?: number;         // Top N results
      domains?: string[];          // Filter by domain
    } = {}
  ): Promise<LoRAAdapterEntity[]> {

    // 1. Search local genome first (fast)
    const localResults = await this.searchLocalAdapters(
      taskDescription,
      options
    );

    // 2. If insufficient local results, search network
    if (options.searchNetwork &&
        (localResults.length === 0 || localResults[0].metrics.accuracy < (options.minQuality || 0.8))) {

      console.log(`🔍 Searching P2P network for specialized adapters...`);

      const networkResults = await this.searchNetworkAdapters(
        taskDescription,
        options
      );

      // Merge and deduplicate
      return this.mergeAndRankResults(localResults, networkResults);
    }

    return localResults;
  }

  /**
   * Search local genome using DataDaemon vector search
   */
  private async searchLocalAdapters(
    taskDescription: string,
    options: SearchOptions
  ): Promise<LoRAAdapterEntity[]> {

    const results = await Commands.execute('data/vector-search', {
      collection: 'genomic_layers',
      queryText: taskDescription,
      k: options.maxResults || 5,
      similarityThreshold: 0.6,
      hybridMode: 'hybrid',  // Combine semantic + keyword
      filter: {
        'metrics.accuracy': { $gte: options.minQuality || 0.7 },
        ...(options.domains && { trainingDomains: { $in: options.domains } })
      }
    });

    return results.results.map(r => r.data as LoRAAdapterEntity);
  }

  /**
   * Search P2P network for adapters
   */
  private async searchNetworkAdapters(
    taskDescription: string,
    options: SearchOptions
  ): Promise<LoRAAdapterEntity[]> {

    // Generate query embedding locally
    const queryEmbedding = await Commands.execute('data/generate-embedding', {
      text: taskDescription,
      model: 'all-minilm'
    });

    // Broadcast search to mesh
    const networkResults = await this.mesh.searchGenomicLayers({
      embedding: queryEmbedding.vector,
      keywords: await this.extractKeywords(taskDescription),
      filters: {
        minQuality: options.minQuality,
        domains: options.domains
      },
      timeout: 5000  // 5s max wait
    });

    return networkResults;
  }

  /**
   * Activate best-matching adapter for domain
   */
  async activateAdapterForTask(
    taskDescription: string,
    domain: string
  ): Promise<LoRAAdapter | null> {

    // Find relevant adapters
    const candidates = await this.findRelevantAdapters(taskDescription, {
      searchNetwork: true,
      minQuality: 0.75,
      maxResults: 3,
      domains: [domain]
    });

    if (candidates.length === 0) {
      console.log(`⚠️ No adapters found for: ${taskDescription}`);
      return null;
    }

    const best = candidates[0];
    console.log(`✅ Activating adapter: ${best.name} (quality: ${(best.metrics.accuracy * 100).toFixed(1)}%)`);

    // If from network, download first
    if (!this.hasLocalCopy(best.id)) {
      await this.downloadAdapter(best);
    }

    // Load and activate
    const adapter = await this.loadAdapter(best.id);
    await this.activateAdapter(domain, adapter);

    return adapter;
  }

  /**
   * Publish adapter to network (makes it discoverable)
   */
  async publishAdapter(adapterId: UUID): Promise<void> {
    const adapter = this.adapters.get(adapterId);
    if (!adapter) {
      throw new Error(`Adapter not found: ${adapterId}`);
    }

    // 1. Build searchable content
    const searchableContent = buildSearchableContent(adapter.entity);

    // 2. Generate embedding
    const embedding = await Commands.execute('data/generate-embedding', {
      text: searchableContent,
      model: 'all-minilm'
    });

    // 3. Index locally
    await Commands.execute('data/index-vector', {
      collection: 'genomic_layers',
      id: adapter.entity.id,
      embedding: embedding.vector
    });

    // 4. Publish descriptor to P2P mesh
    await this.mesh.publishGenomicLayer({
      ...adapter.entity,
      embedding: embedding.vector,
      peers: [this.mesh.getMyAddress()]
    });

    console.log(`📢 Published adapter to network: ${adapter.entity.name}`);
  }
}
```

---

## P2P Network Protocol

### Genomic Layer Discovery Protocol

```typescript
// mesh/GenomicLayerDiscovery.ts

export interface GenomicLayerSearchRequest {
  type: 'genomic-layer-search';
  requestId: UUID;
  sender: PeerId;
  timestamp: number;
  query: {
    embedding: number[];         // Query vector
    keywords: string[];          // For hybrid search
    filters: {
      minQuality?: number;
      domains?: string[];
      capabilities?: string[];
    };
  };
}

export interface GenomicLayerSearchResponse {
  type: 'genomic-layer-search-response';
  requestId: UUID;
  sender: PeerId;
  timestamp: number;
  results: Array<{
    adapter: LoRAAdapterEntity;
    similarity: number;          // Cosine similarity 0-1
  }>;
}

export class GenomicLayerDiscovery {

  /**
   * Handle incoming search request from peer
   */
  async handleSearchRequest(request: GenomicLayerSearchRequest): Promise<void> {

    // Search local genome using DataDaemon
    const results = await Commands.execute('data/vector-search', {
      collection: 'genomic_layers',
      queryVector: request.query.embedding,
      k: 20,
      filter: {
        'metrics.accuracy': { $gte: request.query.filters.minQuality || 0.7 },
        ...(request.query.filters.domains && {
          trainingDomains: { $in: request.query.filters.domains }
        })
      }
    });

    // Send response back to requester
    const response: GenomicLayerSearchResponse = {
      type: 'genomic-layer-search-response',
      requestId: request.requestId,
      sender: this.mesh.myPeerId,
      timestamp: Date.now(),
      results: results.results.map(r => ({
        adapter: r.data as LoRAAdapterEntity,
        similarity: r.score
      }))
    };

    await this.mesh.sendTo(request.sender, response);
  }

  /**
   * Search network for adapters
   */
  async searchNetwork(
    query: GenomicLayerSearchRequest['query'],
    timeout: number = 5000
  ): Promise<LoRAAdapterEntity[]> {

    const requestId = generateUUID();

    // Broadcast to all connected peers
    await this.mesh.broadcast('genomic-layer-discovery', {
      type: 'genomic-layer-search',
      requestId,
      sender: this.mesh.myPeerId,
      timestamp: Date.now(),
      query
    });

    // Collect responses with timeout
    const responses = await this.collectResponses(requestId, timeout);

    // Aggregate and rank
    const allResults = responses.flatMap(r => r.results);
    return allResults
      .sort((a, b) => b.similarity - a.similarity)
      .map(r => r.adapter);
  }
}
```

---

## CLI Commands

### Genome Management

```bash
# List local adapters
./jtag genome/list

# Search local genome
./jtag genome/search "async debugging patterns"
# Results:
# 1. async-debugging-v2 (quality: 0.91, similarity: 0.89)
# 2. race-condition-detector (quality: 0.88, similarity: 0.85)

# Search network
./jtag genome/search "async debugging patterns" --network
# Results:
# Local (2):
#   1. async-debugging-v2 (quality: 0.91)
#   2. race-condition-detector (quality: 0.88)
#
# Network (3):
#   1. websocket-concurrency (quality: 0.92, peer: carol-ai)
#   2. async-advanced (quality: 0.89, peer: bob-ai)
#   3. concurrency-patterns (quality: 0.86, peer: dave-ai)

# Download and install from network
./jtag genome/install <content-hash> --from-peer=carol-ai

# Publish adapter to network
./jtag genome/publish <adapter-id>

# Rate an adapter
./jtag genome/rate <adapter-id> --rating=5 --comment="Excellent debugging!"
```

### Vector Search Commands

```bash
# Backfill embeddings for existing adapters
./jtag data/backfill-vectors --collection=genomic_layers

# Search with filters
./jtag data/vector-search \
  --collection=genomic_layers \
  --query="concurrent error handling" \
  --k=5 \
  --filter='{"metrics.accuracy":{"$gte":0.8}}'

# Check vector index status
./jtag data/vector-stats --collection=genomic_layers
# Output:
# Collection: genomic_layers
# Total documents: 47
# Documents with vectors: 47
# Vector dimensions: 384
# Index size: 71KB
```

---

## Usage Examples

### Automatic Skill Activation

```typescript
// In PersonaResponseGenerator or task processor

async processTask(task: Task): Promise<void> {
  // Automatically find and load relevant skill
  const adapter = await this.persona.genome.activateAdapterForTask(
    task.description,
    task.domain
  );

  if (adapter) {
    console.log(`🧬 Loaded specialized skill: ${adapter.entity.name}`);
  }

  // Process task with appropriate skill active
  const result = await this.persona.generateText({
    prompt: this.buildPrompt(task),
    context: task.domain
  });

  return result;
}
```

### Skill Discovery Flow

```typescript
// User asks: "How do I debug race conditions in async code?"

// 1. Extract task from query
const taskDescription = "Debug race conditions in async code";
const domain = "debugging";

// 2. Search for relevant adapters
const adapters = await persona.genome.findRelevantAdapters(taskDescription, {
  searchNetwork: true,
  minQuality: 0.8,
  maxResults: 3,
  domains: ['debugging', 'async', 'concurrency']
});

// 3. Present options to user (optional)
console.log("Found specialized skills:");
adapters.forEach((a, i) => {
  console.log(`${i + 1}. ${a.name} (${(a.metrics.accuracy * 100).toFixed(1)}%)`);
  console.log(`   ${a.description}`);
  console.log(`   Downloads: ${a.downloads}, Rating: ${avgRating(a.ratings)}`);
});

// 4. Activate best match
const chosen = adapters[0];
await persona.genome.activateAdapterForTask(taskDescription, domain);

// 5. Respond with specialized knowledge
const response = await persona.generateText({
  prompt: "Explain how to debug race conditions in async code",
  context: domain
});
```

---

## Performance Considerations

### Local Search

- **Vector search latency**: 1-5ms for 100s of adapters
- **Embedding generation**: ~50-100ms per query (Ollama local)
- **Total latency**: <200ms for local search

### Network Search

- **Broadcast latency**: 10-50ms per peer
- **Response collection**: 100-500ms for 10 peers
- **Total latency**: 1-5 seconds with timeout

### Optimization

1. **Cache embeddings** for common queries
2. **Pre-filter by metadata** before vector search
3. **Batch downloads** of multiple adapters
4. **Prioritize local adapters** (faster, no network)

---

## Security & Trust

### Content Verification

```typescript
// Verify downloaded adapter integrity
async downloadAdapter(descriptor: LoRAAdapterEntity): Promise<void> {
  const weights = await this.mesh.download(descriptor.contentHash, {
    peers: descriptor.peers,
    verify: true
  });

  // Verify hash matches
  const actualHash = sha256(weights);
  if (actualHash !== descriptor.contentHash) {
    throw new Error('Integrity check failed! Downloaded weights do not match hash.');
  }

  // Store locally
  await this.storeAdapter(descriptor.id, weights);
}
```

### Trust Circles

```typescript
// Only download from trusted peers
const TRUSTED_PEERS = ['bob-ai', 'carol-ai', 'helper-ai'];

async findRelevantAdapters(task: string, options: SearchOptions) {
  const results = await this.searchNetworkAdapters(task, options);

  // Filter by trust
  return results.filter(adapter =>
    TRUSTED_PEERS.includes(adapter.creator)
  );
}
```

### Reputation System

```typescript
// Rate adapters after use
async rateAdapter(adapterId: UUID, rating: number, comment?: string): Promise<void> {
  await Commands.execute('data/update', {
    collection: 'genomic_layers',
    id: adapterId,
    updates: {
      $push: {
        ratings: {
          userId: this.personaId,
          rating,
          comment,
          timestamp: new Date().toISOString()
        }
      }
    }
  });

  // Share rating with network
  await this.mesh.broadcastRating(adapterId, rating);
}
```

---

## Migration Path

### Week 1: Local Vector Search

1. Extend DataDaemon with vector operations
2. Index existing local adapters
3. Implement `findRelevantAdapters()` (local only)

### Week 2: Network Discovery Protocol

1. Implement GenomicLayerDiscovery
2. Handle search requests/responses
3. Test cross-peer searches

### Week 3: Download & Verification

1. Implement content-addressed storage
2. Multi-peer download with verification
3. Automatic cache management

### Week 4: Marketplace UI

1. CLI commands for search/install/rate
2. Interactive adapter browser
3. Reputation visualization

---

## Next Steps

1. **Read**: `SEMANTIC-SEARCH-ARCHITECTURE.md` for DataDaemon implementation
2. **Read**: `HIPPOCAMPUS-VECTOR-RETRIEVAL.md` for memory retrieval patterns
3. **Implement**: Start with local vector search
4. **Test**: Index existing adapters and search
5. **Expand**: Add network discovery protocol

---

## References

- Parent doc: `system/search/SEMANTIC-SEARCH-ARCHITECTURE.md`
- Related: `LORA-GENOME-PAGING.md` (adapter management)
- Related: `PERSONA-CONVERGENCE-ROADMAP.md` (overall architecture)
