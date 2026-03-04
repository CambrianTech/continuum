# Rust Logger Worker - Complete Path Analysis

> **⚠️ HISTORICAL DOCUMENT**: This analysis was created before the logger worker was absorbed into
> `continuum-core` as `LoggerModule`. The standalone logger worker no longer exists. Logger commands
> now go to `/tmp/continuum-core.sock` with the `log/` prefix.

## Current Situation (OUTDATED)
- Server is running
- Workers directory exists but is EMPTY
- No socket file created
- Rust worker is NOT running

## File Locations - What Actually Exists

### Rust Worker Binary
```bash
# Expected location (per Logger.ts line 217):
workers/logger/target/release/logger-worker

# Check if it exists:
-rwx------  1 joel  staff  609968 Dec  9 19:43 workers/logger/target/release/logger-worker

# Check if debug version exists:
```

### Rust Source Files
```bash
# Main Rust source:
-rwx------  1 joel  staff  5122 Dec  9 17:38 workers/logger/src/main.rs

# Cargo.toml:
-rwx------  1 joel  staff  316 Dec  9 19:42 workers/logger/Cargo.toml
```

### TypeScript Integration Files
```bash
# Logger.ts (main integration point):
-rwx------  1 joel  staff  24518 Dec  9 20:28 system/core/logging/Logger.ts

# WorkerClient.ts (generic client):

# LoggerWorkerClient.ts (logger-specific client):
```

### Socket and Directory Paths
```bash
# Workers directory (should contain socket):
total 2048
drwx------  1 joel  staff  524288 Dec  9 20:29 .
drwx------  1 joel  staff  524288 Dec  9 20:29 ..

# Old socket location (should NOT exist):
srwxr-xr-x  1 joel  wheel  0 Dec  9 20:24 /tmp/logger-worker.sock
```

## Code Configuration - What Logger.ts Expects

### Socket Path (Logger.ts:175)
```typescript
const socketPath = path.join(process.cwd(), '.continuum', 'jtag', 'workers', 'logger.sock');
// Resolves to: /Volumes/FlashGordon/cambrian/continuum/src/.continuum/jtag/workers/logger.sock
```

### Binary Path (Logger.ts:217)
```typescript
const workerBinary = path.join(__dirname, '../../../workers/logger/target/release/logger-worker');
// __dirname = compiled location of Logger.ts
// Need to determine actual __dirname at runtime
```

### Startup Flow (Logger.ts:215-270)
1. Check if binary exists (line 220)
2. Create workers directory (line 230-239)
3. Remove old socket (line 242-249)
4. Spawn Rust process with socket path as argument (line 260-268)
5. Wait 1 second (line 208)
6. Connect TypeScript client to socket (line 188-207)

## Problem Analysis

### Issue 1: Binary Location
```
Logger.ts expects: __dirname/../../../workers/logger/target/release/logger-worker

__dirname is the COMPILED JavaScript location, not TypeScript source location.

Actual compiled path: likely dist/ or build/ directory
This means the relative path ../../../ might be wrong!
```

### Issue 2: Initialization Timing
```
Logger.ts initializes ONCE when singleton is created.
If binary doesn't exist or fails to start, it never retries.
System is already running = Logger already initialized = won't try again.
```

### Issue 3: Fallback Behavior
```
If Rust worker fails to start, Logger silently falls back to TypeScript logging.
No error visible to user.
System works fine without Rust worker.
```

## Diagnostic Commands

### Check Current Process State
```bash
# Is Logger trying to use Rust worker?
Current working directory: /Volumes/FlashGordon/cambrian/continuum/src

# Check if any logger-worker processes exist:
No logger-worker processes running

# Check Logger.ts compiled location:
./dist/system/core/logging/Logger.js
```

### Test Binary Manually
```bash
# Try to run the binary directly:
🦀 Rust Logger Worker starting...
📡 Listening on: --help
Error: Os { code: 45, kind: Uncategorized, message: "Operation not supported" }
```

## Next Steps - How to Actually Fix This

### Step 1: Build the Rust Binary
```bash
cd workers/logger
cargo build --release
# This creates: workers/logger/target/release/logger-worker
```

### Step 2: Determine Correct Binary Path
```bash
# Find where Logger.js actually is:
find . -name "Logger.js" -type f

# Calculate correct relative path from there to workers/logger/target/release/logger-worker
```

### Step 3: Fix Logger.ts Binary Path
```typescript
// Current (might be wrong):
const workerBinary = path.join(__dirname, '../../../workers/logger/target/release/logger-worker');

// Should probably be:
const workerBinary = path.join(process.cwd(), 'workers/logger/target/release/logger-worker');
// OR determine correct relative path based on actual __dirname
```

### Step 4: Restart System
```bash
# Only after fixing the binary path:
npm start
# This will reinitialize Logger and try to start Rust worker
```

### Step 5: Verify It Works
```bash
# Check if socket was created:
ls -la .continuum/jtag/workers/logger.sock

# Check if worker process is running:
ps aux | grep logger-worker

# Check logs for success/failure:
# (need to determine where logs go)
```

## Critical Questions to Answer

1. Does the Rust binary actually exist and is it executable?
2. Where is Logger.js compiled to? (affects __dirname)
3. Is the relative path ../../../ correct from that location?
4. Did Logger.ts already initialize before we made changes?
5. Are there any error messages in the logs we're missing?

