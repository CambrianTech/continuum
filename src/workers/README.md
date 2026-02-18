# Rust Workers - Modular Self-Defining Registry

## Architecture Philosophy

**Each worker defines itself** - like Rust's Cargo workspace pattern:
- Worker config lives WITH the worker (not in central registry)
- Generator discovers all workers automatically
- Adding a worker = create worker.config.ts in worker directory
- Zero coupling - workers don't know about each other

```
workers/
├── archive/              ← Cold storage archival
├── continuum-core/       ← Unified runtime (Voice, Data, Embedding, Search, Logger)
├── inference-grpc/       ← LLM inference (gRPC, separate for memory isolation)
├── models/               ← Model weights (piper, whisper, vad)
└── shared/               ← Shared Rust code

generator/generate-worker-registry.ts discovers all worker.config.ts files
    ↓
Generates workers-config.json (for bash) and WorkerRegistry.ts (for TypeScript)
    ↓
Both consumers use generated files (NEVER edit generated files!)
```

## Worker Configuration Pattern

Each worker has a `worker.config.ts` in its directory:

**Example**: `workers/archive/worker.config.ts`

```typescript
/**
 * Archive Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

export default {
  name: 'archive',
  binary: 'workers/archive/target/release/archive-worker',
  socket: '/tmp/jtag-archive-worker.sock',
  args: [
    '/tmp/jtag-command-router.sock',
    '.continuum/jtag/data/database.sqlite',
    '.continuum/jtag/data/archive/database-001.sqlite'
  ],
  description: 'Archive worker for moving old data to cold storage using Commands.execute()',
  enabled: true
} as const;

export type ArchiveWorkerConfig = typeof import('./worker.config').default;
```

### Field Definitions

- **`name`**: Worker identifier (used for process matching)
- **`binary`**: Path to compiled Rust binary (relative to jtag root)
- **`socket`**: Unix socket path for IPC
- **`args`**: Array of arguments passed to worker
- **`description`**: Human-readable description
- **`enabled`**: Optional (default: true), set false to disable worker

## Generated Files (DO NOT EDIT MANUALLY)

### 1. `workers/workers-config.json` (for bash scripts)

Used by start-workers.sh and stop-workers.sh. Auto-generated from worker.config.ts files.

### 2. `shared/types/WorkerRegistry.ts` (for TypeScript)

Provides type-safe worker names and socket paths:

```typescript
// Auto-generated types
export const WORKER_NAMES = ['archive', 'continuum-core', 'inference-grpc'] as const;
export type WorkerName = typeof WORKER_NAMES[number];

export const WORKER_SOCKETS: Record<WorkerName, string> = {
  'archive': '/tmp/jtag-archive-worker.sock',
  'continuum-core': '/tmp/continuum-core.sock',
  'inference-grpc': 'tcp://localhost:50051'
} as const;

export const ENABLED_WORKERS = ['archive', 'continuum-core', 'inference-grpc'] as const;
```

## Usage

### Start Workers
```bash
npm run worker:start
# Or directly:
./workers/start-workers.sh
```

**What it does**:
1. Reads `workers-config.json` (auto-generated)
2. Builds all enabled workers with `cargo build --release`
3. Kills existing workers (clean slate)
4. Removes old socket files
5. Starts each enabled worker with specified args
6. Waits for socket creation
7. Verifies all running

### Stop Workers
```bash
npm stop
# Or directly:
./workers/stop-workers.sh
```

**What it does**:
1. Reads `workers-config.json` (auto-generated)
2. Kills all workers by name
3. Removes all socket files (including shared sockets)
4. Verifies stopped

### Regenerate Registry
```bash
npx tsx generator/generate-worker-registry.ts
```

**Runs automatically during**:
- `npm run prebuild` (before builds)
- Development workflow

## Adding a New Worker

**Step 1**: Create worker directory with `worker.config.ts`

```bash
mkdir workers/my-new-worker
```

**Step 2**: Add `worker.config.ts` to worker directory

```typescript
/**
 * My New Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

export default {
  name: 'my-new-worker',
  binary: 'workers/my-new-worker/target/release/my-new-worker',
  socket: '/tmp/jtag-my-new-worker.sock',
  args: ['--some-flag'],
  description: 'Does something cool',
  enabled: true
} as const;

export type MyNewWorkerConfig = typeof import('./worker.config').default;
```

**Step 3**: Run generator (or it runs automatically on prebuild)

```bash
npx tsx generator/generate-worker-registry.ts
```

**That's it!** The worker is now:
- ✅ Discovered by generator
- ✅ Added to workers-config.json
- ✅ Added to WorkerRegistry.ts types
- ✅ Available in start/stop scripts

## Temporarily Disabling a Worker

Edit the worker's `worker.config.ts`:

```typescript
export default {
  name: 'data-daemon',
  enabled: false,  // ← Disable worker
  // ...
} as const;
```

Then regenerate (or wait for automatic regeneration):
```bash
npx tsx generator/generate-worker-registry.ts
```

## Benefits of Modular Pattern

✅ **Self-contained** - Worker config lives with worker code
✅ **Zero coupling** - Workers don't know about each other
✅ **Dynamic discovery** - File system scanning finds workers
✅ **Type-safe** - Generated TypeScript types for compile-time safety
✅ **Single source of truth** - worker.config.ts is canonical
✅ **Easy to add** - Just create worker.config.ts
✅ **Easy to disable** - Set enabled flag in config
✅ **Self-documenting** - Each worker documents itself

## Architecture Comparison

### ❌ Old Pattern (Anti-Pattern)
```typescript
// Central registry that breaks modularity
export const WORKERS = {
  archive: { ... },
  logger: { ... }
  // Adding a worker requires editing this file
};
```

### ✅ New Pattern (Modular)
```typescript
// Each worker defines itself
// workers/archive/worker.config.ts
export default { name: 'archive', ... };

// Generator discovers all configs automatically
// Adding a worker = just create worker.config.ts
```

## Integration with Build System

The generator runs automatically during prebuild:

```json
{
  "prebuild": "... && npx tsx generator/generate-worker-registry.ts && ..."
}
```

This ensures:
- Registry is always up-to-date
- TypeScript types match worker configs
- No manual synchronization needed

## TypeScript Type Safety

Use generated types in your code:

```typescript
import { WorkerName, WORKER_SOCKETS } from '@shared/types/WorkerRegistry';

// Type-safe worker names
const workerName: WorkerName = 'archive'; // ✅ Autocomplete!

// Type-safe socket paths
const socket = WORKER_SOCKETS.archive; // ✅ '/tmp/jtag-archive-worker.sock'

// Compile error for invalid workers
const invalid: WorkerName = 'nonexistent'; // ❌ TypeScript error!
```

## Requirements

- **jq**: JSON processor for bash scripts
  ```bash
  brew install jq
  ```

## Troubleshooting

### Worker won't start
1. Check binary path in worker.config.ts exists: `ls workers/*/target/release/*-worker`
2. Check socket isn't already in use: `ls /tmp/jtag-*.sock`
3. Regenerate registry: `npx tsx generator/generate-worker-registry.ts`
4. Rebuild worker: `cargo build --release` in worker directory

### Worker not discovered by generator
1. Verify `worker.config.ts` exists in worker directory
2. Verify it exports default object with required fields
3. Check generator output: `npx tsx generator/generate-worker-registry.ts`

### Worker won't stop
1. Manually kill: `pkill -f <worker-name>-worker`
2. Remove socket: `rm /tmp/jtag-<worker-name>.sock`
3. Check for orphans: `ps aux | grep worker`

### Generated files out of sync
1. Regenerate: `npx tsx generator/generate-worker-registry.ts`
2. Or: `npm run prebuild` (regenerates everything)

### TypeScript compilation errors after adding worker
1. Regenerate registry: `npx tsx generator/generate-worker-registry.ts`
2. Restart TypeScript server in your editor
3. Check `shared/types/WorkerRegistry.ts` was updated

---

**Bottom line**: Workers define themselves, generator discovers them, both bash and TypeScript use generated configs. Add workers by creating worker.config.ts, not editing central registries!
