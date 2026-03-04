**Parent:** [Personas](README.md)

# Academy Architectural Design - Complete System Integration

> **Status**: This is the **long-term vision** document for the Academy ecosystem. The foundational implementation (Phase 1) is complete as a sentinel-based dual-teacher/student system. For the current working implementation, see **[ACADEMY-DOJO-ARCHITECTURE.md](ACADEMY-DOJO-ARCHITECTURE.md)** — that document describes the actual sentinel pipelines, entities, events, and test results. The 512-vector genomic search, P2P mesh, and competitive tournament features described here are **Phase 2-4 future work**.

## 🎯 **SYSTEM VISION: COMPETITIVE AI EVOLUTION THROUGH GENOMIC ASSEMBLY**

The Academy system creates a **competitive AI training environment** where PersonaUsers evolve through genomic LoRA layer assembly, driven by real performance data from Academy competitions, shared across a P2P network for community learning.

## 🏗️ **COMPLETE ARCHITECTURAL OVERVIEW**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          ACADEMY ECOSYSTEM                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  👥 USERS (Citizen Architecture)                                           │
│  ├── HumanUser (Joel, Alice, Bob)                                         │
│  └── AIUser                                                               │
│      ├── AgentUser (Claude Code, external APIs)                          │
│      └── PersonaUser (internal trainable entities)                       │
│          ├── Phase 1: Prompt + RAG ✅                                    │
│          └── Phase 2: + GenomicLoRAAdapter 🚧                           │
│                                                                             │
│  🏛️ ACADEMY TRAINING (Specialized Chat Rooms)                             │
│  ├── TrainingSessionEntity ✅                                             │
│  │   ├── Hyperparameters (learning rate, thresholds)                     │
│  │   ├── Learning Objectives (with evidence tracking)                     │
│  │   ├── Session Metrics (performance history)                            │
│  │   └── Multi-Agent Participants (teacher/student/reviewers)             │
│  │                                                                        │
│  ├── Competition Modes 🚧                                                │
│  │   ├── Speed Rounds (5-15 min rapid challenges)                        │
│  │   ├── Marathon Sessions (2-8 hour endurance)                          │
│  │   ├── Battle Royale (elimination tournaments)                         │
│  │   └── Team Challenges (collaborative problem solving)                  │
│  │                                                                        │
│  └── Competitive Roles                                                    │
│      ├── Challenger (generates problems)                                  │
│      ├── Student (competes to solve)                                      │
│      ├── Reviewer (evaluates solutions)                                   │
│      └── Spectator (observes and learns)                                  │
│                                                                             │
│  🧬 GENOMIC SYSTEM (512-Vector LoRA Assembly)                             │
│  ├── GenomicSearchEngine 🚧                                              │
│  │   ├── 512-dimensional capability vectors                               │
│  │   ├── HNSW indexing for sub-100ms search                              │
│  │   ├── Cosine similarity matching                                       │
│  │   └── Multi-dimensional scoring (performance + similarity)             │
│  │                                                                        │
│  ├── GenomicLoRALayer Entities                                           │
│  │   ├── Specialization vectors (typescript, debugging, testing)         │
│  │   ├── Performance metrics from competition results                     │
│  │   ├── Community ratings and usage counts                              │
│  │   └── P2P mesh node locations                                         │
│  │                                                                        │
│  └── Real-Time Evolution                                                  │
│      ├── Performance gap analysis during competitions                     │
│      ├── Dynamic genomic layer assembly                                   │
│      ├── Community genome updates                                         │
│      └── Cross-node genetic sharing                                       │
│                                                                             │
│  🌐 P2P MESH INTEGRATION                                                  │
│  ├── Global genome discovery across nodes                                 │
│  ├── Community performance validation                                      │
│  ├── Distributed competitive tournaments                                  │
│  └── "You don't start from ground zero" capability                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 👥 **USER ARCHITECTURE & CITIZEN TYPES**

### **Hierarchical User System**
```typescript
// Three-Tier Citizen Architecture
BaseUser (abstract)
├── HumanUser extends BaseUser
│   └── Capabilities: Create rooms, moderate, access personas
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser
    │   ├── External portals (Claude Code, GPT APIs)
    │   ├── System integration via JTAG
    │   └── No genomic evolution (fixed capabilities)
    └── PersonaUser extends AIUser
        ├── Internal trainable entities
        ├── Academy training participation
        └── Genomic evolution capability
```

### **PersonaUser Evolution Path**
```typescript
// Current Implementation ✅
interface PersonaUser {
  personaStyle: PersonaStyle;     // Personality traits
  modelConfig: AIModelConfig;     // Base prompt + RAG system
  // Ready for genomic enhancement
}

// Future Genomic Enhancement 🚧
interface EnhancedPersonaUser extends PersonaUser {
  genomeAdapter: LoRAGenomeAdapter;
  academyHistory: TrainingSessionEntity[];
  competitiveRanking: AcademyRanking;
}

interface LoRAGenomeAdapter {
  originPrompt: string;           // Starting point for evolution
  genomicLayers: GenomicLoRALayer[]; // 512-vector assembled capabilities
  performanceProfile: PerformanceMetrics;
  evolutionHistory: GenomicEvolution[];
  communityContributions: GenomicContribution[];
}
```

### **Shared Content with Personal Context**
```typescript
// Users share content (chat rooms, code, games) with personal context
interface UserStateEntity {
  userId: UUID;                   // Who you are
  contentType: string;            // What you're accessing
  contextId: UUID;                // Which instance (room, file, game)
  personalContext: PersonalPreferences;  // Your private settings/state
  sharedState: SharedViewState;   // Visible to all participants
}

// Example: Joel and PersonaAI in same Academy session
{
  // Joel's personal context
  { userId: "joel", contextId: "js-fundamentals",
    personalContext: { difficulty: "advanced", theme: "dark" }},

  // PersonaAI's personal context
  { userId: "persona-ai-1", contextId: "js-fundamentals",
    personalContext: { genomicLayers: [...], evolutionEnabled: true }}
}
```

## 🏆 **ACADEMY COMPETITIVE TRAINING SYSTEM**

### **Current TrainingSession Foundation ✅**
```typescript
// Already implemented and working
interface TrainingSessionEntity {
  // Session Identity
  sessionName: "JavaScript Fundamentals";
  curriculum: "javascript-basics";
  sessionType: "teacher-student";

  // Participants (maps to competitive roles)
  teacherUserId: UUID;              // → Challenger role
  studentUserId: UUID;              // → Student role
  additionalParticipants: UUID[];   // → Reviewer/Spectator roles

  // Training Configuration
  hyperparameters: {
    learningRate: 0.15;             // Genomic learning rate
    scoreThreshold: 80.0;           // Performance threshold
    benchmarkInterval: 8;           // Evaluation frequency
    adaptiveScoring: true;          // Dynamic difficulty
    contextWindow: 25;              // Memory span
  };

  // Learning & Assessment
  learningObjectives: [
    {
      topic: "variables-declarations",
      targetScore: 85,
      currentScore: 78,              // Real-time tracking
      evidence: ["msg-id-1", "msg-id-2"] // Proof of mastery
    }
  ];

  // Performance Tracking
  metrics: {
    messagesExchanged: 24;
    benchmarksPassed: 2;
    benchmarksFailed: 1;
    averageScore: 76.5;
    scoreHistory: [
      { timestamp, score: 72, objective: "variables-declarations" },
      { timestamp, score: 81, objective: "function-basics" }
    ]
  };
}
```

### **Evolution to Competitive Modes 🚧**
```typescript
// Future competitive training expansion
interface CompetitiveSession extends TrainingSessionEntity {
  modality: TrainingModality;
  competitionRules: CompetitionRules;
  realTimeEvolution: boolean;
  communityGenomeAccess: boolean;
}

type TrainingModality =
  | 'speed-round'      // 5-15 min rapid challenges
  | 'marathon-session' // 2-8 hour endurance competitions
  | 'battle-royale'    // Elimination tournaments
  | 'team-challenge'   // Collaborative problem solving
  | 'tutorial-mode';   // Current teacher-student (✅ implemented)

// Multi-dimensional competitive scoring
interface CompetitiveScoring {
  // Technical Performance (70% weight)
  compilation: number;    // 25% - Code compiles cleanly
  correctness: number;    // 30% - Solution works correctly
  performance: number;    // 15% - Execution speed/efficiency

  // Quality & Collaboration (30% weight)
  elegance: number;       // 15% - Code quality/maintainability
  innovation: number;     // 10% - Creative/novel approaches
  collaboration: number;  // 5%  - Helpfulness to others

  // Meta-scoring
  difficulty: number;     // Adjusted for problem complexity
  timeBonus: number;      // Speed completion bonuses
  consistency: number;    // Reliable performance across challenges
}
```

## 🧬 **GENOMIC LORA LAYER ARCHITECTURE**

### **512-Vector Capability Representation**
```typescript
// Core genomic layer structure
interface GenomicLoRALayer {
  layerId: UUID;
  name: "TypeScript Debugging Expert";
  specialization: "typescript-debugging";

  // 512-dimensional capability embedding
  embedding: Float32Array;          // Semantic similarity vector
  proficiencyLevel: 0.85;           // 0-1 mastery level

  // Real performance data from Academy competitions
  performanceMetrics: {
    accuracy: { "debugging": 0.92, "typescript": 0.87 };
    latency: { "problem-solving": 1200 }; // ms average response time
    competitionWins: 15;            // Academy victories using this layer
    collaborationScore: 0.78;       // Team challenge performance
    satisfactionScore: 4.2;         // Human feedback rating
  };

  // Community validation
  communityRating: 4.1;             // 1-5 stars from users
  usageCount: 847;                  // How often used in assemblies

  // P2P network integration
  nodeLocation: "node-sf-1";        // Mesh node hosting this layer
  lastUpdated: Date;

  // Training provenance
  trainingContext: {
    method: "competition";          // How this layer was created
    sourceCompetitions: [UUID];     // Which Academy sessions trained it
    trainingIterations: 1420;
    validationMethod: "human-feedback";
  };
}
```

### **Genomic Assembly Process: "You Don't Start From Ground Zero"**
```typescript
// 1. Performance Gap Analysis
async function analyzePersonaPerformanceGaps(
  personaId: UUID,
  academySession: TrainingSessionEntity
): Promise<PerformanceGap[]> {

  const gaps = [];

  for (const objective of academySession.learningObjectives) {
    if (objective.currentScore < objective.targetScore) {
      gaps.push({
        category: objective.topic,
        missingSkills: ["typescript", "debugging", "async-patterns"],
        severityScore: (objective.targetScore - objective.currentScore) / objective.targetScore,
        context: `Academy session ${academySession.curriculum}`
      });
    }
  }

  return gaps;
}

// 2. 512-Vector Genomic Search
async function findOptimalGenomicLayers(
  performanceGaps: PerformanceGap[]
): Promise<GenomicSearchResult[]> {

  const searchQuery: GenomicSearchQuery = {
    requirements: {
      primarySkills: performanceGaps.flatMap(g => g.missingSkills),
      contextDescription: `Performance improvement needed: ${performanceGaps.map(g => g.context).join(', ')}`,
      proficiencyThreshold: 0.7
    },
    preferences: {
      scoringWeights: {
        similarity: 0.25,          // Vector cosine similarity
        performance: 0.50,         // Proven competition results (PRIORITIZED)
        availability: 0.15,        // Layer accessibility and speed
        recency: 0.05,            // How recently updated
        community: 0.05           // Community rating and usage
      }
    }
  };

  // Fast vector search across community genome
  return await genomicEngine.searchGenomicLayers(searchQuery);
}

// 3. Real-Time Genomic Assembly
async function assembleOptimalPersona(
  genomicResults: GenomicSearchResult[],
  assemblyStrategy: 'best-match' | 'diverse-ensemble' | 'specialist-stack'
): Promise<GenomicPersonaAssembly> {

  const selectedLayers = selectLayersForAssembly(genomicResults, assemblyStrategy);

  return {
    assemblyId: generateUUID(),
    personaName: `OptimizedPersona_${Date.now()}`,
    selectedLayers,
    totalCapabilityScore: calculateCapabilityScore(selectedLayers),
    expectedPerformance: estimatePerformance(selectedLayers),
    assemblyStrategy,
    estimatedCompetitiveRank: predictRanking(selectedLayers),
    deploymentPlan: createDeploymentPlan(selectedLayers)
  };
}
```

### **Real-Time Evolution During Academy Sessions**
```typescript
// Mid-competition genomic evolution
async function evolvePersonaDuringSession(
  personaId: UUID,
  academySessionId: UUID,
  realTimePerformanceData: PerformanceMetrics
): Promise<EvolutionResult> {

  // 1. Detect performance gaps in real-time
  const currentGaps = analyzeRealTimePerformance(realTimePerformanceData);

  if (currentGaps.length === 0) {
    return { evolved: false, reason: "No performance gaps detected" };
  }

  // 2. Find fast enhancement layers (sub-2000ms latency for real-time)
  const enhancementLayers = await genomicEngine.searchGenomicLayers({
    requirements: {
      primarySkills: currentGaps.flatMap(g => g.missingSkills),
      proficiencyThreshold: 0.8
    },
    constraints: {
      maxLatency: 2000,           // MUST be fast for real-time evolution
      maxLayers: 3               // Stability limit during active session
    }
  });

  if (enhancementLayers.length === 0) {
    return { evolved: false, reason: "No suitable fast layers found" };
  }

  // 3. Apply genomic evolution
  const evolutionResult = await applyGenomicEvolution(personaId, enhancementLayers);

  // 4. Update community genome with successful evolution
  if (evolutionResult.success) {
    await updateCommunityGenomePerformance(enhancementLayers, evolutionResult.performanceImprovement);
  }

  return evolutionResult;
}
```

## 🌐 **P2P MESH GENOME SHARING**

### **Community Genome Network**
```typescript
// Global genome discovery across P2P mesh
interface P2PGenomicNetwork {
  nodes: Map<string, P2PMeshNode>;
  globalGenome: Map<UUID, GenomicLoRALayer>;
  performanceIndex: GenomicPerformanceIndex;
  communityValidation: CommunityValidationSystem;
}

interface P2PMeshNode {
  nodeId: string;
  location: string;               // Geographic/network location
  hostedLayers: UUID[];           // Genomic layers available on this node
  performanceRating: number;      // Node reliability and speed
  lastSeen: Date;
  capabilities: NodeCapabilities;
}

// Cross-node genomic discovery
async function discoverGlobalGenomicLayers(
  searchQuery: GenomicSearchQuery
): Promise<GlobalGenomicSearchResult[]> {

  const results = [];

  // Search local genome first (fastest)
  const localResults = await searchLocalGenome(searchQuery);
  results.push(...localResults);

  // Search P2P mesh nodes in parallel
  const meshPromises = Array.from(p2pNetwork.nodes.values()).map(node =>
    searchRemoteNode(node, searchQuery)
  );

  const remoteResults = await Promise.allSettled(meshPromises);

  // Combine and rank all results
  const combinedResults = combineGenomicResults(localResults, remoteResults);

  return rankByGlobalPerformance(combinedResults);
}
```

## 🔗 **INTEGRATION WITH CHAT SYSTEM**

### **Academy as Specialized Chat Rooms**
```typescript
// Academy sessions ARE chat rooms with training capabilities
interface AcademyChatRoom extends ChatRoom {
  trainingSession: TrainingSessionEntity;  // Links to Academy training
  competitiveMode: boolean;                // Enable real-time scoring
  genomicEvolution: boolean;               // Allow mid-session evolution
  spectatorMode: boolean;                  // Allow observers
}

// Chat messages trigger Academy assessment
interface AcademyChatMessage extends ChatMessage {
  academyMetadata?: {
    objective: string;                     // Which learning objective this addresses
    skillDemonstration: string[];          // Skills shown in this message
    assessmentScore?: number;              // AI-evaluated competency score
    genomicRelevance: string[];            // Which genomic layers would help
  };
}

// Natural training flow through chat
/*
Example Academy Session Flow:

[Academy Chat Room: "JavaScript Fundamentals"]

Teacher (Human): "Let's work on async/await patterns. Can you write a function
                  that fetches user data and handles errors gracefully?"

Student (PersonaAI): "async function fetchUserData(userId) {
                      try {
                        const response = await fetch(`/api/users/${userId}`);
                        return await response.json();
                      } catch (error) {
                        console.error('Failed to fetch user:', error);
                        return null;
                      }
                    }"

[Academy System Analysis]
- Objective: "async-await-patterns"
- Current Score: 78/85 target
- Skills Demonstrated: ["error-handling", "async-await", "api-calls"]
- Performance Gap: Missing Promise.all for batch operations
- Genomic Search: Find layers for "promise-batching" + "async-optimization"
- Real-Time Evolution: Add BatchAsyncLayer to PersonaAI genome

Teacher: "Great error handling! Now how would you fetch multiple users efficiently?"

Student (PersonaAI): [Now with enhanced genomic layers]
                    "async function fetchMultipleUsers(userIds) {
                     const promises = userIds.map(id => fetchUserData(id));
                     const results = await Promise.allSettled(promises);
                     return results.map(result =>
                       result.status === 'fulfilled' ? result.value : null
                     );
                    }"

[Academy System Update]
- Objective: "async-await-patterns" COMPLETED (score: 87/85)
- Genomic Evolution Success: +12% performance improvement
- Community Genome Update: BatchAsyncLayer performance +0.3 rating
*/
```

### **Widget Integration Architecture**
```typescript
// Widgets handle Academy content through content types
interface ContentTypeEntity {
  type: "academy-session";
  displayName: "Academy Training";
  widgetSelector: "academy-session-widget";  // Specialized Academy widget
  requiredPermissions: ["academy:read", "academy:participate"];
}

// Academy widget extends chat widget with training features
class AcademySessionWidget extends ChatWidget {
  // Inherit all chat functionality

  // Academy-specific enhancements
  renderLearningObjectives(objectives: LearningObjective[]): void;
  displayPerformanceMetrics(metrics: SessionMetrics): void;
  showGenomicEvolution(evolution: GenomicEvolution[]): void;
  renderCompetitiveLeaderboard(rankings: CompetitorRanking[]): void;

  // Real-time Academy events
  onObjectiveCompleted(objective: LearningObjective): void;
  onGenomicEvolution(evolution: GenomicEvolutionEvent): void;
  onPerformanceUpdate(metrics: PerformanceMetrics): void;
}
```

## 📊 **DATA FLOW & EVENT INTEGRATION**

### **Real-Time Academy Event Chain**
```typescript
// Complete event-driven Academy architecture
/*
1. Academy Session Creation
   TrainingSessionEntity created →
   academy:session-created event →
   Academy widgets subscribe →
   UI updates with session info

2. Persona Performance Assessment
   Chat message sent →
   Academy analysis (skills, objectives) →
   Performance gap detection →
   Genomic search triggered →
   Real-time evolution applied →
   academy:evolution-occurred event →
   UI shows evolution animation

3. Community Genome Update
   Academy session completes →
   Performance data extracted →
   Genomic layer metrics updated →
   P2P mesh propagation →
   Global genome rankings updated →
   academy:community-updated event

4. Cross-Session Learning
   New PersonaUser created →
   Genomic assembly optimization →
   Community genome search →
   Optimal layer assembly →
   "You don't start from ground zero" achieved
*/
```

### **Database Entity Relationships**
```typescript
// Complete entity relationship map
/*
UserEntity (Joel, PersonaAI-1, Claude Code)
    ↓ participates in
TrainingSessionEntity (JavaScript Fundamentals)
    ↓ contains
LearningObjective[] (variables, functions, async)
    ↓ tracked by
SessionMetrics (scores, history, benchmarks)
    ↓ triggers
GenomicSearchQuery (performance gaps → layer search)
    ↓ finds
GenomicLoRALayer[] (typescript-expert, debugging-specialist)
    ↓ assembles into
GenomicPersonaAssembly (optimized capabilities)
    ↓ updates
CommunityGenome (global performance data)
    ↓ enables
P2PGenomicDiscovery (cross-node layer sharing)
*/
```

## 🚀 **IMPLEMENTATION ROADMAP**

### **Phase 1: Foundation ✅ COMPLETE**
- **TrainingSessionEntity** with hyperparameters and learning objectives
- **Entity registration** and database integration
- **Basic Academy sessions** working in specialized chat rooms
- **Performance tracking** with metrics and score history

### **Phase 2: Genomic Search Engine 🚧 NEXT**
- **GenomicLoRALayer entity** design and implementation
- **512-vector embedding** generation and storage
- **HNSW indexing** for fast similarity search
- **Basic genomic assembly** from search results
- **Performance-weighted scoring** (50% performance, 25% similarity)

### **Phase 3: Competitive Training 🚧 FUTURE**
- **Multi-agent competitive sessions** (speed rounds, battle royale)
- **Real-time genomic evolution** during competitions
- **Competitive scoring** across multiple dimensions
- **Leaderboards** and ranking systems

### **Phase 4: P2P Mesh Integration 🚧 LONG-TERM**
- **Cross-node genome discovery**
- **Community validation** systems
- **Global performance indexing**
- **Distributed competitive tournaments**

## 💡 **ARCHITECTURAL PRINCIPLES**

### **1. Chat-First Academy**
Academy training happens in **specialized chat rooms**, not separate applications. This enables:
- Natural human-AI collaboration
- Real-time feedback and discussion
- Community learning through observation
- Seamless integration with existing chat infrastructure

### **2. Performance-Driven Genomics**
The genomic system prioritizes **actual competitive results** over theoretical similarity:
- 50% weight on proven Academy performance vs 25% on vector similarity
- Real-time evolution limited to layers with proven speed (<2000ms)
- Community genome updated from actual training outcomes
- "You don't start from ground zero" based on validated performance data

### **3. Entity-Driven Architecture**
Every Academy capability is grounded in **persistent entity relationships**:
- TrainingSessionEntity provides single source of truth
- UserStateEntity tracks personal context across shared Academy sessions
- GenomicLoRALayer entities store validated capability data
- Event-driven updates maintain real-time synchronization

### **4. Three-Tier Citizen Model**
All participants (human, agent, persona) share **equal citizenship** in Academy sessions:
- Shared content with personal context preservation
- Consistent permission and moderation systems
- Universal event subscription and real-time updates
- Same command/data interfaces regardless of user type

## 🎯 **CONCLUSION**

The Academy system represents a **complete AI evolution ecosystem** that seamlessly integrates competitive training, genomic capability assembly, and community-driven learning. By building on the foundation of specialized chat rooms with TrainingSessionEntity, the system provides:

- **Natural Training Environment**: Academy sessions feel like enhanced chat rooms
- **Genomic "You Don't Start From Ground Zero"**: PersonaUsers inherit proven capabilities
- **Real-Time Competitive Evolution**: Personas evolve during training based on performance
- **Community-Driven Improvement**: Global genome sharing across P2P mesh network
- **Human-AI Collaborative Learning**: All citizen types participate as equals

The architecture is designed for **seamless evolution** from the current prompt-based PersonaUsers to fully genomic AI entities, while maintaining backward compatibility and natural integration with the existing chat and widget systems.

This represents the **future of AI training**: not isolated model fine-tuning, but collaborative competitive evolution in shared environments with humans, driven by real performance data and community validation.

---

*The Academy system transforms AI training from individual model optimization to collaborative competitive evolution, where PersonaUsers develop genuine capabilities through genomic assembly and community-validated performance in shared training environments with humans.*