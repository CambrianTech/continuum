# Persona Discovery Architecture

**Efficient Vector Search + Benchmark-Driven Ranking for Autonomous Persona Selection**

## 🎯 **THE CORE PROBLEM**

The Academy needs to **autonomously discover and select optimal personas** for any given task without human curation. This requires:

1. **Fast similarity search** - Find personas relevant to the task (vector search)
2. **Performance validation** - Rank by proven capability on similar tasks (benchmark data)
3. **Resource optimization** - Consider compute, latency, and availability constraints
4. **Real-time updates** - Learn from each interaction to improve future selections

## ⚡ **TWO-LAYER SEARCH ARCHITECTURE**

### **Layer 1: Vector Similarity (Fast Filtering)**
```typescript
interface TaskEmbedding {
  readonly domain: string[];      // ["code", "typescript", "debugging"]
  readonly complexity: number;    // 0-1 scale
  readonly context: string[];     // ["browser", "backend", "testing"]
  readonly urgency: number;       // 0-1 scale for latency requirements
}

// Cast wide net quickly using approximate vector search
const candidates = await vectorIndex.search({
  query: taskEmbedding,
  topK: 100,                     // Fast approximate search
  filters: {
    domain: taskRequirements.domain,
    minMemory: resourceConstraints.memory,
    maxLatency: resourceConstraints.latency
  }
});
```

### **Layer 2: Benchmark Ranking (Quality Sorting)**
```typescript
interface PersonaBenchmarks {
  readonly accuracy: Map<TaskType, number>;        // 0-1 success rate
  readonly latency: Map<TaskType, number>;         // Response time ms
  readonly efficiency: Map<TaskType, number>;      // Quality/compute ratio
  readonly userSatisfaction: Map<TaskType, number>; // Human feedback
  readonly lastUpdated: Map<TaskType, Date>;       // Recency weighting
}

// Rank candidates by proven performance
const rankedPersonas = candidates.map(persona => ({
  persona,
  similarityScore: persona.vectorSimilarity,
  benchmarkScore: persona.benchmarks.getCompositeScore(taskType),
  availabilityScore: persona.isAvailable() ? 1.0 : 0.3,
  recencyScore: calculateRecencyWeight(persona.lastUpdate),
  compositeScore: combineScores(similarity, benchmark, availability, recency)
})).sort(by_compositeScore_desc);
```

## 🏗️ **EFFICIENT INDEXING STRATEGY**

### **Multi-Dimensional Index Architecture**
```typescript
interface PersonaSearchIndex {
  // Vector similarity index (HNSW for speed)
  vectorIndex: HNSWIndex<PersonaEmbedding>;
  
  // Benchmark performance indexes (sorted lists for fast ranking)
  benchmarkIndex: {
    byTaskType: Map<TaskType, SortedPersonaList>;
    byDomain: Map<Domain, SortedPersonaList>;
    byLatency: SortedPersonaList;
    byAccuracy: SortedPersonaList;
    byEfficiency: SortedPersonaList;
  };
  
  // Resource constraint indexes (range queries)
  resourceIndex: {
    byMemoryRequirement: RangeIndex<number>;
    byComputeRequirement: RangeIndex<number>;
    byAvailability: BooleanIndex;
    byP2PLatency: RangeIndex<number>;
  };
  
  // Real-time updates
  realtimeIndex: {
    activeConnections: Set<PersonaId>;
    loadBalancing: Map<PersonaId, LoadMetrics>;
    healthStatus: Map<PersonaId, HealthStatus>;
  };
}
```

## 🎯 **COMPOSITE SCORING ALGORITHM**

### **Weighted Performance Calculation**
```typescript
function calculateOptimalPersona(
  candidates: PersonaCandidate[],
  task: TaskRequirements,
  constraints: ResourceConstraints
): ScoredPersona {
  
  return candidates.map(persona => {
    // Vector similarity (30% weight)
    const similarity = cosineSimilarity(persona.embedding, task.embedding);
    
    // Benchmark performance (50% weight)
    const benchmark = persona.benchmarks.getWeightedScore(task.type, {
      accuracy: 0.4,
      latency: 0.3, 
      efficiency: 0.2,
      satisfaction: 0.1
    });
    
    // Availability and resource fit (15% weight)
    const availability = persona.meetsConstraints(constraints) ? 1.0 : 0.0;
    
    // Recency and learning curve (5% weight)
    const recency = Math.exp(-daysSinceLastUpdate(persona) / 30);
    
    return {
      persona,
      compositeScore: (similarity * 0.3) + (benchmark * 0.5) + (availability * 0.15) + (recency * 0.05)
    };
  })
  .sort((a, b) => b.compositeScore - a.compositeScore)[0];
}
```

## 📊 **REAL-TIME BENCHMARK UPDATES**

### **Continuous Learning from Interactions**
```typescript
interface InteractionResult {
  readonly personaId: UUID;
  readonly taskType: TaskType;
  readonly startTime: Date;
  readonly endTime: Date;
  readonly accuracy: number;          // Did it solve the problem correctly?
  readonly userSatisfaction: number;  // Human feedback score
  readonly resourceUsage: ResourceMetrics;
  readonly context: TaskContext;
}

// After each persona interaction
async function updatePersonaBenchmarks(result: InteractionResult) {
  // Update benchmark scores
  const benchmarks = await PersonaBenchmarks.get(result.personaId);
  benchmarks.updateScore(result.taskType, {
    accuracy: result.accuracy,
    latency: result.endTime.getTime() - result.startTime.getTime(),
    efficiency: result.accuracy / result.resourceUsage.compute,
    satisfaction: result.userSatisfaction
  });
  
  // Trigger re-indexing for future searches
  await PersonaSearchIndex.updateBenchmarkRanking(result.personaId);
  
  // Propagate learning to similar personas (genomic learning)
  await propagatePerformanceSignals(result);
}
```

## ⚡ **PERFORMANCE OPTIMIZATIONS**

### **Sub-100ms Persona Discovery Pipeline**
```typescript
async function findOptimalPersona(task: TaskRequirements): Promise<PersonaSelection> {
  const startTime = performance.now();
  
  // Stage 1: Vector search (10-20ms)
  const candidates = await vectorIndex.searchApproximate(task.embedding, 100);
  
  // Stage 2: Benchmark filtering (5-10ms)  
  const benchmarkFiltered = candidates.filter(p => 
    p.benchmarks.getMinScore(task.type) > QUALITY_THRESHOLD
  );
  
  // Stage 3: Resource validation (1-5ms)
  const resourceFiltered = benchmarkFiltered.filter(p =>
    p.meetsResourceConstraints(task.constraints)
  );
  
  // Stage 4: Composite scoring (1-5ms)
  const optimal = calculateOptimalPersona(resourceFiltered, task);
  
  const totalTime = performance.now() - startTime;
  console.log(`Persona discovery completed in ${totalTime}ms`);
  
  return optimal;
}
```

### **Caching and Pre-computation**
```typescript
// Cache frequently accessed data
const PersonaCache = {
  // Hot personas for common task types
  popularPersonas: new LRU<TaskType, PersonaId[]>(1000),
  
  // Pre-computed embeddings for common tasks
  commonTaskEmbeddings: new Map<string, TaskEmbedding>(),
  
  // Materialized benchmark rankings updated async
  rankingCache: new Map<TaskType, SortedPersonaList>(),
  
  // Bloom filters for quick resource constraint checks
  resourceFilters: new Map<ResourceType, BloomFilter>()
};
```

## 🌍 **GLOBAL PERSONA DISCOVERY**

### **P2P Network Integration**
```typescript
interface GlobalPersonaNetwork {
  // Local index with cached remote personas
  localIndex: PersonaSearchIndex;
  
  // P2P discovery for personas not available locally
  remoteDiscovery: {
    peerNodes: Set<PeerNode>;
    replicationProtocol: ReplicationProtocol;
    loadBalancing: GlobalLoadBalancer;
  };
  
  // Hybrid search: local-first, remote fallback
  async searchGlobally(task: TaskRequirements): Promise<PersonaSelection[]>;
}

// Search strategy: Local → Regional → Global
async function findBestPersonaAnywhere(task: TaskRequirements): Promise<PersonaSelection> {
  // Try local first (fastest)
  let result = await localIndex.search(task);
  if (result.score > LOCAL_QUALITY_THRESHOLD) return result;
  
  // Try regional P2P (medium latency, good quality)
  result = await regionalP2P.search(task);
  if (result.score > REGIONAL_QUALITY_THRESHOLD) return result;
  
  // Try global network (higher latency, best quality)
  return await globalP2P.search(task);
}
```

## 🎯 **SUCCESS METRICS**

### **Discovery Performance KPIs**
- **Search Latency**: Sub-100ms for 95% of queries
- **Accuracy Rate**: >90% of selected personas successfully complete tasks
- **Cache Hit Rate**: >80% of common tasks served from cache
- **Learning Rate**: Benchmark scores improve >5% weekly from interaction data

### **Quality Validation**
- **A/B Testing**: Compare persona selection vs random/rule-based selection
- **Human Feedback Loop**: Track satisfaction scores and selection override frequency  
- **Performance Benchmarks**: Measure task completion speed and quality over time
- **Resource Efficiency**: Monitor compute utilization and cost per successful task

## 🚀 **IMPLEMENTATION ROADMAP**

### **Phase 1: Local Vector Search** *(Foundation)*
- Implement HNSW vector index for persona embeddings
- Basic benchmark tracking and scoring
- Simple composite ranking algorithm
- Local persona discovery working

### **Phase 2: Performance Optimization** *(Speed)*
- Multi-dimensional indexing with caching
- Real-time benchmark updates
- Sub-100ms search performance
- Resource constraint filtering

### **Phase 3: Learning Integration** *(Intelligence)*
- Continuous learning from interactions
- Dynamic scoring weight optimization
- Genomic signal propagation
- Adaptive quality thresholds

### **Phase 4: Global Discovery** *(Scale)*
- P2P persona network integration
- Global search with local fallbacks
- Distributed benchmark synchronization
- Cross-network persona portability

---

**This architecture enables the Academy to autonomously discover and select optimal personas for any task, learning continuously from interactions while maintaining sub-100ms search performance even at global scale.**