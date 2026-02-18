# AI Infrastructure Dashboard - TensorBoard for JTAG

**Vision**: A TensorBoard-inspired real-time monitoring and control panel for AI adapter testing, training, and deployment across all providers (Ollama, OpenAI, Anthropic, Together, Fireworks, etc.).

---

## 🎯 Core Philosophy

### **Entity-First Architecture**
- **Entities** = Stateful data (TestExecutionEntity, TrainingJobEntity, ModelEntity)
- **Events** = Real-time updates (progress, completion, errors)
- **Commands** = Actions (start test, query status, deploy model)
- **Widgets** = Interactive UI (charts, tables, controls)

### **Universal Pattern**
```typescript
// Same code works in browser, server, CLI, tests
const result = await Commands.execute('ai/adapter/test', {
  adapter: 'ollama',
  async: true
});

// Subscribe to real-time updates
Events.subscribe('data:test_executions:progress', (event) => {
  updateUI(event.progress);
});
```

### **TensorBoard Inspiration**
- **Real-time monitoring** - Live loss curves, metrics, system stats
- **Interactive exploration** - Zoom, pan, filter, drill-down
- **Comparative analysis** - Side-by-side provider comparison
- **Historical tracking** - Performance trends over time
- **Reproducibility** - Full audit trail in entities

---

## 📊 System Architecture

### **Phase 1: Diagnostic Foundation** (Current)

```
User Request
    ↓
Command (ai/adapter/test)
    ↓
Create TestExecutionEntity → Database
    ↓
Start Async Work → Background Thread
    ↓                      ↓
Return UUID Handle    Emit Progress Events
    ↓                      ↓
User monitors         Widgets Update in Real-Time
```

### **Phase 2-6: Full Infrastructure**

```
┌─────────────────────────────────────────────────────────┐
│                   AI Infrastructure Dashboard            │
│                                                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Test Runner  │  │ Training Mgr │  │ Model Deploy │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                           │
│  ┌──────────────────────────────────────────────────┐   │
│  │           Real-Time Visualizations               │   │
│  │  • Loss Curves       • Heatmaps                  │   │
│  │  • Distributions     • System Metrics            │   │
│  │  • Comparative Charts • Logs                     │   │
│  └──────────────────────────────────────────────────┘   │
│                                                           │
│  ┌──────────────────────────────────────────────────┐   │
│  │           Entity Store (Database)                │   │
│  │  • TestExecutionEntity                           │   │
│  │  • TrainingJobEntity                             │   │
│  │  • ModelEntity                                   │   │
│  │  • CapabilityMatrixEntity                        │   │
│  │  • BenchmarkEntity                               │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

---

## 🗂️ Entity Definitions

See detailed entity schemas in:
- [AI-TESTING-ENTITIES.md](./AI-TESTING-ENTITIES.md) - TestExecutionEntity, BenchmarkEntity
- [AI-TRAINING-ENTITIES.md](./AI-TRAINING-ENTITIES.md) - TrainingJobEntity, CheckpointEntity
- [AI-MODEL-ENTITIES.md](./AI-MODEL-ENTITIES.md) - ModelEntity, DeploymentEntity
- [AI-CAPABILITY-ENTITIES.md](./AI-CAPABILITY-ENTITIES.md) - CapabilityMatrixEntity

### **Quick Overview**

```typescript
// TestExecutionEntity - Diagnostic test runs
interface TestExecutionEntity {
  testId: UUID;
  status: 'queued' | 'running' | 'completed' | 'failed';
  progress: number;  // 0-100
  metrics: { timestamps, latencies, throughput, errorRates };
  results?: AdapterTestResult;
  logs: LogEntry[];
}

// TrainingJobEntity - Fine-tuning jobs (TensorBoard-style)
interface TrainingJobEntity {
  jobId: UUID;
  status: 'training' | 'completed' | 'failed';
  metrics: {
    steps: number[];
    trainingLoss: number[];
    validationLoss: number[];
    learningRate: number[];
  };
  checkpoints: CheckpointInfo[];
  system: { cpuPercent, memoryMB, gpuPercent };
}

// ModelEntity - Trained model artifacts
interface ModelEntity {
  modelId: UUID;
  skillName: string;
  version: string;  // "v1", "v2", etc.
  deployed: boolean;
  deployedTo: UUID[];  // PersonaUser IDs
  artifactPath: string;
  benchmarks: { latency, throughput, quality };
}

// CapabilityMatrixEntity - Provider scorecard
interface CapabilityMatrixEntity {
  matrix: Record<adapter, Record<capability, {
    supported: boolean;
    avgLatency: number;
    p95Latency: number;
    errorRate: number;
    availability: number;
  }>>;
  trends: HistoricalPerformance[];
}
```

---

## 🎮 Command Suite

### **Phase 1: Self-Diagnostic** (Foundation - COMPLETED ✅)

```bash
# Start test (async)
./jtag ai/adapter/test --adapter=ollama --async
# Returns: { testId: "uuid", status: "queued" }

# Monitor status (NEEDED)
./jtag ai/adapter/test/status --testId=<uuid>

# Get results (NEEDED)
./jtag ai/adapter/test/results --testId=<uuid>
```

**Implementation Status**:
- ✅ Command structure created (AdapterTestServerCommand.ts)
- ✅ Types defined (AdapterTestTypes.ts)
- ✅ Command registered (75 commands)
- ⚠️ NEEDS: Async pattern with UUID handle
- ⚠️ NEEDS: Status/results commands
- ⚠️ NEEDS: TestExecutionEntity

### **Phase 2: Capability Matrix**

```bash
# Generate compatibility matrix
./jtag ai/adapter/matrix --output=json|table

# Example output:
# ┌──────────┬────────────┬────────────┬──────────┐
# │ Adapter  │ Text-Gen   │ Embeddings │ Images   │
# ├──────────┼────────────┼────────────┼──────────┤
# │ ollama   │ ✅ (342ms) │ ✅ (89ms)  │ ❌       │
# │ openai   │ ✅ (156ms) │ ✅ (45ms)  │ ✅ (2.1s)│
# │ anthropic│ ✅ (234ms) │ ❌         │ ❌       │
# └──────────┴────────────┴────────────┴──────────┘

# Query capabilities
./jtag ai/adapter/find --capability=embeddings --minPerformance=100ms
```

**Benefits**: Instant visibility, performance benchmarking, smart provider selection

### **Phase 3: Fine-Tuning Validation**

```bash
# Validate training support
./jtag ai/adapter/validate-training --adapter=ollama --model=llama3

# Test training pipeline (dry-run)
./jtag ai/adapter/test-training-pipeline \
  --adapter=ollama \
  --model=llama3 \
  --datasetSize=100 \
  --dryRun=true
```

**Benefits**: Catch issues before expensive training, estimate costs/time

### **Phase 4: Comparative Benchmarking**

```bash
# Run same prompt across adapters
./jtag ai/adapter/benchmark \
  --prompt="Explain quantum entanglement" \
  --adapters=ollama,openai,anthropic \
  --metrics=latency,quality,cost

# Historical benchmarks
./jtag ai/adapter/benchmark/history --adapter=ollama --last=30days
```

**Benefits**: Data-driven provider selection, track degradation, cost optimization

### **Phase 5: Training Job Management**

```bash
# Start fine-tuning job
./jtag ai/train/start \
  --adapter=ollama \
  --model=llama3 \
  --dataset=./training-data.jsonl \
  --skillName=typescript-expert \
  --epochs=3
# Returns: { jobId: "uuid", status: "queued" }

# Monitor training (real-time)
./jtag ai/train/status --jobId=<uuid>
./jtag ai/train/watch --jobId=<uuid>  # Live updates

# Job control
./jtag ai/train/list --status=running
./jtag ai/train/cancel --jobId=<uuid>
./jtag ai/train/resume --jobId=<uuid> --checkpoint=epoch-2
```

**Benefits**: Async training, progress monitoring, checkpoint management

### **Phase 6: Model Registry**

```bash
# List trained models
./jtag ai/model/list --adapter=ollama --type=fine-tuned

# Deploy to PersonaGenome
./jtag ai/model/deploy \
  --modelId=typescript-expert-v1 \
  --persona=helper-ai \
  --domain=code

# Test fine-tuned vs baseline
./jtag ai/model/test \
  --modelId=typescript-expert-v1 \
  --baseline=llama3

# Version management
./jtag ai/model/versions --skillName=typescript-expert
./jtag ai/model/rollback --persona=helper-ai --version=v2
```

**Benefits**: Model versioning, A/B testing, easy rollbacks

---

## 🎨 Widget Architecture

### **Main Dashboard** (Full-Page Control Panel)

```
┌─────────────────────────────────────────────────────────────┐
│  AI Infrastructure Dashboard                        [⟳ Live] │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Active Tests (3)              Training Jobs (1)             │
│  ┌──────────────────┐         ┌──────────────────┐          │
│  │ Ollama Test      │         │ TypeScript Expert│          │
│  │ ████████░░  82%  │         │ ███████░░░  65%  │          │
│  │ Testing embed... │         │ Epoch 2/3        │          │
│  └──────────────────┘         │ Loss: 1.234      │          │
│                                └──────────────────┘          │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ Real-Time Loss Curves                                │   │
│  │    2.5 ┤                                              │   │
│  │    2.0 ┤╮                                             │   │
│  │    1.5 ┤ ╰╮                                           │   │
│  │    1.0 ┤   ╰─╮                                        │   │
│  │    0.5 ┤     ╰────────                                │   │
│  │    0.0 └──────────────────────────────────────────   │   │
│  │         0    50   100   150   200  Steps              │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  Capability Matrix                Model Registry             │
│  ┌────────────────┐              ┌────────────────┐         │
│  │     Text Embed │              │ typescript-v2  │         │
│  │ olla  ✅   ✅  │              │ code-review-v1 │         │
│  │ open  ✅   ✅  │              │ chat-expert-v3 │         │
│  │ anth  ✅   ❌  │              └────────────────┘         │
│  └────────────────┘                                         │
└─────────────────────────────────────────────────────────────┘
```

See detailed widget specifications in:
- [AI-DASHBOARD-WIDGETS.md](./AI-DASHBOARD-WIDGETS.md)
- [AI-TRAINING-VISUALIZATIONS.md](./AI-TRAINING-VISUALIZATIONS.md)
- [AI-GENOME-DESIGNER.md](./AI-GENOME-DESIGNER.md) - Three.js visual genome editor

### **Widget Components**

1. **AIInfrastructureDashboardWidget** - Main control panel
2. **TestExecutionWidget** - Individual test monitoring
3. **TrainingVisualizerWidget** - TensorBoard-style loss curves
4. **CapabilityHeatmapWidget** - Provider comparison matrix
5. **ModelRegistryWidget** - Deployed model management
6. **SystemMetricsWidget** - CPU/GPU/memory monitoring
7. **LogViewerWidget** - Searchable, filterable logs
8. **BenchmarkComparatorWidget** - Side-by-side provider comparison
9. **GenomeDesignerWidget** - Three.js visual LoRA editor (Phase 7+)

### **Three.js Genome Designer Concept**

```
┌─────────────────────────────────────────────────────────────┐
│  Genome Designer - typescript-expert-v2              [3D/2D] │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│         ┌────────────────────────────────────┐              │
│         │   Three.js Scene (GPU-Accelerated) │              │
│         │                                     │              │
│         │     ●────●────●────●────●          │              │
│         │    /│\  /│\  /│\  /│\  /│\         │              │
│         │   ● ● ●● ● ●● ● ●● ● ●● ● ●        │              │
│         │    Base Model Layers               │              │
│         │                                     │              │
│         │           ↓ LoRA Adapters ↓        │              │
│         │     ╔════╗  ╔════╗  ╔════╗         │              │
│         │     ║ TS ║  ║Code║  ║Doc ║         │              │
│         │     ╚════╝  ╚════╝  ╚════╝         │              │
│         │    (Active) (Paged) (Paged)        │              │
│         │                                     │              │
│         └────────────────────────────────────┘              │
│                                                               │
│  Skills:                      Memory Usage:                  │
│  ┌──────────────────┐        ┌──────────────────┐          │
│  │ ✅ typescript (v2)│        │ GPU: ████░░ 62%  │          │
│  │ □  code-review    │        │ RAM: ███░░░ 45%  │          │
│  │ □  documentation  │        └──────────────────┘          │
│  └──────────────────┘                                        │
│                                                               │
│  [Activate Skill] [Evict LRU] [Train New]                   │
└─────────────────────────────────────────────────────────────┘
```

**Three.js Features**:
- **3D Network Visualization** - Neural network layers as 3D graph
- **LoRA Adapter Nodes** - Draggable, color-coded by domain
- **Real-time Activation** - Highlight active adapters (GPU memory)
- **LRU Animation** - Fade out/paging animation for evicted adapters
- **Performance Overlay** - Real-time inference latency heatmap
- **Interactive Training** - Click adapter to train, see loss curve overlay
- **Skill Composition** - Drag-connect multiple adapters for multi-skill inference

---

## 🔄 Real-Time Event Flow

### **Event Naming Convention**
```
data:<collection>:<action>

Examples:
- data:test_executions:started
- data:test_executions:progress
- data:test_executions:completed
- data:training_jobs:epoch_complete
- data:models:deployed
```

### **Server → Client Flow**

```typescript
// Server: Emit progress during training
private async trainModelWithUpdates(jobId: UUID) {
  for (let step = 0; step < totalSteps; step++) {
    const loss = await this.trainingStep();

    // Emit every 10 steps
    if (step % 10 === 0) {
      Events.emit('data:training_jobs:progress', {
        jobId,
        step,
        metrics: { trainingLoss: loss, learningRate: currentLR }
      });

      // Update entity in database
      await Commands.execute('data/update', {
        collection: 'training_jobs',
        id: jobId,
        updates: { 'metrics.steps': { $push: step } }
      });
    }
  }
}

// Browser: Widget auto-updates
Events.subscribe('data:training_jobs:progress', (event) => {
  const widget = document.querySelector(`[data-job-id="${event.jobId}"]`);
  widget.updateLossCurve(event.metrics);
  widget.updateProgress(event.step);
});
```

---

## 📋 Implementation Roadmap

### **Immediate: Phase 1 Completion** (Make Current Command Usable)

**Goal**: Add async pattern to `ai/adapter/test` command

**Tasks**:
1. ✅ Command structure (DONE)
2. ⚠️ Create TestExecutionEntity
3. ⚠️ Add to EntityRegistry
4. ⚠️ Refactor command to return UUID immediately
5. ⚠️ Run tests in background with progress events
6. ⚠️ Implement `ai/adapter/test/status` command
7. ⚠️ Implement `ai/adapter/test/results` command
8. ⚠️ Add basic TestExecutionWidget

**Estimated Time**: 4-6 hours
**Blocking**: None - foundation for everything else

### **Phase 2: Capability Matrix** (High Value, Low Complexity)

**Goal**: Provider comparison and capability discovery

**Tasks**:
1. Create CapabilityMatrixEntity
2. Implement `ai/adapter/matrix` command
3. Implement `ai/adapter/find` command
4. Build CapabilityHeatmapWidget
5. Cache matrix data with expiry

**Estimated Time**: 3-4 hours
**Blocking**: Phase 1 (uses same test infrastructure)

### **Phase 3: Training Job Management** (Core for Phase 7)

**Goal**: Async fine-tuning with monitoring

**Tasks**:
1. Create TrainingJobEntity
2. Create CheckpointEntity
3. Implement `ai/train/start` command
4. Implement `ai/train/status` command
5. Implement `ai/train/watch` command (CLI streaming)
6. Build TrainingVisualizerWidget
7. Add checkpoint management

**Estimated Time**: 8-12 hours
**Blocking**: Real Ollama integration (currently stubs)

### **Phase 4: Model Registry** (Enables Production Use)

**Goal**: Deploy and manage trained models

**Tasks**:
1. Create ModelEntity
2. Create DeploymentEntity
3. Implement `ai/model/deploy` command
4. Implement `ai/model/list` command
5. Implement `ai/model/rollback` command
6. Build ModelRegistryWidget
7. Integrate with PersonaGenome

**Estimated Time**: 6-8 hours
**Blocking**: Phase 3 (needs trained models)

### **Phase 5: Full Dashboard** (TensorBoard Experience)

**Goal**: Interactive monitoring control panel

**Tasks**:
1. Build AIInfrastructureDashboardWidget
2. Integrate all sub-widgets
3. Add tab navigation (Overview, Loss Curves, Scalars, etc.)
4. Implement Chart.js/D3.js visualizations
5. Add real-time auto-refresh
6. Create dashboard route

**Estimated Time**: 10-15 hours
**Blocking**: Phases 1-4 (needs all entities/commands)

### **Phase 6: Genome Designer** (Advanced - Three.js)

**Goal**: Visual LoRA genome editor

**Tasks**:
1. Create GenomeDesignerWidget
2. Integrate Three.js
3. Build 3D network visualization
4. Add LoRA adapter nodes
5. Implement drag-and-drop skill composition
6. Add performance heatmap overlay
7. Integrate with PersonaGenome

**Estimated Time**: 15-20 hours
**Blocking**: Phase 4 (needs model artifacts)

---

## 🎯 Success Metrics

### **Phase 1 Success**:
- ✅ `./jtag ai/adapter/test --all` returns immediately with UUID
- ✅ Tests run in background without blocking
- ✅ Status command shows real-time progress
- ✅ Results command returns completed test data
- ✅ Basic widget displays live progress

### **Full System Success**:
- ✅ Train models across all providers (Ollama, OpenAI, etc.)
- ✅ Real-time loss curves during training
- ✅ One-command model deployment to PersonaUsers
- ✅ A/B test fine-tuned vs baseline models
- ✅ Historical performance tracking
- ✅ Visual genome editing with Three.js
- ✅ Complete audit trail for reproducibility

---

## 📚 Related Documentation

- [UNIVERSAL-PRIMITIVES.md](./UNIVERSAL-PRIMITIVES.md) - Commands, Events, Entity foundation
- [ARCHITECTURE-RULES.md](./ARCHITECTURE-RULES.md) - Type safety, environment separation
- [PERSONA-CONVERGENCE-ROADMAP.md](../system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md) - PersonaGenome integration
- [LORA-GENOME-PAGING.md](../system/user/server/modules/LORA-GENOME-PAGING.md) - Virtual memory for skills

### **New Documentation** (To be created):
- [AI-TESTING-ENTITIES.md](./AI-TESTING-ENTITIES.md) - Test execution entity schemas
- [AI-TRAINING-ENTITIES.md](./AI-TRAINING-ENTITIES.md) - Training job entity schemas
- [AI-MODEL-ENTITIES.md](./AI-MODEL-ENTITIES.md) - Model artifact entity schemas
- [AI-CAPABILITY-ENTITIES.md](./AI-CAPABILITY-ENTITIES.md) - Capability matrix schemas
- [AI-DASHBOARD-WIDGETS.md](./AI-DASHBOARD-WIDGETS.md) - Widget specifications
- [AI-TRAINING-VISUALIZATIONS.md](./AI-TRAINING-VISUALIZATIONS.md) - TensorBoard-style charts
- [AI-GENOME-DESIGNER.md](./AI-GENOME-DESIGNER.md) - Three.js visual editor

---

## 💡 Key Design Principles

### **1. Entity-First**
All stateful data goes in entities. Commands read/write entities. Events notify of changes.

### **2. Universal Code**
Same TypeScript code runs in browser, server, CLI. No duplication.

### **3. Real-Time by Default**
Emit events for every state change. Widgets subscribe and auto-update.

### **4. Provider-Agnostic**
All commands work through AIProviderAdapter interface. No provider-specific code.

### **5. Observable & Debuggable**
Full audit trail in entities. Searchable logs. Performance metrics.

### **6. Interactive & Visual**
TensorBoard-style exploration. Three.js for complex visualizations.

### **7. Production-Ready**
Async by default. Error handling. Checkpoint/resume. Rollback capability.

---

## 🚀 Getting Started

### **Current State** (Phase 1 - Partial):
```bash
# Test command exists but times out (needs async pattern)
./jtag ai/adapter/test --adapter=ollama
# Error: Request timeout after 30000ms
```

### **After Phase 1 Completion**:
```bash
# Returns immediately with handle
./jtag ai/adapter/test --adapter=ollama --async
# { "testId": "550e8400...", "status": "queued" }

# Monitor progress
./jtag ai/adapter/test/status --testId=550e8400...
# { "progress": 45, "currentStep": "Testing embeddings..." }

# Get results when complete
./jtag ai/adapter/test/results --testId=550e8400...
# { "adapter": "ollama", "healthy": true, "testResults": [...] }
```

### **After Full Implementation**:
```bash
# Visual dashboard
open http://localhost:9003/ai/dashboard

# Start training
./jtag ai/train/start --adapter=ollama --skillName=typescript-expert

# Watch real-time (like TensorBoard)
./jtag ai/train/watch --jobId=abc123
# Live loss curves, system metrics, logs streaming...

# Deploy trained model
./jtag ai/model/deploy --modelId=typescript-expert-v2 --persona=helper-ai
```

---

**This architecture transforms AI infrastructure from "hope it works" to "know it works" with full observability, control, and reproducibility.**

**Ready to implement Phase 1 async pattern now!** 🚀
