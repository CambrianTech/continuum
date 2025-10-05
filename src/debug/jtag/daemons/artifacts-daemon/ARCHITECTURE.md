# ArtifactsDaemon Migration Architecture
**Created**: 2025-10-05
**Status**: Design Phase
**Goal**: Centralize ALL filesystem access through ArtifactsDaemon for security, access control, and Grid compatibility

---

## 🎯 Problem Statement

**Current State**: 163 files directly import `fs` module, bypassing ArtifactsDaemon
- No centralized access control
- Path validation scattered everywhere
- Grid compatibility blocked (browser can't use `fs`)
- Security risk (unrestricted filesystem access)
- Config management inconsistent

**Target State**: ALL filesystem operations go through ArtifactsDaemon
- Single chokepoint for access control
- Centralized path validation via `validateAndResolvePath()`
- Grid-compatible (ArtifactsDaemon routes browser→server)
- Secure (permission checks at single point)
- Config loading unified

---

## 🏛️ Architecture Design

### Core Principle: FileSystem Abstraction Layer

```
┌──────────────────────────────────────┐
│   Commands, Daemons, Adapters       │
│   (NO direct fs imports)             │
└────────────────┬─────────────────────┘
                 │ uses
                 ▼
         ┌──────────────┐
         │ Artifacts API │ (system/core/artifacts/ArtifactsAPI.ts)
         └──────┬───────┘
                │ delegates to
                ▼
      ┌─────────────────────┐
      │  ArtifactsDaemon    │
      │  (shared/server)     │
      └─────────┬───────────┘
                │ server-only
                ▼
         ┌────────────┐
         │ fs module  │
         └────────────┘
```

### Artifacts API (Convenience Layer)

Create `system/core/artifacts/ArtifactsAPI.ts`:
```typescript
import { JTAGClient } from '../client/JTAGClient';
import type { StorageType } from '../../../daemons/artifacts-daemon/shared/ArtifactsDaemon';

/**
 * High-level filesystem API that routes through ArtifactsDaemon
 * Works in ANY environment (browser/server/Grid)
 */
export class ArtifactsAPI {
  constructor(private client: JTAGClient) {}

  async read(relativePath: string, storageType: StorageType = 'system'): Promise<string> {
    const result = await this.client.daemons.artifacts.execute('read', {
      relativePath,
      storageType,
      context: this.client.context,
      sessionId: this.client.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to read ${relativePath}: ${result.error}`);
    }

    return result.data;
  }

  async write(relativePath: string, content: string, storageType: StorageType = 'system'): Promise<void> {
    const result = await this.client.daemons.artifacts.execute('write', {
      relativePath,
      content,
      storageType,
      context: this.client.context,
      sessionId: this.client.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to write ${relativePath}: ${result.error}`);
    }
  }

  async append(relativePath: string, content: string, storageType: StorageType = 'system'): Promise<void> {
    const result = await this.client.daemons.artifacts.execute('append', {
      relativePath,
      content,
      storageType,
      context: this.client.context,
      sessionId: this.client.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to append ${relativePath}: ${result.error}`);
    }
  }

  async list(relativePath: string, storageType: StorageType = 'system'): Promise<string[]> {
    const result = await this.client.daemons.artifacts.execute('list', {
      relativePath,
      storageType,
      context: this.client.context,
      sessionId: this.client.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to list ${relativePath}: ${result.error}`);
    }

    return result.data;
  }

  async exists(relativePath: string, storageType: StorageType = 'system'): Promise<boolean> {
    try {
      await this.read(relativePath, storageType);
      return true;
    } catch {
      return false;
    }
  }

  async mkdir(relativePath: string, storageType: StorageType = 'system'): Promise<void> {
    const result = await this.client.daemons.artifacts.execute('mkdir', {
      relativePath,
      storageType,
      context: this.client.context,
      sessionId: this.client.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to mkdir ${relativePath}: ${result.error}`);
    }
  }

  async delete(relativePath: string, storageType: StorageType = 'system'): Promise<void> {
    const result = await this.client.daemons.artifacts.execute('delete', {
      relativePath,
      storageType,
      context: this.client.context,
      sessionId: this.client.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to delete ${relativePath}: ${result.error}`);
    }
  }
}

// Export singleton instance
let _artifactsAPI: ArtifactsAPI | null = null;

export function getArtifactsAPI(): ArtifactsAPI {
  if (!_artifactsAPI) {
    const client = JTAGClient.sharedInstance();
    _artifactsAPI = new ArtifactsAPI(client);
  }
  return _artifactsAPI;
}
```

---

## 📋 Migration Priority (Critical Path)

### Phase 1: Config & Core Infrastructure (THIS SPRINT - Day 0.5)
**Priority: CRITICAL** - Blocks PersonaUser AI integration

1. **Config Loading** (`system/shared/Config.ts`)
   - Add `'config'` StorageType to ArtifactsDaemon
   - Implement `loadEnvironment()` operation
   - Parse `~/.continuum/config.env` KEY=value lines
   - Set `process.env` variables

2. **SQLite Adapter** (`system/data/adapters/SQLiteAdapter.ts`)
   - Replace `fs.existsSync()` → `artifacts.exists()`
   - Replace `fs.mkdirSync()` → `artifacts.mkdir()`
   - Database file access already goes through proper paths

3. **JsonFile Adapter** (`system/data/adapters/JsonFileAdapter.ts`)
   - Replace `fs.readFile()` → `artifacts.read()`
   - Replace `fs.writeFile()` → `artifacts.write()`
   - Replace `fs.readdir()` → `artifacts.list()`

---

### Phase 2: Commands (DEFER TO NEXT SPRINT)
**Priority: MEDIUM** - Improve consistency, not blocking

4. **file/save Command** (`commands/file/save/server/FileSaveServerCommand.ts`)
   - Already delegates to ArtifactsDaemon via `file/save` command
   - Check if double-handling exists

5. **file/append Command** (`commands/file/append/server/FileAppendServerCommand.ts`)
   - Already delegates to ArtifactsDaemon via `file/append` command
   - Check if double-handling exists

6. **data/* Commands** (`commands/data/*/server/*`)
   - Should NOT have fs access at all
   - Should delegate to DataDaemon
   - Audit and remove any direct fs usage

---

### Phase 3: System Components (DEFER TO TECHNICAL DEBT SPRINT)
**Priority: LOW** - Works fine, just not ideal

7. **Process Coordinator, Registry, TmuxSession** (40+ files)
   - Low-level system initialization
   - Not Grid-compatible anyway (server-only)
   - Can wait for major refactor

---

## 🔨 Implementation Plan

### Step 1: Add ArtifactsAPI Convenience Layer
```bash
# Create convenience API
touch system/core/artifacts/ArtifactsAPI.ts

# Wire into JTAGClient
# Add: public artifacts: ArtifactsAPI;
```

### Step 2: Add 'config' StorageType
```typescript
// In daemons/artifacts-daemon/shared/ArtifactsDaemon.ts

export type StorageType =
  | 'database'   // $HOME/.continuum/database/
  | 'session'    // .continuum/jtag/sessions/user/{sessionId}/
  | 'system'     // .continuum/jtag/system/
  | 'cache'      // .continuum/cache/
  | 'logs'       // .continuum/logs/
  | 'config';    // $HOME/.continuum/ (for config.env)

// In validateAndResolvePath():
case 'config':
  // Global user config: $HOME/.continuum/
  basePath = process.env.HOME + '/.continuum';
  break;
```

### Step 3: Add loadEnvironment() Operation
```typescript
// In daemons/artifacts-daemon/server/ArtifactsDaemonServer.ts

protected async handleLoadEnvironment(payload: ArtifactsPayload): Promise<ArtifactsResult> {
  try {
    // Read config.env from $HOME/.continuum/
    const configPath = path.join(process.env.HOME!, '.continuum', 'config.env');

    if (!fs.existsSync(configPath)) {
      return {
        success: true,
        data: { loaded: 0, message: 'No config.env found' }
      };
    }

    const content = await fs.promises.readFile(configPath, 'utf-8');
    const lines = content.split('\n');
    let loaded = 0;

    for (const line of lines) {
      const trimmed = line.trim();

      // Skip empty lines and comments
      if (!trimmed || trimmed.startsWith('#')) continue;

      // Parse KEY=value
      const match = trimmed.match(/^([A-Z_][A-Z0-9_]*)=(.*)$/);
      if (match) {
        const [, key, value] = match;
        process.env[key] = value;
        loaded++;
        console.log(`🔑 Loaded env var: ${key}`);
      }
    }

    return {
      success: true,
      data: { loaded, path: configPath }
    };
  } catch (error: any) {
    return {
      success: false,
      error: `Failed to load environment: ${error.message}`
    };
  }
}
```

### Step 4: Call on Server Startup
```typescript
// In server/jtag-server.ts (or wherever server initializes)

async function initializeServer() {
  // ... existing initialization ...

  // Load environment variables from ~/.continuum/config.env
  const artifactsDaemon = daemonRegistry.get('artifacts');
  const envResult = await artifactsDaemon.handleMessage({
    payload: {
      operation: 'loadEnvironment',
      context: { environment: 'server' },
      sessionId: 'system'
    }
  });

  if (envResult.success) {
    console.log(`✅ Loaded ${envResult.data.loaded} environment variables`);
  }

  // ... continue initialization ...
}
```

### Step 5: Migrate SQLiteAdapter
```typescript
// Before:
import * as fs from 'fs';
import * as path from 'path';

if (!fs.existsSync(dbDir)) {
  fs.mkdirSync(dbDir, { recursive: true });
}

// After:
import { getArtifactsAPI } from '../../../system/core/artifacts/ArtifactsAPI';

const artifacts = getArtifactsAPI();

if (!await artifacts.exists(dbPath, 'database')) {
  await artifacts.mkdir(dbPath, 'database');
}
```

### Step 6: Migrate JsonFileAdapter
```typescript
// Before:
import { promises as fs } from 'fs';

const fileContent = await fs.readFile(filePath, 'utf-8');
await fs.writeFile(filePath, JSON.stringify(data, null, 2), 'utf-8');
const files = await fs.readdir(collectionPath);

// After:
import { getArtifactsAPI } from '../../../system/core/artifacts/ArtifactsAPI';

const artifacts = getArtifactsAPI();

const fileContent = await artifacts.read(filePath, 'database');
await artifacts.write(filePath, JSON.stringify(data, null, 2), 'database');
const files = await artifacts.list(collectionPath, 'database');
```

---

## ✅ Success Criteria

### Phase 1 (THIS SPRINT):
- [ ] `process.env.OPENAI_API_KEY` populated on server startup
- [ ] `process.env.ANTHROPIC_API_KEY` populated on server startup
- [ ] SQLiteAdapter has zero `fs` imports
- [ ] JsonFileAdapter has zero `fs` imports
- [ ] Config.ts uses ArtifactsAPI instead of direct fs
- [ ] All migrations pass existing tests

### Phase 2 (NEXT SPRINT):
- [ ] file/* commands verified to delegate to ArtifactsDaemon
- [ ] data/* commands have zero `fs` imports
- [ ] All command tests pass

### Phase 3 (TECHNICAL DEBT):
- [ ] fs imports reduced from 163 to <10 (system-level only)
- [ ] ESLint rule: Forbid `import fs` outside ArtifactsDaemon
- [ ] Grid compatibility verified (browser can use all file operations)

---

## 🚨 Risk Mitigation

### Breaking Changes Prevention:
1. **Incremental Migration**: Migrate one adapter/command at a time
2. **Keep Tests Passing**: Run full test suite after each migration
3. **Fallback Support**: ArtifactsAPI can detect server-only and use fs directly as fallback during migration
4. **Feature Flags**: Add `ARTIFACTS_MIGRATION_MODE=strict|lenient` env var

### Performance Concerns:
- **Network Overhead**: ArtifactsDaemon routing adds ~1ms latency (negligible)
- **Caching**: ArtifactsDaemon can cache frequently accessed files
- **Batching**: Add batch operations for bulk file access

---

## 📊 Migration Tracking

**Total fs Imports**: 163
**Phase 1 Target**: 5 critical files (Config, SQLiteAdapter, JsonFileAdapter, file/save, file/append)
**Phase 2 Target**: 20 command files
**Phase 3 Target**: Remaining 138 system files

**Estimated Effort**:
- Phase 1: 4 hours (THIS SPRINT - Day 0.5 extended)
- Phase 2: 8 hours (NEXT SPRINT)
- Phase 3: 16 hours (TECHNICAL DEBT SPRINT)

**Total**: ~28 hours = 3.5 days of focused work

---

**Document Status**: Ready for implementation
**Next Action**: Create ArtifactsAPI.ts and implement Phase 1
