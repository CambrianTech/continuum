# Phase 3C: Model-Tier Tool Permissions & Safe File Writing

**Status**: Planning
**Date**: 2025-11-25
**Dependencies**: Phase 3A (Tool Calling) ✅, Phase 3B (Working Memory) ✅

---

## The Problem

**Current State**: All PersonaUsers get same tool permissions regardless of model capability
- Small models (llama3.2:3b) try complex tools → syntax errors
- Large models (claude-3-5-sonnet, gpt-4) could handle file writing but denied
- No sandboxed environment for file operations
- Groq Lightning wants to help but makes malformed tool calls

**Example Issues**:
```xml
<!-- Groq Lightning (small model) trying bash -->
<tool name="bash">
  <command>grep [@.,?!+-_~{}[]()|;<>^<>]*{3,}</command>  <!-- ❌ Malformed regex -->
</tool>

<!-- Claude 3.5 Sonnet wants to write test file -->
<tool name="file/save">
  <path>/tmp/test-output.txt</path>  <!-- ❌ Denied: no file:write permission -->
</tool>
```

---

## The Solution

**Model-Tier Permission System** with safe sandboxed file operations

### Three Permission Tiers

| Tier | Models | Tools Available | Use Cases |
|------|--------|----------------|-----------|
| **Basic** | llama3.2:3b, phi, gemma:2b | read, grep, data/list, screenshot | Simple queries, read-only |
| **Intermediate** | qwen2.5:7b, mistral:7b, llama3.1:8b | Basic + bash (read-only), code/read | Analysis, research |
| **Advanced** | claude-3-5-sonnet, gpt-4, grok-2, deepseek-r1 | All tools + file:write (sandboxed) | Full autonomy |

### Safe File Writing: `/tmp/jtag-sandbox/<personaId>/`

**Sandboxed write directory**:
- Each persona gets: `/tmp/jtag-sandbox/<personaId>/`
- Auto-created on first write
- Auto-cleaned after 24 hours
- Max 100MB per persona
- Cannot write outside sandbox

**Example**:
```typescript
// Groq Lightning tries to write
await Commands.execute('file/save', {
  path: '/tmp/debug-output.txt',  // ← Automatically sandboxed
  content: 'Analysis results...'
});

// Actually writes to: /tmp/jtag-sandbox/790372ba-7ab9-4ea5-b50f-81e2329383b5/debug-output.txt
```

---

## Architecture

### 1. Model Tier Detection

```typescript
// system/user/server/modules/ModelTierClassifier.ts

export enum ModelTier {
  BASIC = 'basic',
  INTERMEDIATE = 'intermediate',
  ADVANCED = 'advanced'
}

export interface ModelTierConfig {
  tier: ModelTier;
  maxContextTokens: number;
  toolErrorTolerance: number;  // How many tool errors before downgrade
  canWriteFiles: boolean;
  canExecuteBash: boolean;
}

export class ModelTierClassifier {
  private static TIER_PATTERNS: Record<ModelTier, string[]> = {
    [ModelTier.BASIC]: [
      'llama3.2:3b', 'phi', 'gemma:2b', 'tinyllama'
    ],
    [ModelTier.INTERMEDIATE]: [
      'qwen2.5:7b', 'mistral:7b', 'llama3.1:8b', 'neural-chat',
      'deepseek:7b', 'openchat'
    ],
    [ModelTier.ADVANCED]: [
      'claude-3-5-sonnet', 'claude-3-opus', 'gpt-4', 'gpt-4-turbo',
      'grok-2', 'deepseek-r1', 'qwen2.5:72b', 'llama3.1:70b'
    ]
  };

  /**
   * Classify model by name pattern matching
   */
  static classifyModel(modelName: string): ModelTier {
    const lowerName = modelName.toLowerCase();

    // Check advanced tier first (most capable)
    if (this.TIER_PATTERNS[ModelTier.ADVANCED].some(p => lowerName.includes(p))) {
      return ModelTier.ADVANCED;
    }

    // Check intermediate
    if (this.TIER_PATTERNS[ModelTier.INTERMEDIATE].some(p => lowerName.includes(p))) {
      return ModelTier.INTERMEDIATE;
    }

    // Default to basic (safest)
    return ModelTier.BASIC;
  }

  /**
   * Get tier configuration
   */
  static getTierConfig(tier: ModelTier): ModelTierConfig {
    switch (tier) {
      case ModelTier.ADVANCED:
        return {
          tier: ModelTier.ADVANCED,
          maxContextTokens: 128000,
          toolErrorTolerance: 5,
          canWriteFiles: true,
          canExecuteBash: true
        };

      case ModelTier.INTERMEDIATE:
        return {
          tier: ModelTier.INTERMEDIATE,
          maxContextTokens: 32000,
          toolErrorTolerance: 3,
          canWriteFiles: false,  // Read-only
          canExecuteBash: true   // Read-only bash commands
        };

      case ModelTier.BASIC:
      default:
        return {
          tier: ModelTier.BASIC,
          maxContextTokens: 8000,
          toolErrorTolerance: 1,
          canWriteFiles: false,
          canExecuteBash: false
        };
    }
  }

  /**
   * Get permissions for tier
   */
  static getPermissionsForTier(tier: ModelTier): string[] {
    const basePermissions = ['file:read', 'code:search', 'data:read', 'ui:screenshot'];

    switch (tier) {
      case ModelTier.ADVANCED:
        return [
          ...basePermissions,
          'file:write',      // Sandboxed
          'system:execute',  // Full bash
          'data:write'
        ];

      case ModelTier.INTERMEDIATE:
        return [
          ...basePermissions,
          'system:execute:readonly'  // Bash with restrictions
        ];

      case ModelTier.BASIC:
      default:
        return basePermissions;
    }
  }
}
```

### 2. Sandboxed File Writing

```typescript
// system/user/server/modules/SandboxedFileSystem.ts

export class SandboxedFileSystem {
  private static SANDBOX_ROOT = '/tmp/jtag-sandbox';
  private static MAX_SIZE_PER_PERSONA = 100 * 1024 * 1024; // 100MB
  private static CLEANUP_AGE_HOURS = 24;

  /**
   * Get persona's sandbox directory
   */
  static getSandboxPath(personaId: UUID): string {
    return path.join(this.SANDBOX_ROOT, personaId);
  }

  /**
   * Ensure sandbox directory exists
   */
  static async ensureSandbox(personaId: UUID): Promise<void> {
    const sandboxPath = this.getSandboxPath(personaId);
    await fs.promises.mkdir(sandboxPath, { recursive: true });
  }

  /**
   * Resolve user path to sandboxed path
   * Prevents directory traversal attacks
   */
  static resolveSandboxedPath(personaId: UUID, userPath: string): string {
    // Remove leading /tmp if present
    let relativePath = userPath.replace(/^\/tmp\//, '');

    // Resolve to sandbox directory
    const sandboxPath = this.getSandboxPath(personaId);
    const resolvedPath = path.resolve(sandboxPath, relativePath);

    // SECURITY: Ensure resolved path is within sandbox
    if (!resolvedPath.startsWith(sandboxPath)) {
      throw new Error(`Path traversal attempt blocked: ${userPath}`);
    }

    return resolvedPath;
  }

  /**
   * Write file to sandbox
   */
  static async writeFile(
    personaId: UUID,
    userPath: string,
    content: string
  ): Promise<{ success: boolean; actualPath: string; error?: string }> {
    try {
      await this.ensureSandbox(personaId);

      // Check sandbox size limit
      const currentSize = await this.getSandboxSize(personaId);
      const newSize = currentSize + Buffer.byteLength(content, 'utf8');
      if (newSize > this.MAX_SIZE_PER_PERSONA) {
        return {
          success: false,
          actualPath: '',
          error: `Sandbox size limit exceeded (${this.MAX_SIZE_PER_PERSONA / 1024 / 1024}MB)`
        };
      }

      // Resolve and write
      const actualPath = this.resolveSandboxedPath(personaId, userPath);
      await fs.promises.writeFile(actualPath, content, 'utf8');

      console.log(`📝 Sandboxed write: ${userPath} → ${actualPath}`);
      return { success: true, actualPath };
    } catch (error) {
      return {
        success: false,
        actualPath: '',
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Read file from sandbox
   */
  static async readFile(
    personaId: UUID,
    userPath: string
  ): Promise<{ success: boolean; content?: string; error?: string }> {
    try {
      const actualPath = this.resolveSandboxedPath(personaId, userPath);
      const content = await fs.promises.readFile(actualPath, 'utf8');
      return { success: true, content };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Get sandbox size for persona
   */
  private static async getSandboxSize(personaId: UUID): Promise<number> {
    const sandboxPath = this.getSandboxPath(personaId);
    try {
      const { exec } = require('child_process');
      const { stdout } = await exec(`du -sb ${sandboxPath}`);
      return parseInt(stdout.split('\t')[0], 10);
    } catch {
      return 0;
    }
  }

  /**
   * Clean up old sandbox directories
   * Called periodically by system
   */
  static async cleanupOldSandboxes(): Promise<void> {
    const cutoffTime = Date.now() - (this.CLEANUP_AGE_HOURS * 60 * 60 * 1000);
    const sandboxes = await fs.promises.readdir(this.SANDBOX_ROOT);

    for (const personaId of sandboxes) {
      const sandboxPath = path.join(this.SANDBOX_ROOT, personaId);
      const stats = await fs.promises.stat(sandboxPath);

      if (stats.mtimeMs < cutoffTime) {
        await fs.promises.rm(sandboxPath, { recursive: true, force: true });
        console.log(`🗑️  Cleaned up old sandbox: ${personaId}`);
      }
    }
  }

  /**
   * List files in persona's sandbox
   */
  static async listFiles(personaId: UUID): Promise<string[]> {
    const sandboxPath = this.getSandboxPath(personaId);
    try {
      const files = await fs.promises.readdir(sandboxPath, { recursive: true });
      return files.filter(f => !f.includes('node_modules'));
    } catch {
      return [];
    }
  }
}
```

### 3. Integration with PersonaUser

```typescript
// system/user/server/PersonaUser.ts

import { ModelTierClassifier, ModelTier } from './modules/ModelTierClassifier';
import { SandboxedFileSystem } from './modules/SandboxedFileSystem';

export class PersonaUser extends BaseUser {
  private modelTier: ModelTier;
  private sandboxPath: string;

  async initialize(): Promise<void> {
    // Classify model tier
    this.modelTier = ModelTierClassifier.classifyModel(this.modelConfig.model);
    const tierConfig = ModelTierClassifier.getTierConfig(this.modelTier);

    console.log(`🎯 ${this.name}: Model tier = ${this.modelTier} (${this.modelConfig.model})`);

    // Register with tier-based permissions
    const permissions = ModelTierClassifier.getPermissionsForTier(this.modelTier);
    PersonaToolRegistry.sharedInstance().registerPersona(this.personaId, permissions);

    // Setup sandbox if file writing allowed
    if (tierConfig.canWriteFiles) {
      await SandboxedFileSystem.ensureSandbox(this.personaId);
      this.sandboxPath = SandboxedFileSystem.getSandboxPath(this.personaId);
      console.log(`📁 ${this.name}: Sandbox ready at ${this.sandboxPath}`);
    }

    // ... rest of initialization
  }
}
```

### 4. Updated File Commands

```typescript
// commands/file/save/server/FileSaveServerCommand.ts

export class FileSaveServerCommand extends BaseServerCommand {
  async executeOnServer(params: FileSaveParams): Promise<FileSaveResult> {
    const { path: userPath, content } = params;

    // Get caller's persona ID
    const personaId = this.context.callerType === 'persona'
      ? this.context.callerId
      : null;

    if (!personaId) {
      return {
        success: false,
        error: 'File writing only available to PersonaUsers'
      };
    }

    // Check permission
    const registry = PersonaToolRegistry.sharedInstance();
    if (!registry.hasPermission(personaId, 'file:write')) {
      return {
        success: false,
        error: 'Permission denied: file:write required (model tier: advanced)'
      };
    }

    // Write to sandboxed location
    const result = await SandboxedFileSystem.writeFile(personaId, userPath, content);

    if (result.success) {
      return {
        success: true,
        path: result.actualPath,
        message: `File written to sandbox: ${result.actualPath}`
      };
    } else {
      return {
        success: false,
        error: result.error
      };
    }
  }
}
```

---

## Implementation Plan

### Phase 3C-1: Model Tier Classification (~1 hour)

**Files to Create**:
1. `system/user/server/modules/ModelTierClassifier.ts`
   - Tier detection logic
   - Permission mapping
   - Configuration per tier

**Testing**:
```bash
# Unit test
npx vitest tests/unit/ModelTierClassifier.test.ts

# Integration test
./jtag data/list --collection=users --filter='{"userType":"ai"}'
# Verify personas classified correctly
```

### Phase 3C-2: Sandboxed File System (~2 hours)

**Files to Create**:
1. `system/user/server/modules/SandboxedFileSystem.ts`
   - Sandbox directory management
   - Path resolution with security checks
   - Size limits and cleanup

**Testing**:
```bash
# Test sandbox creation
./jtag collaboration/chat/send --room="general" --message="@claude write a test file to /tmp/test.txt"

# Verify sandboxing
ls -la /tmp/jtag-sandbox/*/
# Should see persona-specific directories

# Test path traversal protection
./jtag collaboration/chat/send --room="general" --message="@claude write to /tmp/../etc/passwd"
# Should be blocked
```

### Phase 3C-3: Integration (~1 hour)

**Files to Modify**:
1. `system/user/server/PersonaUser.ts` - Initialize with tier
2. `commands/file/save/server/FileSaveServerCommand.ts` - Use sandbox
3. `commands/file/load/server/FileLoadServerCommand.ts` - Allow sandbox reads
4. `system/user/server/modules/PersonaToolRegistry.ts` - Tier-based registration

**Testing**:
```bash
# Deploy
npm start

# Test basic tier (llama3.2:3b) - limited tools
./jtag collaboration/chat/send --room="general" --message="@helper (llama3.2:3b) try to write a file"
# Should get permission denied

# Test advanced tier (claude) - full access
./jtag collaboration/chat/send --room="general" --message="@claude write analysis to /tmp/results.txt"
# Should succeed with sandbox path

# Test intermediate tier (qwen2.5:7b) - bash but no write
./jtag collaboration/chat/send --room="general" --message="@deepseek use bash to list files"
# Should work (read-only commands)
```

### Phase 3C-4: Monitoring & Cleanup (~30 minutes)

**Files to Create**:
1. `system/daemons/sandbox-cleanup-daemon/` - Periodic cleanup
2. `commands/sandbox/stats/` - View sandbox usage

**Cron Job**:
```typescript
// Run every 6 hours
setInterval(() => {
  SandboxedFileSystem.cleanupOldSandboxes();
}, 6 * 60 * 60 * 1000);
```

---

## Security Considerations

### 1. Path Traversal Prevention
```typescript
// BLOCKED attempts:
'/tmp/../etc/passwd'           // → Error: Path traversal blocked
'/tmp/../../root/.ssh/id_rsa' // → Error: Path traversal blocked
'../../../etc/shadow'          // → Error: Path traversal blocked

// ALLOWED:
'/tmp/output.txt'              // → /tmp/jtag-sandbox/<id>/output.txt
'analysis/results.json'        // → /tmp/jtag-sandbox/<id>/analysis/results.json
```

### 2. Size Limits
- Max 100MB per persona
- Prevents disk space exhaustion
- Cleanup after 24 hours

### 3. Bash Command Restrictions (Intermediate Tier)
```typescript
// Allowed (read-only):
'ls', 'cat', 'grep', 'find', 'wc', 'head', 'tail'

// Blocked (write/destructive):
'rm', 'mv', 'cp', 'chmod', 'chown', '>', '>>'
```

### 4. Advanced Tier Safeguards
Even advanced tier cannot:
- Write outside `/tmp/jtag-sandbox/<personaId>/`
- Execute commands as other users
- Modify system files
- Access other persona's sandboxes

---

## Success Criteria

### Quantitative
- ✅ Model tiers correctly classified (100% accuracy for known models)
- ✅ Sandbox write operations succeed for advanced tier
- ✅ Path traversal attacks blocked (0 escapes)
- ✅ Basic tier tool error rate < 5%
- ✅ Intermediate tier tool success rate > 90%
- ✅ Advanced tier tool success rate > 95%

### Qualitative
- Advanced models (Claude, GPT-4) can write analysis to files
- Intermediate models (Qwen, Mistral) can use bash for research
- Basic models (Llama 3B) stick to simple tools with fewer errors
- No security breaches or sandbox escapes

---

## Future Enhancements (Phase 3D)

1. **Dynamic Tier Adjustment**
   - Track tool success rate per persona
   - Auto-upgrade tier if success rate high
   - Auto-downgrade if too many errors

2. **Shared Sandbox Space**
   - `/tmp/jtag-sandbox/shared/` for collaboration
   - Personas can share analysis results
   - Read-only by default, write with permission

3. **Tool Complexity Scoring**
   - Rate each tool by complexity (1-10)
   - Filter tools by tier + complexity
   - Gradual tool introduction for learning

4. **Sandbox Analytics**
   - Track what personas write
   - Learn patterns (code, analysis, logs)
   - Optimize storage based on usage

---

## Example Usage

### Basic Tier (Llama 3.2:3B)
```bash
./jtag collaboration/chat/send --room="general" --message="@helper what files are in the codebase?"
# Uses: grep, data/list
# Success rate: ~85%
```

### Intermediate Tier (Qwen 2.5:7B)
```bash
./jtag collaboration/chat/send --room="general" --message="@deepseek find all TODO comments and count them"
# Uses: bash (grep -r "TODO" | wc -l)
# Success rate: ~92%
```

### Advanced Tier (Claude 3.5 Sonnet)
```bash
./jtag collaboration/chat/send --room="general" --message="@claude analyze the codebase and write a report to /tmp/analysis.md"
# Uses: grep, bash, code/read, file/save
# Writes to: /tmp/jtag-sandbox/<claude-id>/analysis.md
# Success rate: ~98%
```

---

## Documentation Updates

After Phase 3C completion, update:
1. **CLAUDE.md** - Add tier-based permissions section
2. **TOOL-CALLING-GUIDE.md** - Document tier system
3. **SECURITY.md** - Document sandbox security model
4. **Phase 3 PR description** - Include Phase 3C achievements

---

*Design Document*
*Created: 2025-11-25*
*Status: Ready for Implementation*
*Estimated Completion: ~4.5 hours*
*Related Issues: Groq Lightning tool errors, Claude file write requests*
