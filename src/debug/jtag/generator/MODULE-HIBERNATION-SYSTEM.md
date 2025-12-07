# Module Hibernation System

## Philosophy: Nothing Is Ever Lost

In JTAG, we never "delete" modules. Instead, we **hibernate** them - a non-destructive, reversible preservation that removes modules from the active system while keeping them fully restorable.

## Core Principle

```
Active ←→ Hibernated ←→ Archived
  ↑          ↑            ↑
  |          |            |
Running  Preserved   Long-term
System   (< 30d)     Storage
```

**Key insight**: Hibernation is not deletion - it's **temporal displacement**. The module exists outside the active system but remains fully accessible and instantly restorable.

## Why Hibernation?

### 1. Psychological Safety
No fear of losing work. Try experimental features knowing you can always restore them.

### 2. System Stability Without Data Loss
Failed modules can be immediately hibernated, system continues running, nothing lost.

### 3. Experimental Freedom
Generate → Test → Hibernate → Try alternative → Wake winner → Archive loser

### 4. Self-Healing Architecture
System automatically hibernates failures, preserves for analysis, can restore when fixed.

### 5. Perfect Memory
Every module ever created remains accessible through hibernation/archive system.

## Module States

### Active
- **Location**: `commands/`, `daemons/`, `widgets/`
- **State**: Running in system
- **Discovery**: Scanned by structure generator
- **Access**: Immediate execution

### Hibernated
- **Location**: `/tmp/jtag-hibernation/`
- **State**: Packaged as .tgz with metadata
- **Duration**: < 30 days typical
- **Restore**: Instant (1-2 seconds)
- **Purpose**: Temporary removal from active system

### Archived
- **Location**: `~/.jtag/module-archive/`
- **State**: Long-term storage
- **Duration**: > 30 days
- **Restore**: Still instant, just different location
- **Purpose**: Declutter hibernation, maintain history

## Architecture

### Module Package Structure

```
/tmp/jtag-hibernation/
├── chat-send.1765067828264.tgz       # Full module package
├── chat-send.1765067828264.metadata.json
└── experimental-ui.1765023445123.tgz
```

### Metadata Format

```json
{
  "module": "chat-send",
  "type": "command",
  "hibernatedAt": 1765067828264,
  "reason": "runtime-failure",
  "error": {
    "type": "NullPointerException",
    "message": "Cannot read property 'userId' of undefined",
    "stack": "..."
  },
  "dependencies": ["user-daemon", "data-daemon"],
  "dependents": ["chat-widget", "notification-command"],
  "usage": {
    "lastUsed": 1765067500000,
    "totalCalls": 1247,
    "avgCallsPerDay": 42
  },
  "lastWorkingVersion": "abc123def",
  "hibernatedBy": "ares",
  "packageSize": 2457600,
  "checksums": {
    "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }
}
```

## CLI Commands

### Hibernate a Module

```bash
# Manual hibernation
./jtag module/hibernate --name="old-feature" --reason="deprecated"

# Output:
# 📦 Packaging commands/old-feature...
# 💤 Hibernated to: /tmp/jtag-hibernation/old-feature.1765067828264.tgz
# 📝 Metadata saved with failure analysis
# ✅ Module removed from active system
# 🔄 Structure files regenerated (110 commands active)
# ♻️  Restorable with: ./jtag module/wake --name="old-feature"
```

### List Hibernated Modules

```bash
./jtag module/list-hibernated

# Output:
# 💤 Hibernated Modules (3):
#
# 1. old-feature (command)
#    Hibernated: 30 days ago
#    Reason: deprecated
#    Size: 2.3 MB
#    Usage: 42 calls/day (was active)
#    Restorable: ✅
#
# 2. experimental-ui (widget)
#    Hibernated: 5 hours ago
#    Reason: runtime-failure (NullPointerException)
#    Size: 1.1 MB
#    Hibernated by: ares (automatic)
#    Restorable: ✅
#
# 3. broken-daemon (daemon)
#    Hibernated: 60 days ago
#    Reason: architecture-redesign
#    Size: 4.5 MB
#    ⚠️  Will be archived in 30 days
#    Restorable: ✅
```

### Wake a Module

```bash
./jtag module/wake --name="experimental-ui"

# Output:
# ⏰ Waking experimental-ui from hibernation...
# 📦 Extracting: /tmp/jtag-hibernation/experimental-ui.1765067828264.tgz
# ✅ Restored to: widgets/experimental-ui
# 🔄 Structure files regenerated (12 widgets active)
# 🚀 Module ready to use!
#
# 💡 Tip: Check logs for original failure reason before using
```

### Archive Old Hibernations

```bash
./jtag module/archive --older-than=30

# Output:
# 📦 Archiving hibernations older than 30 days...
#
# Moving to ~/.jtag/module-archive/:
#   - old-feature (2.3 MB)
#   - deprecated-command (1.8 MB)
#
# ✅ Archived 2 modules
# 💾 Total archive size: 4.1 MB
# ♻️  Still restorable with: ./jtag module/wake --from-archive --name="old-feature"
```

### Restore from Archive

```bash
./jtag module/wake --from-archive --name="old-feature"

# Output:
# 📦 Restoring from archive...
# ⏰ Waking old-feature from long-term storage...
# ✅ Restored to: commands/old-feature
# 🚀 Module ready to use!
```

## Integration with Self-Healing System

### Ares Automatic Hibernation

When Ares detects a module failure:

```typescript
// Ares PersonaUser with system-administration LoRA active

async handleModuleFailure(module: FailedModule) {
  console.log(`🤖 Ares: Module ${module.name} crashed`);
  console.log(`   Error: ${module.error.message}`);

  // Assess complexity
  const assessment = await this.assessFailure(module);

  if (assessment.complexity === 'simple' && assessment.dependents.length === 0) {
    // Autonomous decision: hibernate immediately
    console.log(`💤 Ares: Hibernating ${module.name} (autonomous decision)`);
    console.log(`   Reason: Isolated failure, no dependents`);
    console.log(`   (Don't worry - fully restorable!)`);

    await this.hibernationSystem.hibernate({
      ...module,
      hibernatedBy: 'ares',
      reason: 'runtime-failure',
      failureDetails: module.error
    });

    console.log(`✅ System stable, ${module.name} preserved for analysis`);
  } else {
    // Complex case: consult Athena
    console.log(`🧠 Ares: Complex failure, consulting Athena...`);
    await this.consultAthena(module, assessment);
  }
}
```

### Periodic Review and Wake Candidates

Ares periodically reviews hibernated modules:

```typescript
async reviewHibernatedModules() {
  const hibernated = await this.hibernationSystem.listHibernated();

  for (const module of hibernated) {
    // Module been hibernated > 24 hours?
    if (module.age > 24 * 60 * 60 * 1000) {
      const analysis = await this.analyzeHibernation(module);

      if (analysis.likelyFixed) {
        console.log(`💡 Ares: ${module.name} might work now`);
        console.log(`   Original issue: ${module.reason}`);
        console.log(`   Analysis: ${analysis.reasoning}`);
        console.log(`   Risk of wake test: Low (can re-hibernate)`);

        // Propose to democratic vote
        await this.proposeDemocraticDecision({
          type: 'wake-and-test-module',
          module: module.name,
          reasoning: analysis.reasoning,
          risk: 'low'
        });
      }
    }
  }
}
```

## Hibernation Workflows

### Workflow 1: Failed Module Auto-Recovery

```
1. Module crashes during runtime
   ↓
2. Ares detects failure
   ↓
3. Ares assesses complexity
   ↓
4. Simple failure → Hibernate immediately
   ↓
5. System continues running (stable in < 2s)
   ↓
6. Module preserved with full metadata
   ↓
7. Ares periodically reviews for wake candidates
```

### Workflow 2: Experimental Feature Development

```
1. Generate experimental feature
   ./jtag generate experiment.json
   ↓
2. Test in active system
   npm start
   ↓
3. Doesn't work as expected
   ↓
4. Hibernate (not delete!)
   ./jtag module/hibernate --name="experiment"
   ↓
5. Try alternative approach
   ./jtag generate alternative.json
   ↓
6. Alternative works better
   ↓
7. Keep alternative active, experiment hibernated
   (Can wake experiment later if needed)
```

### Workflow 3: Dependency Management

```
1. Command A depends on Daemon B
   ↓
2. Daemon B crashes
   ↓
3. Ares detects: "Daemon B has dependents"
   ↓
4. Ares consults Athena (complex decision)
   ↓
5. Athena analyzes: "Hibernate entire dependency tree"
   ↓
6. Both Daemon B and Command A hibernated
   ↓
7. System stable, dependency tree preserved
   ↓
8. Fix Daemon B offline
   ↓
9. Wake both together when fixed
```

## Integration with Package System

Hibernation uses the same `npm pack` infrastructure:

```bash
# Hibernation is just structured packaging
cd commands/my-command
npm pack
mv *.tgz /tmp/jtag-hibernation/my-command.$(date +%s).tgz

# Wake is just structured extraction
tar -xzf /tmp/jtag-hibernation/my-command.*.tgz -C commands/my-command --strip-components=1
```

**Benefits**:
- Reuses proven npm packaging
- Standard .tgz format (portable)
- Can share hibernated modules
- Can version hibernated modules

## Hibernation vs. Git

**Hibernation complements Git, doesn't replace it:**

| Feature | Git | Hibernation |
|---------|-----|-------------|
| Purpose | Version history | Runtime state |
| Scope | Entire repo | Individual modules |
| Speed | Slow (commit + push) | Instant (< 2s) |
| Granularity | Commit-level | Module-level |
| Runtime | Not runtime-aware | Runtime-aware (failures, usage) |
| Metadata | Manual commit messages | Automatic failure analysis |

**Best practice**: Use both
- Git for version control and collaboration
- Hibernation for runtime state management and self-healing

## Future: Hibernation as Distribution

Hibernated modules are already packaged and ready to share:

```bash
# Share a hibernated module with colleague
scp /tmp/jtag-hibernation/useful-command.*.tgz colleague@host:/tmp/

# Colleague wakes it directly
./jtag module/wake --from=/tmp/useful-command.*.tgz

# Or publish to shared registry
./jtag module/publish --source=hibernation --name="useful-command"
```

## Benefits Summary

### For Developers
- **Safety**: Never lose work
- **Freedom**: Experiment without fear
- **Reversibility**: Instant undo
- **Simplicity**: One command to hibernate, one to wake

### For System
- **Stability**: Failed modules don't crash system
- **Performance**: Active system stays lean
- **Memory**: Perfect memory of all modules
- **Self-healing**: Automatic failure recovery

### For AI Personas
- **Autonomy**: Ares can hibernate failures without human approval
- **Analysis**: Full metadata for post-mortem analysis
- **Learning**: Study hibernated modules to prevent future failures
- **Restoration**: AI can propose wake candidates based on patterns

## Implementation Status

### ✅ Phase 1: Foundation (DONE)
- Generator creates self-contained modules
- `npm pack` creates .tgz packages
- Manual extraction workflow validated

### 🚧 Phase 2: Hibernation System (NEXT)
- Implement `ModuleHibernationSystem` class
- Add CLI commands: hibernate, wake, list-hibernated
- Metadata generation and storage
- Registry tracking

### 📋 Phase 3: Ares Integration (PLANNED)
- Automatic hibernation on module failure
- Periodic review of hibernated modules
- Wake candidate proposals
- Democratic vote integration for complex cases

### 📋 Phase 4: Archive System (PLANNED)
- Automatic archival of old hibernations (> 30 days)
- Long-term storage management
- Archive statistics and reporting

### 📋 Phase 5: Advanced Features (FUTURE)
- Hibernation history (multiple versions)
- Diff between hibernated versions
- Selective wake (partial module restoration)
- Shared hibernation registry (team distribution)

## Related Documentation

- **[AUDIT-SYSTEM-DESIGN.md](./AUDIT-SYSTEM-DESIGN.md)** - Audit checks for hibernated modules
- **[GENERATOR-ROADMAP.md](./GENERATOR-ROADMAP.md)** - Generator phases and hibernation integration
- **[PERSONA-CONVERGENCE-ROADMAP.md](../system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md)** - Ares self-healing integration
- **[LORA-GENOME-PAGING.md](../system/user/server/modules/LORA-GENOME-PAGING.md)** - LoRA adapters for system administration

## Conclusion

Hibernation transforms module management from **permanent decisions** (delete) to **temporal decisions** (hibernate). Nothing is ever lost, everything is restorable, and the system heals itself while maintaining perfect memory.

**Core insight**: The filesystem is temporal storage. Hibernation makes time reversible.
