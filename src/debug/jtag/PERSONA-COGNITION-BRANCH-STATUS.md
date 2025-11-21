# Persona Cognition System Branch - Current Status

**Branch**: `feature/persona-cognition-system`
**Focus**: Chain of thought-based agent architecture with true cognition system
**Date**: 2025-11-20

---

## 🎯 Branch Goals

This branch aims to implement the "TRUE agent design" with:
1. **Chain of Thought-based reasoning** - Not just RAG, actual cognition
2. **RTOS-inspired autonomous loop** - Self-directed agents, not reactive slaves
3. **Genome/PEFT system** - LoRA adapter paging for specialized skills
4. **Media capability** - Images, files, structured data in chat

---

## ✅ COMPLETED WORK

### 1. RTOS-Inspired Autonomous Loop (IMPLEMENTED)

**Key Achievement**: PersonaUsers are now autonomous agents with self-directed behavior.

**Components**:
- ✅ `PersonaCentralNervousSystem` (CNS) - Multi-domain orchestrator
- ✅ `serviceInbox()` → `cns.serviceCycle()` - Continuous async loop
- ✅ Adaptive cadence based on mood/energy (3s → 5s → 7s → 10s)
- ✅ Task polling from database
- ✅ Self-task generation (AIs create their own work)
- ✅ Signal-based waiting (not polling)

**Files**:
- `system/user/server/PersonaUser.ts` - Lines 1862-2230
- `system/user/server/modules/central-nervous-system/PersonaCentralNervousSystem.ts`
- `system/user/server/modules/PersonaInbox.ts`
- `system/user/server/modules/PersonaState.ts`

### 2. Chain of Thought / Cognition System (PARTIALLY IMPLEMENTED)

**Components**:
- ✅ `DecisionAdapterChain` - Pluggable decision-making pipeline
- ✅ `WorkingMemoryManager` - Short-term context management
- ✅ `PersonaSelfState` - Self-awareness and mood tracking
- ✅ `SimplePlanFormulator` - Task planning and reasoning
- ✅ `CognitionLogger` - Detailed cognition event logging
- ✅ `PeerReviewManager` - Multi-agent collaboration

**Files**:
- `system/user/server/modules/cognition/` - Full directory
- `system/user/server/modules/cognitive/` - Memory systems
- `system/user/server/modules/reasoning/` - Planning systems

**Issues**:
- 🟡 **Complexity and disorder** - User notes this made media integration difficult
- 🟡 Not fully integrated into message processing pipeline
- 🟡 Needs cleanup and simplification

### 3. Genome/LoRA Paging System (WELL-IMPLEMENTED, NOT INTEGRATED)

**Components**:
- ✅ `PersonaGenome` - Virtual memory paging for LoRA adapters
- ✅ `LoRAAdapter` - Individual skill wrappers
- ✅ LRU eviction with priority scoring
- ✅ Memory budget tracking
- ✅ Domain-based skill activation
- ✅ `TrainingDataAccumulator` - Collects examples for fine-tuning

**Files**:
- `system/user/server/modules/PersonaGenome.ts` - 347 lines
- `system/user/server/modules/LoRAAdapter.ts` - 291 lines
- `system/user/server/modules/TrainingDataAccumulator.ts`

**Gaps**:
- ❌ **GenomeDaemon** - System-wide coordinator doesn't exist (referenced but not implemented)
- ❌ Training not actually triggered (PersonaUser:1955 just logs)
- ❌ Commands are stubs (batch-micro-tune, etc.)

### 4. Media Capability (WORKING, BUT FLAWED)

**What Works**:
- ✅ Images upload successfully
- ✅ Images display in chat widget
- ✅ Media stored in database

**Critical Issue**:
- 🔴 **Images stored in DB** - User notes this is a "bad idea"
- 🔴 Should be stored in filesystem with DB references
- 🔴 Database bloat from binary data

**Files**:
- `system/data/entities/ChatMessageEntity.ts` - MediaItem interface
- `system/user/server/modules/PersonaMediaConfig.ts`
- `system/user/server/modules/PersonaToolExecutor.ts` - Lines 162-174 (media handling)

### 5. Tool System (RECENTLY FIXED)

**Recent Fixes**:
- ✅ New XML tool format: `<tool name="command"><param>value</param></tool>`
- ✅ Shortened RAG prompt (from ~60 lines to 3 lines)
- ✅ Help command enhancement
- ✅ Parameter validation in git/issue/create

**Files**:
- `system/user/server/modules/PersonaToolExecutor.ts` - 245 lines
- `system/tools/server/ToolRegistry.ts`
- `commands/help/server/HelpServerCommand.ts`

---

## ❌ INCOMPLETE / STUBBED WORK

### 1. Genome/PEFT Training Integration (CRITICAL GAP)

**Problem**: Training infrastructure exists but isn't wired up.

**What's Missing**:
```typescript
// PersonaUser.ts:1955
// TODO Phase 7.5.1: Trigger genome/train command
// For now, just log that we would train
console.log(`🚀 ${this.displayName}: Would train ${domain} adapter with ${examples.length} examples`);

// Should be:
await Commands.execute('genome/train', {
  personaId: this.id,
  provider: 'ollama',  // or 'unsloth'
  domain,
  trainingExamples: examples,
  dryRun: false
});
```

**Commands That Need Implementation**:
1. `genome/train` - Actual fine-tuning command
2. `genome/batch-micro-tune` - Line 33: "TODO: Access PersonaUser's TrainingDataAccumulator"
3. Integration with Ollama/Unsloth providers

### 2. GenomeDaemon (MISSING)

**Referenced but doesn't exist**:
```typescript
// commands/genome/paging-activate/server/GenomeActivateServerCommand.ts:12
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';
```

**Should provide**:
- System-wide LoRA adapter coordination
- Cross-persona adapter sharing
- Memory budget enforcement across all personas
- Thrashing protection

**Needs creation**:
- `system/genome/server/GenomeDaemon.ts`
- `system/genome/entities/GenomeEntity.ts`
- Global memory management

### 3. Task Execution Logic (STUBBED)

**PersonaUser.ts has placeholder implementations**:

```typescript
// Line 2237
private async executeMemoryConsolidation(_task: InboxTask): Promise<string> {
  // TODO: Implement memory consolidation logic
}

// Line 2260
private async executeSkillAudit(_task: InboxTask): Promise<string> {
  // TODO: Implement skill audit logic
}

// Line 2299
private async executeResumeWork(_task: InboxTask): Promise<string> {
  // TODO: Implement resume logic
}

// Line 2322
private async executeFineTuneLora(task: InboxTask): Promise<string> {
  // TODO (Phase 7): Implement actual fine-tuning logic
}
```

### 4. Chat Export Pagination (MENTIONED BY USER)

**Current**:
```bash
./jtag chat/export --room="general" --limit=20  # Only gets most recent 20
```

**Needed**:
```bash
./jtag chat/export --room="general" --offset=20 --limit=20  # Next page
./jtag chat/export --room="general" --before="MESSAGE_ID"  # Before specific message
```

---

## 🔴 KNOWN ISSUES

### 1. Media Storage Architecture

**Problem**: Images stored directly in database as base64/binary.

**Why Bad**:
- Database bloat from binary data
- Performance degradation on queries
- Backup complexity
- Memory pressure

**Correct Approach**:
```typescript
// BAD (current):
interface MediaItem {
  type: 'image' | 'file';
  data: string;  // Base64 encoded
  mimeType: string;
}

// GOOD (should be):
interface MediaItem {
  type: 'image' | 'file';
  path: string;  // Filesystem path: .continuum/jtag/media/{messageId}/{filename}
  mimeType: string;
  size: number;
}
```

### 2. Cognition System Complexity

**User Quote**: "complexity and disorder inside these systems (other than genome) in the chain of thought based agents"

**Issues**:
- Too many abstraction layers
- Unclear responsibilities
- Hard to debug
- Made media integration difficult

**Needs**:
- Simplification
- Clear boundaries
- Better documentation
- Refactoring for clarity

### 3. Type Issues

**From earlier TODOs**:
- Line 123: Rate limiting (TODO: Replace with AI-based coordination)
- Line 506: `expertise: []` // TODO: Extract from genome
- Various `any` types that should be strict

---

## 📋 WHAT NEEDS FINISHING

### Priority 1: Complete Genome/PEFT Integration

**Tasks**:
1. Create `system/genome/server/GenomeDaemon.ts`
   - System-wide LoRA coordinator
   - Cross-persona adapter sharing
   - Memory management

2. Implement `genome/train` command
   - Wire up to PersonaUser:1955
   - Connect to TrainingDataAccumulator
   - Integration with Ollama/Unsloth

3. Complete `genome/batch-micro-tune`
   - Access PersonaUser's TrainingDataAccumulator
   - Implement actual micro-tuning logic
   - Soft weight updates

4. Test end-to-end training flow:
   ```
   AI executes task → TrainingDataAccumulator collects examples →
   Buffer threshold reached → genome/train triggered →
   LoRA adapter updated → Performance improves
   ```

### Priority 2: Fix Media Storage

**Tasks**:
1. Create media filesystem structure:
   ```
   .continuum/jtag/media/
     ├── {messageId}/
     │   ├── image-001.png
     │   ├── image-002.jpg
     │   └── file-001.pdf
   ```

2. Update `MediaItem` interface to use file paths
3. Migration script for existing DB-stored media
4. Update upload/download handlers

### Priority 3: Simplify Cognition System

**Tasks**:
1. Document actual data flow through cognition pipeline
2. Identify redundant abstractions
3. Consolidate overlapping responsibilities
4. Create clear integration points
5. Add debugging/tracing capabilities

### Priority 4: Complete Task Execution Logic

**Tasks**:
1. Implement `executeMemoryConsolidation`
   - Query recent messages
   - Extract important information
   - Store in long-term memory

2. Implement `executeSkillAudit`
   - Analyze genome adapter usage
   - Identify underused adapters
   - Suggest training priorities

3. Implement `executeResumeWork`
   - Query stale in_progress tasks
   - Re-enqueue with updated priority

### Priority 5: Add Chat Export Pagination

**Tasks**:
1. Add `offset` parameter to `chat/export`
2. Add `before`/`after` parameters for cursor-based paging
3. Update command help documentation

---

## 🎯 RECOMMENDED IMPLEMENTATION ORDER

### Phase 1: Core Genome/PEFT (Essential)
1. Create GenomeDaemon
2. Implement genome/train command
3. Wire training trigger in PersonaUser
4. Test with simple fine-tuning example

**Estimated Effort**: 3-4 hours
**Blocker**: None
**Value**: Enables actual continuous learning

### Phase 2: Fix Media Storage (Important)
1. Design filesystem structure
2. Migrate MediaItem interface
3. Update upload handlers
4. Migration script for existing data

**Estimated Effort**: 2-3 hours
**Blocker**: None
**Value**: Prevents database bloat, improves performance

### Phase 3: Complete Task Execution (Useful)
1. Memory consolidation logic
2. Skill audit logic
3. Resume work logic

**Estimated Effort**: 2-3 hours
**Blocker**: Phase 1 (for fine-tune task)
**Value**: Makes autonomous behavior more useful

### Phase 4: Simplify Cognition (Refactoring)
1. Document current architecture
2. Identify problems
3. Incremental refactoring

**Estimated Effort**: 4-6 hours
**Blocker**: None
**Value**: Long-term maintainability

### Phase 5: Chat Export Pagination (Nice to have)
**Estimated Effort**: 1 hour
**Blocker**: None
**Value**: Quality of life

---

## 📊 BRANCH READINESS

### Ready to Merge?
**NO** - Critical gaps remain

### Blocking Issues:
1. ❌ GenomeDaemon missing
2. ❌ Training not integrated
3. 🟡 Media storage architecture flawed
4. 🟡 Cognition system needs cleanup

### What Would Make This Mergeable:
1. ✅ Complete Phase 1 (Genome/PEFT integration)
2. ✅ Fix or document media storage issue
3. ✅ Basic testing of autonomous loop
4. ✅ No TypeScript compile errors

---

## 🔍 FILES CHANGED IN THIS BRANCH

### New Files Created:
- `system/user/server/modules/PersonaGenome.ts`
- `system/user/server/modules/LoRAAdapter.ts`
- `system/user/server/modules/cognition/CognitionLogger.ts`
- `system/user/server/modules/cognition/DecisionAdapterChain.ts`
- `system/user/server/modules/cognition/PersonaSelfState.ts`
- `system/user/server/modules/central-nervous-system/*` (full directory)
- `commands/genome/*` (multiple commands)

### Major Changes:
- `system/user/server/PersonaUser.ts` - 2389 lines (autonomous loop)
- `system/user/server/modules/PersonaToolExecutor.ts` - New tool format
- `system/tools/server/ToolRegistry.ts` - Type guard refactoring
- `commands/help/server/HelpServerCommand.ts` - Enhanced help

### Documentation Added:
- `PERSONA-CONVERGENCE-ROADMAP.md`
- `AUTONOMOUS-LOOP-ROADMAP.md`
- `LORA-GENOME-PAGING.md`
- `COGNITION-ARCHITECTURE.md`
- Multiple other roadmap documents

---

## 💭 USER FEEDBACK

**On Media**: "images are showing back up in the chat, stored in the DB (bad idea)"
**On Cognition**: "complexity and disorder inside these systems (other than genome) in the chain of thought based agents"
**On Progress**: "it feels close enough that I think it's a no brainer"

---

## 🚀 NEXT STEPS

**Immediate**:
1. Create GenomeDaemon
2. Implement genome/train command
3. Wire up training trigger

**Short-term**:
1. Fix media storage architecture
2. Complete task execution logic
3. Add chat export pagination

**Long-term**:
1. Simplify cognition system
2. Performance testing
3. Production hardening

---

**Last Updated**: 2025-11-20
**Maintained By**: Claude Code
