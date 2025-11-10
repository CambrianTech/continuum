# LoRA Fine-Tuning Training System Architecture

**Status**: Design Phase → Implementation Ready
**Version**: 1.0
**Last Updated**: November 7, 2025
**Authors**: Claude Code

---

## Executive Summary

This document defines the complete architecture for LoRA fine-tuning training system in Continuum. The system builds on existing infrastructure (dataset generation, multi-database handles, genome entities) to provide end-to-end training workflow from data preparation through deployment.

**Key Goals:**
1. Type-safe, provider-agnostic training orchestration
2. Multi-backend support (MLX, PEFT, API-based)
3. Storage-adapter-agnostic data management
4. Continuous learning integration with PersonaUser
5. Production-ready monitoring and metrics

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Architecture Principles](#architecture-principles)
3. [Data Storage Architecture](#data-storage-architecture)
4. [Entity Definitions](#entity-definitions)
5. [Command Specifications](#command-specifications)
6. [Training Workflow](#training-workflow)
7. [Integration Points](#integration-points)
8. [File System Layout](#file-system-layout)
9. [Data Flow Diagrams](#data-flow-diagrams)
10. [Implementation Phases](#implementation-phases)

---

## System Overview

### Current Infrastructure (EXISTS)

```
✅ Dataset Generation
   ├── ai/dataset/create → JSONL files in /datasets/parsed/
   ├── TrainingDatasetEntity metadata tracking
   ├── GitHistoryParser: 1,590 examples (269MB)
   └── Format: OpenAI/Anthropic compatible

✅ Multi-Database Handles
   ├── DatabaseHandleRegistry → multiple databases (ANY adapter)
   ├── data/open, data/close, data/list-handles
   └── Storage-adapter-agnostic (SQLite, JSON, Vector, Graph, etc.)

✅ LoRA Adapter System
   ├── LoRAAdapter class (load/unload/activate)
   ├── Provider-agnostic (Anthropic, OpenAI, Ollama)
   ├── LRU eviction when memory full
   └── Genome paging (virtual memory for skills)

✅ Genome Entities
   ├── GenomeEntity: Complete persona genome
   ├── GenomeLayerEntity: Single LoRA adapter layer
   ├── TraitType: personality, expertise, conversational
   └── Layer stacking with weights and ordering

✅ Training Environment
   ├── Python venv with MLX + PEFT
   ├── Apple Silicon (M1/M2/M3) support
   ├── PEFTLoRAAdapter implementation
   └── BaseServerLoRATrainer utilities
```

### New Components (TO BUILD)

```
🔨 Training Session Management
   ├── TrainingSessionEntity → track long-running jobs
   ├── TrainingCheckpointEntity → save intermediate state
   ├── TrainingMetricsEntity → loss, accuracy, validation
   └── TrainingLogEntity → detailed execution logs

🔨 Training Commands
   ├── training/start → initiate training job
   ├── training/status → poll job progress
   ├── training/stop → cancel running job
   ├── training/list → list all sessions
   └── training/metrics → fetch training metrics

🔨 Adapter Deployment
   ├── adapter/deploy → register trained adapter
   ├── adapter/test → validate adapter quality
   ├── adapter/version → manage adapter versions
   └── adapter/rollback → revert to previous version

🔨 Continuous Learning
   ├── PersonaUser auto-task generation
   ├── Interaction quality scoring
   ├── Automatic dataset accumulation
   └── Scheduled retraining triggers
```

---

## Architecture Principles

### 1. Storage-Adapter Agnostic

**Problem**: Training data, session state, and metrics shouldn't be coupled to SQLite.

**Solution**: All training data access goes through DatabaseHandleRegistry.

```typescript
// ✅ CORRECT - Storage agnostic
const trainingDbHandle = await Commands.execute('data/open', {
  adapter: 'sqlite', // or 'json', 'vector', 'graph'
  config: { path: '/datasets/prepared/training.sqlite' }
});

const sessions = await Commands.execute('data/list', {
  dbHandle: trainingDbHandle.dbHandle,
  collection: 'training_sessions',
  filter: { personaId }
});

// ❌ WRONG - Direct SQLite coupling
import sqlite3 from 'sqlite3';
const db = new sqlite3.Database('/datasets/prepared/training.sqlite');
```

### 2. Provider Abstraction

**Problem**: Different providers (MLX, PEFT, OpenAI, DeepSeek) have different APIs.

**Solution**: BaseLoRATrainer interface abstracts provider specifics.

```typescript
// Unified interface - works with ANY provider
const trainer: LoRATrainer = providerRegistry.get(request.provider);
const result = await trainer.trainLoRA(request);

// No if/else chains, no provider-specific logic in orchestrator
```

### 3. Async-First Design

**Problem**: Training jobs take 10-30 minutes (local) or hours (API), blocking is unacceptable.

**Solution**: All training operations return immediately with job UUID, poll for status.

```typescript
// Start training (returns immediately)
const session = await Commands.execute('training/start', {
  personaId, dataset, baseModel, ...hyperparameters
});
// → { sessionId: 'uuid-123', status: 'queued', estimatedDuration: 1800000 }

// Poll for progress (non-blocking)
const status = await Commands.execute('training/status', {
  sessionId: 'uuid-123'
});
// → { status: 'training', progress: 0.35, currentLoss: 0.82, eta: 1200000 }

// Completion (webhook or polling)
const final = await Commands.execute('training/status', {
  sessionId: 'uuid-123'
});
// → { status: 'completed', adapterPath: '...', metrics: {...} }
```

### 4. Type Safety (Rust-like)

**Problem**: Training system involves complex data structures - runtime errors are expensive.

**Solution**: Strict TypeScript types everywhere, no `any` or `unknown`.

```typescript
// ✅ CORRECT - Fully typed
const result = await Commands.execute<TrainingStartResult>('training/start', {
  personaId: UUID,
  dataset: TrainingDataset,
  provider: 'peft' | 'mlx' | 'openai',
  baseModel: string,
  hyperparameters: LoRAHyperparameters
});

// ❌ WRONG - Loses type safety
const result: any = await Commands.execute('training/start', params);
```

### 5. Separation of Concerns

**Training Orchestrator** (shared logic):
- Job lifecycle management
- Progress tracking
- Error handling
- Metrics aggregation

**Provider Adapters** (provider-specific):
- Dataset format conversion
- Training execution
- Checkpoint management
- Adapter export

**Deployment Manager** (system integration):
- Adapter registration
- Version management
- Genome integration
- Quality validation

---

## Data Storage Architecture

### Question 1: Do we need JSONL → SQLite conversion?

**Answer**: OPTIONAL, depends on use case.

**Option A: JSONL-Only (Simpler)**
```
/datasets/parsed/continuum-git-2025-11-08.jsonl  ← Created by ai/dataset/create
                ↓ (used directly by trainer)
MLX/PEFT reads JSONL → trains → outputs adapter
```

**Pros**:
- No conversion step
- MLX and PEFT both read JSONL natively
- Simpler pipeline

**Cons**:
- No structured querying (can't filter by quality, date range, etc.)
- No incremental updates (must regenerate entire file)
- No multi-database handles (can't use data/list with filters)

**Option B: JSONL + SQLite (More Powerful)**
```
/datasets/parsed/continuum-git-2025-11-08.jsonl
                ↓ (import via training/prepare)
/datasets/prepared/continuum-git.sqlite
  └── Table: training_examples (id, messages, metadata, quality, used_count)
                ↓ (query with filters)
TrainingSessionEntity references prepared dataset
                ↓ (export to temp JSONL for training)
MLX/PEFT reads temp JSONL → trains → outputs adapter
```

**Pros**:
- Structured querying (filter by quality, unused examples, date range)
- Incremental updates (add new examples without regeneration)
- Multi-database handles (use data/list, data/update)
- Training session can mark examples as "used"
- Quality scoring and metadata tracking

**Cons**:
- Additional conversion step
- More storage (JSONL + SQLite)

**RECOMMENDATION**: **Option B (JSONL + SQLite)** for Phase 1.

**Rationale**:
1. Continuous learning requires marking examples as "used"
2. Quality filtering is essential for good training
3. Multi-database handle system already exists
4. Conversion is one-time cost, querying is ongoing benefit

### Dataset Directory Structure

```
/datasets/
├── raw/                          # Raw source data (git repos, conversations)
│   └── continuum-git/
│       └── .git/
│
├── parsed/                       # JSONL files from ai/dataset/create
│   ├── continuum-git-2025-11-08T00-31-01.jsonl  (269MB, 1590 examples)
│   ├── chat-helper-ai-2025-11-08.jsonl
│   └── ...
│
└── prepared/                     # SQLite databases for training
    ├── continuum-git.sqlite      # Imported from parsed/continuum-git-*.jsonl
    │   └── training_examples table (id, messages, metadata, quality, used)
    ├── chat-helper-ai.sqlite
    └── ...
```

**Workflow**:

1. **Generate JSONL**: `./jtag ai/dataset/create --project=continuum-git`
   - Creates: `/datasets/parsed/continuum-git-2025-11-08T00-31-01.jsonl`
   - Metadata: TrainingDatasetEntity stored in main DB

2. **Prepare SQLite** (NEW): `./jtag training/prepare --datasetPath=/datasets/parsed/continuum-git-*.jsonl`
   - Imports JSONL into SQLite
   - Creates: `/datasets/prepared/continuum-git.sqlite`
   - Returns: `dbHandle` for prepared database

3. **Query Examples**: Use multi-database handles
   ```bash
   # Open prepared database
   ./jtag data/open --adapter=sqlite --path=/datasets/prepared/continuum-git.sqlite --mode=readonly
   # → { dbHandle: 'prepared-db-001' }

   # List high-quality unused examples
   ./jtag data/list --dbHandle=prepared-db-001 --collection=training_examples \
     --filter='{"quality": {"$gte": 0.8}, "used": false}' --limit=100
   ```

4. **Start Training**: `./jtag training/start --dbHandle=prepared-db-001 --personaId=...`
   - Queries examples from prepared DB
   - Exports to temp JSONL for trainer
   - Marks examples as "used" after training

### Storage Schema: Prepared Database

**Collection**: `training_examples`

```typescript
interface TrainingExampleRecord {
  id: UUID;                    // Unique example ID
  messages: TrainingMessage[]; // Chat completion format
  metadata: {
    sourceDataset: string;     // Original JSONL file
    timestamp: number;         // When example was created
    roomId?: UUID;             // If from chat conversation
    confidence: number;        // Quality score (0-1)
  };

  // Training usage tracking
  used: boolean;               // Has this example been used in training?
  usedCount: number;           // Number of times used
  lastUsedAt?: number;         // Timestamp of last use

  // Quality metrics
  quality: number;             // Overall quality score (0-1)
  validated: boolean;          // Has this been manually reviewed?

  // Filtering
  tags: string[];              // ['typescript', 'refactoring', 'testing']
  minTokens: number;           // Minimum tokens in any message
  maxTokens: number;           // Maximum tokens in any message
}
```

---

## Entity Definitions

### TrainingSessionEntity

**Purpose**: Track long-running training jobs.

**Collection**: `training_sessions`

```typescript
import { BaseEntity } from '@system/data/entities/BaseEntity';
import { TextField, JsonField, ForeignKeyField } from '@system/data/decorators/FieldDecorators';
import type { UUID } from '@types/CrossPlatformUUID';
import type { TraitType } from '@system/genome/entities/GenomeLayerEntity';

/**
 * Training session lifecycle status
 */
export type TrainingStatus =
  | 'queued'      // Waiting to start
  | 'preparing'   // Preparing dataset
  | 'training'    // Active training
  | 'validating'  // Running validation
  | 'exporting'   // Exporting adapter
  | 'deploying'   // Registering adapter in genome
  | 'completed'   // Successfully finished
  | 'failed'      // Training failed
  | 'cancelled';  // User cancelled

/**
 * Training hyperparameters
 */
export interface TrainingHyperparameters {
  rank: number;              // LoRA rank (8-256)
  alpha: number;             // LoRA alpha (8-256)
  epochs: number;            // Training epochs (1-100)
  learningRate: number;      // Learning rate (0.00001-0.001)
  batchSize: number;         // Batch size (1-32)
  maxTokens?: number;        // Max sequence length
  warmupSteps?: number;      // Warmup steps for learning rate
  weightDecay?: number;      // L2 regularization
}

/**
 * Training session entity
 *
 * Tracks complete lifecycle of a training job from start to deployment.
 */
export class TrainingSessionEntity extends BaseEntity {
  static readonly collection = 'training_sessions';

  // Identity
  @ForeignKeyField({ references: 'users.id', index: true })
  personaId: UUID;

  @TextField({ index: true })
  personaName: string;

  @TextField({ index: true })
  traitType: TraitType;

  // Provider
  @TextField({ index: true })
  provider: 'peft' | 'mlx' | 'openai' | 'anthropic' | 'deepseek';

  @TextField()
  baseModel: string;          // 'TinyLlama/TinyLlama-1.1B-Chat-v1.0'

  // Dataset
  @TextField()
  datasetDbHandle: string;    // Database handle for prepared dataset

  @TextField()
  datasetCollection: string;  // Collection name (usually 'training_examples')

  @JsonField()
  datasetFilter: Record<string, unknown>;  // Filter used to select examples

  @TextField()
  exampleCount: number;       // Number of examples used

  // Hyperparameters
  @JsonField()
  hyperparameters: TrainingHyperparameters;

  // Status
  @TextField({ index: true })
  status: TrainingStatus;

  @TextField()
  progress: number;           // 0.0 - 1.0

  @TextField()
  currentEpoch?: number;      // Current epoch (1-based)

  @TextField()
  currentStep?: number;       // Current step within epoch

  // Timestamps
  @TextField()
  queuedAt: number;

  @TextField()
  startedAt?: number;

  @TextField()
  completedAt?: number;

  @TextField()
  estimatedCompletion?: number;

  // Results
  @TextField()
  adapterPath?: string;       // Path to trained adapter (local)

  @TextField()
  adapterModelId?: string;    // Model ID (API-based)

  @ForeignKeyField({ references: 'genome_layers.id' })
  genomeLayerId?: UUID;       // Created GenomeLayerEntity

  // Metrics (final values)
  @TextField()
  finalLoss?: number;

  @TextField()
  finalValidationLoss?: number;

  @TextField()
  trainingDuration?: number;  // Milliseconds

  // Error handling
  @TextField()
  error?: string;

  @TextField()
  errorDetails?: string;

  // Cost tracking (for API-based training)
  @TextField()
  estimatedCost?: number;     // USD

  @TextField()
  actualCost?: number;        // USD

  // Provider-specific job ID
  @TextField()
  providerJobId?: string;     // OpenAI job ID, DeepSeek job ID, etc.

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.personaId = '' as UUID;
    this.personaName = '';
    this.traitType = 'conversational';
    this.provider = 'peft';
    this.baseModel = '';
    this.datasetDbHandle = 'default';
    this.datasetCollection = 'training_examples';
    this.datasetFilter = {};
    this.exampleCount = 0;
    this.hyperparameters = {
      rank: 32,
      alpha: 32,
      epochs: 3,
      learningRate: 0.0001,
      batchSize: 4
    };
    this.status = 'queued';
    this.progress = 0;
    this.queuedAt = Date.now();
  }

  get collection(): string {
    return TrainingSessionEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.personaId) {
      return { success: false, error: 'personaId required' };
    }
    if (!this.baseModel) {
      return { success: false, error: 'baseModel required' };
    }
    if (this.exampleCount < 1) {
      return { success: false, error: 'exampleCount must be >= 1' };
    }
    return { success: true };
  }
}
```

### TrainingCheckpointEntity

**Purpose**: Save intermediate training state for resume/recovery.

**Collection**: `training_checkpoints`

```typescript
/**
 * Training checkpoint entity
 *
 * Saves intermediate training state for:
 * - Resume after interruption
 * - Rollback to earlier epoch
 * - Progress visualization
 */
export class TrainingCheckpointEntity extends BaseEntity {
  static readonly collection = 'training_checkpoints';

  @ForeignKeyField({ references: 'training_sessions.id', index: true })
  sessionId: UUID;

  @TextField({ index: true })
  epoch: number;              // Epoch number (1-based)

  @TextField()
  step: number;               // Step within epoch

  @TextField()
  timestamp: number;

  // Metrics at this checkpoint
  @TextField()
  trainLoss: number;

  @TextField()
  validationLoss?: number;

  @TextField()
  learningRate: number;

  // Checkpoint files
  @TextField()
  checkpointPath: string;     // Path to checkpoint directory

  @TextField()
  checkpointSize: number;     // Bytes

  // Keep this checkpoint? (for selective retention)
  @TextField()
  retain: boolean;

  [key: string]: unknown;

  constructor() {
    super();
    this.sessionId = '' as UUID;
    this.epoch = 0;
    this.step = 0;
    this.timestamp = Date.now();
    this.trainLoss = 0;
    this.learningRate = 0;
    this.checkpointPath = '';
    this.checkpointSize = 0;
    this.retain = false;
  }

  get collection(): string {
    return TrainingCheckpointEntity.collection;
  }
}
```

### TrainingMetricsEntity

**Purpose**: Time-series metrics for training visualization.

**Collection**: `training_metrics`

```typescript
/**
 * Training metrics entity (time-series data)
 *
 * Records metrics at regular intervals during training:
 * - Loss curves
 * - Learning rate schedule
 * - GPU utilization
 * - Memory usage
 */
export class TrainingMetricsEntity extends BaseEntity {
  static readonly collection = 'training_metrics';

  @ForeignKeyField({ references: 'training_sessions.id', index: true })
  sessionId: UUID;

  @TextField({ index: true })
  timestamp: number;

  @TextField()
  epoch: number;

  @TextField()
  step: number;

  // Loss metrics
  @TextField()
  trainLoss: number;

  @TextField()
  validationLoss?: number;

  @TextField()
  gradientNorm?: number;

  // Hyperparameters (for schedule tracking)
  @TextField()
  learningRate: number;

  // Performance metrics
  @TextField()
  samplesPerSecond?: number;

  @TextField()
  tokensPerSecond?: number;

  // Resource usage
  @TextField()
  memoryUsedMB?: number;

  @TextField()
  memoryTotalMB?: number;

  @TextField()
  gpuUtilization?: number;    // 0-100

  [key: string]: unknown;

  constructor() {
    super();
    this.sessionId = '' as UUID;
    this.timestamp = Date.now();
    this.epoch = 0;
    this.step = 0;
    this.trainLoss = 0;
    this.learningRate = 0;
  }

  get collection(): string {
    return TrainingMetricsEntity.collection;
  }
}
```

### TrainingLogEntity

**Purpose**: Detailed execution logs for debugging.

**Collection**: `training_logs`

```typescript
/**
 * Training log entity
 *
 * Captures detailed logs during training:
 * - Python subprocess output
 * - API responses
 * - Error stack traces
 * - Debug information
 */
export class TrainingLogEntity extends BaseEntity {
  static readonly collection = 'training_logs';

  @ForeignKeyField({ references: 'training_sessions.id', index: true })
  sessionId: UUID;

  @TextField({ index: true })
  timestamp: number;

  @TextField({ index: true })
  level: 'debug' | 'info' | 'warn' | 'error';

  @TextField()
  message: string;

  @JsonField()
  metadata?: Record<string, unknown>;

  [key: string]: unknown;

  constructor() {
    super();
    this.sessionId = '' as UUID;
    this.timestamp = Date.now();
    this.level = 'info';
    this.message = '';
  }

  get collection(): string {
    return TrainingLogEntity.collection;
  }
}
```

---

## Command Specifications

### training/prepare

**Purpose**: Import JSONL dataset into prepared database for structured querying.

**Default Adapter**: SQLite (recommended for continuous learning - enables marking examples as "used", quality filtering, etc.)

**Note**: While DatabaseHandleRegistry is adapter-agnostic, training/prepare defaults to SQLite for its structured query capabilities needed by continuous learning.

**Path**: `commands/training/prepare/`

```typescript
import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@types/CrossPlatformUUID';

export interface TrainingPrepareParams extends CommandParams {
  /**
   * Path to JSONL dataset file
   * Example: '/datasets/parsed/continuum-git-2025-11-08.jsonl'
   */
  datasetPath: string;

  /**
   * Output database path (default: /datasets/prepared/<name>.sqlite)
   */
  outputPath?: string;

  /**
   * Quality threshold - only import examples with quality >= threshold
   * Default: 0.5
   */
  qualityThreshold?: number;

  /**
   * Overwrite existing database?
   * Default: false
   */
  overwrite?: boolean;
}

export interface TrainingPrepareResult extends CommandResult {
  success: boolean;

  /**
   * Database handle for prepared database
   */
  dbHandle: string;

  /**
   * Path to created SQLite database
   */
  databasePath: string;

  /**
   * Number of examples imported
   */
  examplesImported: number;

  /**
   * Number of examples filtered out
   */
  examplesFiltered: number;

  /**
   * Import duration in milliseconds
   */
  durationMs: number;

  error?: string;
}
```

**Implementation**:
1. Read JSONL file line-by-line
2. Parse each line as TrainingExample
3. Compute quality score (token count, message balance, etc.)
4. Filter by quality threshold
5. Insert into SQLite `training_examples` table
6. Return dbHandle for immediate use

### training/start

**Purpose**: Start a new training session.

**Path**: `commands/training/start/`

```typescript
export interface TrainingStartParams extends CommandParams {
  /**
   * PersonaUser ID to train
   */
  personaId: UUID;

  /**
   * Training provider: 'peft' | 'mlx' | 'openai' | 'anthropic' | 'deepseek'
   */
  provider: 'peft' | 'mlx' | 'openai' | 'anthropic' | 'deepseek';

  /**
   * Prepared dataset database handle
   * Get from training/prepare command
   */
  datasetDbHandle: string;

  /**
   * Filter for selecting training examples from prepared dataset
   * Example: { quality: { $gte: 0.8 }, used: false }
   */
  datasetFilter?: Record<string, unknown>;

  /**
   * Maximum examples to use (default: all matching filter)
   */
  maxExamples?: number;

  /**
   * Trait type to train
   */
  traitType?: TraitType;

  /**
   * Base model to fine-tune
   * Provider-specific (e.g., 'TinyLlama/TinyLlama-1.1B-Chat-v1.0' for PEFT)
   */
  baseModel?: string;

  /**
   * Training hyperparameters
   */
  hyperparameters?: Partial<TrainingHyperparameters>;

  /**
   * Enable checkpoints every N epochs (default: 1)
   */
  checkpointEveryEpochs?: number;

  /**
   * Validation split (0-1, default: 0.1 = 10%)
   */
  validationSplit?: number;

  /**
   * Dry run - estimate cost/time without training
   */
  dryRun?: boolean;
}

export interface TrainingStartResult extends CommandResult {
  success: boolean;

  /**
   * Training session ID (UUID)
   */
  sessionId: UUID;

  /**
   * Initial status (usually 'queued')
   */
  status: TrainingStatus;

  /**
   * Number of examples selected for training
   */
  exampleCount: number;

  /**
   * Estimated cost in USD (for API providers)
   */
  estimatedCost?: number;

  /**
   * Estimated duration in milliseconds
   */
  estimatedDuration: number;

  /**
   * Estimated completion timestamp
   */
  estimatedCompletion: number;

  error?: string;
}
```

**Implementation**:
1. Validate provider and baseModel compatibility
2. Query examples from prepared dataset using filter
3. Create TrainingSessionEntity
4. Estimate cost and duration
5. If not dryRun, queue training job
6. Return sessionId immediately

### training/status

**Purpose**: Get training session status and progress.

**Path**: `commands/training/status/`

```typescript
export interface TrainingStatusParams extends CommandParams {
  /**
   * Training session ID
   */
  sessionId: UUID;

  /**
   * Include detailed metrics? (default: false)
   */
  includeMetrics?: boolean;

  /**
   * Include logs? (default: false)
   */
  includeLogs?: boolean;

  /**
   * Log limit (default: 50)
   */
  logLimit?: number;
}

export interface TrainingStatusResult extends CommandResult {
  success: boolean;

  /**
   * Training session
   */
  session: TrainingSessionEntity;

  /**
   * Detailed metrics (if includeMetrics: true)
   */
  metrics?: TrainingMetricsEntity[];

  /**
   * Recent logs (if includeLogs: true)
   */
  logs?: TrainingLogEntity[];

  error?: string;
}
```

**Implementation**:
1. Load TrainingSessionEntity by sessionId
2. Optionally load metrics time-series
3. Optionally load recent logs
4. Return complete status

### training/stop

**Purpose**: Cancel running training session.

**Path**: `commands/training/stop/`

```typescript
export interface TrainingStopParams extends CommandParams {
  /**
   * Training session ID
   */
  sessionId: UUID;

  /**
   * Save checkpoint before stopping?
   */
  saveCheckpoint?: boolean;
}

export interface TrainingStopResult extends CommandResult {
  success: boolean;

  /**
   * Updated session status (should be 'cancelled')
   */
  status: TrainingStatus;

  /**
   * Checkpoint path (if saveCheckpoint: true)
   */
  checkpointPath?: string;

  error?: string;
}
```

**Implementation**:
1. Load TrainingSessionEntity
2. Kill Python subprocess or cancel API job
3. Optionally save checkpoint
4. Update session status to 'cancelled'

### training/list

**Purpose**: List training sessions with filters.

**Path**: `commands/training/list/`

```typescript
export interface TrainingListParams extends CommandParams {
  /**
   * Filter by personaId
   */
  personaId?: UUID;

  /**
   * Filter by status
   */
  status?: TrainingStatus | TrainingStatus[];

  /**
   * Filter by provider
   */
  provider?: string;

  /**
   * Limit results
   */
  limit?: number;

  /**
   * Offset for pagination
   */
  offset?: number;
}

export interface TrainingListResult extends CommandResult {
  success: boolean;

  /**
   * Training sessions
   */
  sessions: TrainingSessionEntity[];

  /**
   * Total count (before limit/offset)
   */
  totalCount: number;

  error?: string;
}
```

### training/metrics

**Purpose**: Fetch training metrics for visualization.

**Path**: `commands/training/metrics/`

```typescript
export interface TrainingMetricsParams extends CommandParams {
  /**
   * Training session ID
   */
  sessionId: UUID;

  /**
   * Metric type to fetch
   */
  metricType?: 'trainLoss' | 'validationLoss' | 'learningRate' | 'all';

  /**
   * Sample rate (return every Nth point, default: 1 = all points)
   */
  sampleRate?: number;
}

export interface TrainingMetricsResult extends CommandResult {
  success: boolean;

  /**
   * Metrics time-series
   */
  metrics: TrainingMetricsEntity[];

  /**
   * Summary statistics
   */
  summary: {
    minLoss: number;
    maxLoss: number;
    finalLoss: number;
    bestEpoch: number;
  };

  error?: string;
}
```

### adapter/deploy

**Purpose**: Register trained adapter in genome system.

**Path**: `commands/adapter/deploy/`

```typescript
export interface AdapterDeployParams extends CommandParams {
  /**
   * Training session ID (source of adapter)
   */
  sessionId: UUID;

  /**
   * Adapter name (default: <personaName>-<traitType>-<timestamp>)
   */
  adapterName?: string;

  /**
   * Adapter description
   */
  description?: string;

  /**
   * Tags for searching
   */
  tags?: string[];

  /**
   * Deploy to active genome?
   * If true, adds this layer to persona's genome immediately
   */
  deployToGenome?: boolean;

  /**
   * Layer weight in genome (0-1, default: 1.0)
   */
  layerWeight?: number;
}

export interface AdapterDeployResult extends CommandResult {
  success: boolean;

  /**
   * Created GenomeLayerEntity
   */
  genomeLayerId: UUID;

  /**
   * Updated GenomeEntity (if deployToGenome: true)
   */
  genomeId?: UUID;

  /**
   * Adapter file size in bytes
   */
  adapterSize: number;

  /**
   * Deployment duration in milliseconds
   */
  durationMs: number;

  error?: string;
}
```

**Implementation**:
1. Load TrainingSessionEntity
2. Validate adapter files exist
3. Create GenomeLayerEntity:
   - Copy adapter files to `.continuum/genome/adapters/<layer-id>/`
   - Calculate embedding (for similarity search)
   - Store metadata (rank, alpha, baseModel, traitType)
4. Optionally add to GenomeEntity
5. Return genomeLayerId

### adapter/test

**Purpose**: Validate adapter quality before deployment.

**Path**: `commands/adapter/test/`

```typescript
export interface AdapterTestParams extends CommandParams {
  /**
   * Training session ID (adapter to test)
   */
  sessionId: UUID;

  /**
   * Test dataset (use validation split from training)
   */
  useValidationSplit?: boolean;

  /**
   * Custom test prompts
   */
  testPrompts?: string[];

  /**
   * Number of samples to test (default: 20)
   */
  sampleCount?: number;
}

export interface AdapterTestResult extends CommandResult {
  success: boolean;

  /**
   * Test metrics
   */
  metrics: {
    perplexity: number;        // Lower is better
    averageConfidence: number; // 0-1
    validationLoss: number;
  };

  /**
   * Sample outputs (for manual review)
   */
  samples: Array<{
    prompt: string;
    baseModelOutput: string;
    fineTunedOutput: string;
  }>;

  /**
   * Pass/fail recommendation
   */
  recommendation: 'deploy' | 'retrain' | 'review';

  error?: string;
}
```

---

## Training Workflow

### End-to-End Flow

```
┌─────────────────────────────────────────────────────────────────┐
│ Phase 1: Dataset Preparation                                    │
└─────────────────────────────────────────────────────────────────┘

1. Generate JSONL dataset:
   ./jtag ai/dataset/create --project=continuum-git
   → /datasets/parsed/continuum-git-2025-11-08.jsonl (269MB, 1590 examples)

2. Prepare SQLite database:
   ./jtag training/prepare \
     --datasetPath=/datasets/parsed/continuum-git-2025-11-08.jsonl \
     --qualityThreshold=0.6
   → { dbHandle: 'training-db-001', examplesImported: 1420, examplesFiltered: 170 }

3. Verify prepared dataset:
   ./jtag data/list --dbHandle=training-db-001 --collection=training_examples \
     --filter='{"quality": {"$gte": 0.8}}' --limit=10
   → [ { id: '...', messages: [...], quality: 0.85, used: false }, ... ]

┌─────────────────────────────────────────────────────────────────┐
│ Phase 2: Training Session                                       │
└─────────────────────────────────────────────────────────────────┘

4. Start training (returns immediately):
   ./jtag training/start \
     --personaId=<helper-ai-id> \
     --provider=peft \
     --baseModel=TinyLlama/TinyLlama-1.1B-Chat-v1.0 \
     --datasetDbHandle=training-db-001 \
     --datasetFilter='{"quality": {"$gte": 0.8}, "used": false}' \
     --maxExamples=500 \
     --hyperparameters='{"rank": 32, "epochs": 3}' \
     --validationSplit=0.1

   → {
       sessionId: '550e8400-e29b-41d4-a716-446655440000',
       status: 'queued',
       exampleCount: 450,
       estimatedDuration: 1200000,  // 20 minutes
       estimatedCompletion: 1699999999999
     }

5. Poll for status:
   ./jtag training/status --sessionId=550e8400-...

   → {
       session: {
         status: 'training',
         progress: 0.35,
         currentEpoch: 2,
         currentStep: 157
       }
     }

6. Monitor metrics:
   ./jtag training/metrics --sessionId=550e8400-...

   → {
       metrics: [
         { epoch: 1, step: 100, trainLoss: 1.23, validationLoss: 1.45 },
         { epoch: 1, step: 200, trainLoss: 0.98, validationLoss: 1.12 },
         ...
       ],
       summary: { minLoss: 0.82, finalLoss: 0.85, bestEpoch: 3 }
     }

7. Training completes:
   ./jtag training/status --sessionId=550e8400-...

   → {
       session: {
         status: 'completed',
         progress: 1.0,
         adapterPath: '.continuum/genome/training/550e8400-.../adapter',
         finalLoss: 0.85,
         trainingDuration: 1185000  // 19.75 minutes
       }
     }

┌─────────────────────────────────────────────────────────────────┐
│ Phase 3: Adapter Deployment                                     │
└─────────────────────────────────────────────────────────────────┘

8. Test adapter quality:
   ./jtag adapter/test --sessionId=550e8400-... --sampleCount=20

   → {
       metrics: { perplexity: 12.5, averageConfidence: 0.82 },
       samples: [ { prompt: '...', baseModelOutput: '...', fineTunedOutput: '...' }, ... ],
       recommendation: 'deploy'
     }

9. Deploy adapter to genome:
   ./jtag adapter/deploy \
     --sessionId=550e8400-... \
     --adapterName=helper-ai-typescript-expertise \
     --description="TypeScript code review and refactoring expertise" \
     --tags='["typescript", "code-review", "continuum"]' \
     --deployToGenome=true \
     --layerWeight=1.0

   → {
       genomeLayerId: '660e8400-...',
       genomeId: '770e8400-...',
       adapterSize: 52428800  // 50MB
     }

10. Verify deployment:
    ./jtag ai/genome/stats --personaId=<helper-ai-id>

    → {
        genome: {
          layers: [
            { layerId: '660e8400-...', traitType: 'expertise', enabled: true, weight: 1.0 }
          ],
          totalSizeMB: 50,
          lastTrainedAt: 1699999999999
        }
      }

┌─────────────────────────────────────────────────────────────────┐
│ Phase 4: Continuous Learning (Future)                           │
└─────────────────────────────────────────────────────────────────┘

11. PersonaUser automatically creates training tasks:
    - Accumulate new interactions (100+ examples)
    - Score interaction quality
    - Self-create task: "Retrain TypeScript expertise layer"
    - Trigger: ./jtag training/start (programmatically)
```

---

## Integration Points

### 1. PersonaUser Integration

**Current**: PersonaUser has `serviceInbox()` autonomous loop.

**New**: PersonaUser monitors training sessions and triggers retraining.

```typescript
// system/user/server/PersonaUser.ts

class PersonaUser extends AIUser {
  private activeTrainingSessions: Set<UUID> = new Set();

  /**
   * Check for completed training sessions
   * Called from serviceInbox() loop
   */
  private async checkTrainingSessions(): Promise<void> {
    for (const sessionId of this.activeTrainingSessions) {
      const status = await Commands.execute<TrainingStatusResult>('training/status', {
        sessionId
      });

      if (status.session.status === 'completed') {
        await this.onTrainingCompleted(status.session);
        this.activeTrainingSessions.delete(sessionId);
      } else if (status.session.status === 'failed') {
        await this.onTrainingFailed(status.session);
        this.activeTrainingSessions.delete(sessionId);
      }
    }
  }

  /**
   * Handle training completion - deploy adapter
   */
  private async onTrainingCompleted(session: TrainingSessionEntity): Promise<void> {
    // Test adapter quality
    const testResult = await Commands.execute<AdapterTestResult>('adapter/test', {
      sessionId: session.id
    });

    if (testResult.recommendation === 'deploy') {
      // Auto-deploy to genome
      await Commands.execute('adapter/deploy', {
        sessionId: session.id,
        deployToGenome: true,
        layerWeight: 1.0
      });

      console.log(`✅ Deployed new ${session.traitType} layer`);
    } else {
      console.log(`⚠️  Adapter quality insufficient, requires review`);
      // Create task for human review
    }
  }

  /**
   * Continuous learning - auto-generate training tasks
   */
  async generateSelfTasks(): Promise<void> {
    // Check if we have accumulated enough new interactions
    const newInteractionsCount = await this.countUnusedInteractions();

    if (newInteractionsCount >= 100) {
      // Create self-task to retrain
      const task = {
        taskType: 'training',
        priority: 0.5,
        description: `Retrain ${this.entity.name} with ${newInteractionsCount} new interactions`,
        metadata: {
          provider: 'peft',
          traitType: 'conversational'
        }
      };

      await this.inbox.enqueue(task);
    }
  }

  /**
   * Count unused interactions for training
   */
  private async countUnusedInteractions(): Promise<number> {
    // Query messages where this persona responded
    // AND haven't been used in training yet
    const result = await Commands.execute('data/list', {
      collection: 'chat_messages',
      filter: {
        senderId: this.entity.id,
        // Need to track "used_in_training" flag in ChatMessageEntity
        used_in_training: false
      }
    });

    return result.data?.length ?? 0;
  }
}
```

### 2. GenomeManager Integration

**Current**: GenomeManager handles layer loading/unloading.

**New**: GenomeManager coordinates with training system.

```typescript
// system/genome/fine-tuning/server/GenomeManager.ts

class GenomeManager {
  /**
   * Load genome with trained adapters
   */
  async loadGenome(genomeId: UUID): Promise<void> {
    const genome = await this.loadGenomeEntity(genomeId);

    for (const layerRef of genome.layers) {
      if (layerRef.enabled) {
        const layer = await this.loadLayerEntity(layerRef.layerId);

        // Check if layer needs retraining (staleness check)
        const age = Date.now() - layer.lastTrainedAt;
        const maxAge = 30 * 24 * 60 * 60 * 1000; // 30 days

        if (age > maxAge) {
          console.log(`⚠️  Layer ${layer.name} is stale (${Math.floor(age / 86400000)} days old)`);
          // Could trigger automatic retraining
        }

        await this.loadLayer(layer);
      }
    }
  }

  /**
   * Evict layer to free memory (LRU)
   */
  async evictLRU(): Promise<void> {
    // Find least-recently-used layer
    const lruLayer = this.findLRULayer();

    // Before evicting, check if layer has pending training
    const pendingSessions = await Commands.execute('training/list', {
      filter: { genomeLayerId: lruLayer.id, status: 'training' }
    });

    if (pendingSessions.sessions.length > 0) {
      // Don't evict layer that's currently being trained
      return;
    }

    await this.unloadLayer(lruLayer);
  }
}
```

### 3. DataDaemon Integration

**Current**: DataDaemon uses single database.

**New**: DataDaemon routes operations to correct database handle.

```typescript
// daemons/data-daemon/server/DataDaemonServer.ts

class DataDaemonServer {
  /**
   * Execute data command with optional dbHandle
   */
  async execute(command: string, params: DataCommandParams): Promise<DataCommandResult> {
    const dbHandle = params.dbHandle ?? 'default';
    const adapter = this.handleRegistry.get(dbHandle);

    if (!adapter) {
      throw new Error(`Database handle not found: ${dbHandle}`);
    }

    // Route to appropriate storage adapter
    switch (command) {
      case 'data/create':
        return await adapter.create(params.collection, params.data, params.id);
      case 'data/read':
        return await adapter.read(params.collection, params.id);
      case 'data/list':
        return await adapter.list(params.collection, params.filter, params.limit);
      // ... etc
    }
  }
}
```

---

## File System Layout

```
/Volumes/FlashGordon/cambrian/continuum/
└── src/debug/jtag/
    ├── .continuum/
    │   ├── genome/
    │   │   ├── adapters/                    # Deployed adapters
    │   │   │   ├── 660e8400-...*/
    │   │   │   │   ├── adapter_model.safetensors  (~50MB)
    │   │   │   │   ├── adapter_config.json
    │   │   │   │   └── metadata.json
    │   │   │   └── 770e8400-...*/
    │   │   │
    │   │   ├── training/                    # Training output (temp)
    │   │   │   ├── 550e8400-...*/          # Training session ID
    │   │   │   │   ├── dataset.jsonl        # Exported training data
    │   │   │   │   ├── adapter/             # Trained adapter
    │   │   │   │   ├── checkpoints/         # Epoch checkpoints
    │   │   │   │   └── logs/                # Training logs
    │   │   │   └── ...
    │   │   │
    │   │   └── python/                      # Python environment
    │   │       ├── bootstrap.sh
    │   │       ├── train-wrapper.sh
    │   │       └── pkgs/                    # Conda packages
    │   │
    │   └── jtag/
    │       └── data/
    │           └── database.sqlite          # Main application database
    │
    ├── commands/
    │   ├── training/
    │   │   ├── prepare/
    │   │   │   ├── shared/TrainingPrepareTypes.ts
    │   │   │   ├── server/TrainingPrepareServerCommand.ts
    │   │   │   └── browser/TrainingPrepareBrowserCommand.ts
    │   │   │
    │   │   ├── start/
    │   │   │   ├── shared/TrainingStartTypes.ts
    │   │   │   ├── server/TrainingStartServerCommand.ts
    │   │   │   └── browser/TrainingStartBrowserCommand.ts
    │   │   │
    │   │   ├── status/
    │   │   │   ├── shared/TrainingStatusTypes.ts
    │   │   │   ├── server/TrainingStatusServerCommand.ts
    │   │   │   └── browser/TrainingStatusBrowserCommand.ts
    │   │   │
    │   │   ├── stop/
    │   │   ├── list/
    │   │   └── metrics/
    │   │
    │   └── adapter/
    │       ├── deploy/
    │       ├── test/
    │       ├── version/
    │       └── rollback/
    │
    ├── system/
    │   ├── data/
    │   │   └── entities/
    │   │       ├── TrainingSessionEntity.ts      # NEW
    │   │       ├── TrainingCheckpointEntity.ts   # NEW
    │   │       ├── TrainingMetricsEntity.ts      # NEW
    │   │       └── TrainingLogEntity.ts          # NEW
    │   │
    │   ├── genome/
    │   │   ├── entities/
    │   │   │   ├── GenomeEntity.ts               # EXISTS
    │   │   │   └── GenomeLayerEntity.ts          # EXISTS
    │   │   │
    │   │   └── fine-tuning/
    │   │       ├── shared/
    │   │       │   ├── FineTuningTypes.ts        # EXISTS
    │   │       │   └── BaseLoRATrainer.ts        # EXISTS
    │   │       │
    │   │       └── server/
    │   │           ├── TrainingOrchestrator.ts   # NEW
    │   │           ├── TrainingDatasetBuilder.ts # EXISTS
    │   │           ├── BaseServerLoRATrainer.ts  # EXISTS
    │   │           └── adapters/
    │   │               ├── PEFTLoRAAdapter.ts    # EXISTS
    │   │               ├── OpenAILoRAAdapter.ts  # EXISTS
    │   │               └── DeepSeekLoRAAdapter.ts # EXISTS
    │   │
    │   └── user/
    │       └── server/
    │           └── PersonaUser.ts                # MODIFY
    │
    └── tests/
        ├── unit/
        │   ├── training-session-entity.test.ts
        │   └── training-orchestrator.test.ts
        │
        └── integration/
            ├── training-end-to-end.test.ts
            └── adapter-deployment.test.ts

/Volumes/FlashGordon/cambrian/datasets/
├── raw/
│   └── continuum-git/                           # Raw git repo
│
├── parsed/
│   ├── continuum-git-2025-11-08T00-31-01.jsonl  # Generated by ai/dataset/create
│   └── chat-helper-ai-2025-11-08.jsonl
│
└── prepared/
    ├── continuum-git.sqlite                     # Imported by training/prepare
    │   └── training_examples table
    └── chat-helper-ai.sqlite
```

---

## Data Flow Diagrams

### Dataset Preparation Flow

```
┌─────────────────┐
│ Raw Git Repo    │
│ /datasets/raw/  │
│ continuum-git/  │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ ai/dataset/create                       │
│ - Parses git history                    │
│ - Extracts commit messages + diffs      │
│ - Formats as chat completions           │
│ - Writes JSONL                          │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ JSONL Dataset                           │
│ /datasets/parsed/continuum-git-*.jsonl  │
│ - 269MB, 1590 examples                  │
│ - Format: {"messages": [...]}           │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ training/prepare                        │
│ - Reads JSONL line-by-line              │
│ - Computes quality scores               │
│ - Filters by threshold                  │
│ - Inserts into SQLite                   │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ Prepared Database                       │
│ /datasets/prepared/continuum-git.sqlite │
│ - training_examples table               │
│ - Queryable with data/list              │
│ - Marks examples as "used"              │
└─────────────────────────────────────────┘
```

### Training Session Flow

```
┌─────────────────────────────────────────┐
│ training/start                          │
│ - Creates TrainingSessionEntity         │
│ - Status: 'queued'                      │
│ - Returns sessionId immediately         │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ TrainingOrchestrator.execute()          │
│ - Loads session from database           │
│ - Updates status: 'preparing'           │
│ - Queries examples from prepared DB     │
│ - Exports temp JSONL                    │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ BaseLoRATrainer.trainLoRA()             │
│ - Updates status: 'training'            │
│ - Spawns Python subprocess (PEFT/MLX)   │
│ - OR makes API request (OpenAI)         │
│ - Streams metrics to database           │
└────────┬────────────────────────────────┘
         │
         ├─── Every 100 steps ───┐
         │                        ▼
         │              ┌─────────────────────┐
         │              │ TrainingMetricsEntity│
         │              │ - trainLoss         │
         │              │ - learningRate      │
         │              └─────────────────────┘
         │
         ├─── Every epoch ───────┐
         │                        ▼
         │              ┌─────────────────────┐
         │              │TrainingCheckpointEntity│
         │              │ - Saves adapter state│
         │              └─────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ Training Complete                       │
│ - Updates status: 'completed'           │
│ - Saves adapter to training directory   │
│ - Records finalLoss, trainingDuration   │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ adapter/test (optional)                 │
│ - Loads adapter                         │
│ - Runs validation dataset               │
│ - Computes perplexity, confidence       │
│ - Returns recommendation                │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ adapter/deploy                          │
│ - Creates GenomeLayerEntity             │
│ - Copies adapter to genome/adapters/    │
│ - Adds layer to GenomeEntity            │
│ - Marks training examples as "used"     │
└─────────────────────────────────────────┘
```

### Continuous Learning Flow

```
┌─────────────────────────────────────────┐
│ PersonaUser serviceInbox()              │
│ - Processes chat messages               │
│ - Generates responses                   │
│ - Scores interaction quality            │
└────────┬────────────────────────────────┘
         │
         │ Every 100 interactions
         ▼
┌─────────────────────────────────────────┐
│ PersonaUser.generateSelfTasks()         │
│ - Checks if 100+ unused interactions    │
│ - Creates self-task: "Retrain layer"    │
│ - Priority: 0.5 (medium)                │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ PersonaUser processes self-task         │
│ - Extracts conversations from rooms     │
│ - Builds TrainingDataset                │
│ - Calls training/start programmatically │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│ Training runs asynchronously            │
│ - PersonaUser continues normal work     │
│ - Checks training status periodically   │
└────────┬────────────────────────────────┘
         │
         │ Training completes
         ▼
┌─────────────────────────────────────────┐
│ PersonaUser.onTrainingCompleted()       │
│ - Tests adapter quality                 │
│ - Auto-deploys if quality good          │
│ - Logs improvement metrics              │
└─────────────────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: Core Training Infrastructure (Week 1)

**Goal**: Basic training workflow from prepared dataset to completed adapter.

**Tasks**:
1. ✅ Create entity definitions:
   - TrainingSessionEntity
   - TrainingCheckpointEntity
   - TrainingMetricsEntity
   - TrainingLogEntity

2. ✅ Implement training/prepare command:
   - Read JSONL file
   - Compute quality scores
   - Import to SQLite
   - Return dbHandle

3. ✅ Implement training/start command:
   - Create TrainingSessionEntity
   - Validate provider and baseModel
   - Query examples from prepared DB
   - Estimate cost and duration
   - Queue training job

4. ✅ Implement TrainingOrchestrator:
   - Job queue management
   - Provider routing
   - Progress tracking
   - Error handling

5. ✅ Integration with PEFTLoRAAdapter:
   - Export temp JSONL from prepared DB
   - Spawn Python subprocess
   - Stream metrics to database
   - Save adapter on completion

**Success Criteria**:
- `./jtag training/prepare` imports JSONL → SQLite
- `./jtag training/start` queues training job
- Training runs end-to-end with PEFTLoRAAdapter
- TrainingSessionEntity tracks status and progress
- Adapter saved to `.continuum/genome/training/<session-id>/adapter/`

**Testing**:
```bash
# Test with small dataset (100 examples, TinyLlama, 1 epoch)
./jtag training/prepare --datasetPath=test-dataset.jsonl
./jtag training/start --provider=peft --baseModel=TinyLlama/TinyLlama-1.1B-Chat-v1.0 \
  --datasetDbHandle=training-db-001 --maxExamples=100 --hyperparameters='{"epochs": 1}'
```

### Phase 2: Status Monitoring & Metrics (Week 2)

**Goal**: Real-time training visibility.

**Tasks**:
1. ✅ Implement training/status command:
   - Load TrainingSessionEntity
   - Return current status and progress
   - Optionally include metrics and logs

2. ✅ Implement training/metrics command:
   - Query TrainingMetricsEntity time-series
   - Return loss curves, learning rate schedule
   - Compute summary statistics

3. ✅ Implement training/list command:
   - Query sessions with filters
   - Support pagination

4. ✅ Implement training/stop command:
   - Kill Python subprocess or cancel API job
   - Save checkpoint before stopping
   - Update session status to 'cancelled'

5. ✅ Add metrics streaming to training loop:
   - Insert TrainingMetricsEntity every 100 steps
   - Log Python subprocess output to TrainingLogEntity

**Success Criteria**:
- `./jtag training/status` shows real-time progress
- `./jtag training/metrics` returns loss curves
- `./jtag training/list` filters sessions by status/persona
- `./jtag training/stop` gracefully cancels training

**Testing**:
```bash
# Start training, then monitor
./jtag training/start ... # Get sessionId
./jtag training/status --sessionId=<id>
./jtag training/metrics --sessionId=<id>
./jtag training/stop --sessionId=<id> --saveCheckpoint=true
```

### Phase 3: Adapter Deployment (Week 3)

**Goal**: Deploy trained adapters to genome system.

**Tasks**:
1. ✅ Implement adapter/deploy command:
   - Create GenomeLayerEntity
   - Copy adapter files to genome/adapters/
   - Optionally add to GenomeEntity

2. ✅ Implement adapter/test command:
   - Load adapter
   - Run validation dataset
   - Compute quality metrics
   - Return recommendation

3. ✅ Implement adapter/version command:
   - List all versions of an adapter
   - Compare metrics across versions

4. ✅ Implement adapter/rollback command:
   - Revert to previous adapter version
   - Update GenomeEntity

5. ✅ Integration with GenomeManager:
   - Load deployed adapters
   - Track adapter usage
   - LRU eviction coordination

**Success Criteria**:
- `./jtag adapter/deploy` creates GenomeLayerEntity
- Adapter files copied to correct location
- `./jtag adapter/test` validates quality
- `./jtag ai/genome/stats` shows deployed adapters

**Testing**:
```bash
# Complete flow: train → test → deploy
./jtag training/start ... # Wait for completion
./jtag adapter/test --sessionId=<id> --sampleCount=20
./jtag adapter/deploy --sessionId=<id> --deployToGenome=true
./jtag ai/genome/stats --personaId=<id>
```

### Phase 4: Continuous Learning (Week 4)

**Goal**: PersonaUser auto-generates training tasks.

**Tasks**:
1. ✅ Add training session tracking to PersonaUser:
   - `activeTrainingSessions` Set
   - `checkTrainingSessions()` in serviceInbox loop
   - `onTrainingCompleted()` handler

2. ✅ Implement self-task generation:
   - `generateSelfTasks()` checks for 100+ unused interactions
   - Creates training task with metadata
   - Enqueues to PersonaInbox

3. ✅ Implement interaction quality scoring:
   - Track conversation coherence
   - Detect user corrections
   - Score based on feedback

4. ✅ Add "used_in_training" flag to ChatMessageEntity:
   - Mark messages as used after training
   - Query unused messages for dataset building

5. ✅ Implement automatic retraining triggers:
   - Time-based: Every 30 days
   - Threshold-based: 100+ unused interactions
   - Quality-based: Drop in performance metrics

**Success Criteria**:
- PersonaUser creates training task after 100 interactions
- Training runs automatically without human intervention
- Adapter auto-deploys if quality good
- System logs show continuous improvement

**Testing**:
```bash
# Simulate 100+ interactions, wait for auto-training
# Monitor logs for self-task creation
tail -f .continuum/sessions/user/*/logs/server.log | grep "SELF-TASK"
```

### Phase 5: Multi-Provider Support (Week 5)

**Goal**: Support API-based training (OpenAI, DeepSeek).

**Tasks**:
1. ✅ Implement OpenAILoRAAdapter:
   - Upload dataset to OpenAI API
   - Create fine-tuning job
   - Poll for completion
   - Store API model ID

2. ✅ Implement DeepSeekLoRAAdapter:
   - Similar to OpenAI adapter
   - DeepSeek-specific API calls

3. ✅ Add cost tracking:
   - Estimate cost before training
   - Track actual cost from API
   - Store in TrainingSessionEntity

4. ✅ Implement provider failover:
   - If PEFT fails (GPU unavailable), fallback to OpenAI
   - If OpenAI fails (quota), fallback to DeepSeek

**Success Criteria**:
- `./jtag training/start --provider=openai` works end-to-end
- Cost estimate shown before training
- Actual cost tracked in session
- Adapter accessible via API

**Testing**:
```bash
# Test OpenAI fine-tuning
./jtag training/start --provider=openai --baseModel=gpt-3.5-turbo \
  --datasetDbHandle=training-db-001 --maxExamples=100 --dryRun=true
# Check cost estimate, then run without dryRun
```

### Phase 6: Production Hardening (Week 6+)

**Goal**: Production-ready reliability and monitoring.

**Tasks**:
1. ✅ Add retry logic:
   - Exponential backoff for API failures
   - Resume from checkpoint on crash

2. ✅ Add alerting:
   - Training failure notifications
   - Quality degradation alerts
   - Cost threshold warnings

3. ✅ Add monitoring dashboard:
   - Training session overview
   - Adapter quality trends
   - Cost tracking

4. ✅ Optimize performance:
   - Parallel training sessions
   - Batch dataset preparation
   - Cache embeddings

5. ✅ Add security:
   - Validate training data for malicious examples
   - Sandbox Python subprocess
   - Encrypt API keys

**Success Criteria**:
- System recovers from crashes automatically
- Alerts fire on training failures
- Dashboard shows real-time metrics
- No security vulnerabilities

---

## Summary

This architecture provides:

1. **Type-Safe**: Strict TypeScript types, no `any` or `unknown`
2. **Storage-Agnostic**: Multi-database handles work with any storage adapter
3. **Async-First**: Non-blocking training with status polling
4. **Provider-Agnostic**: Unified interface for MLX, PEFT, OpenAI, DeepSeek
5. **Continuous Learning**: PersonaUser auto-generates training tasks
6. **Production-Ready**: Monitoring, metrics, error handling, cost tracking

**Next Steps**:
1. Create entity files in `system/data/entities/`
2. Implement `training/prepare` command
3. Implement `training/start` command
4. Integrate with `PEFTLoRAAdapter`
5. Test end-to-end with TinyLlama

---

**Document Version**: 1.0
**Last Updated**: November 7, 2025
**Status**: Ready for Implementation
