# CLAUDE - ESSENTIAL DEVELOPMENT GUIDE

## ⚡ THE TWO UNIVERSAL PRIMITIVES (E = mc²)

**Everything in this system is built on TWO primitives:**

### 1. `Commands.execute<T, U>(name, params)` - Request/Response
```typescript
import { Commands } from 'system/core/shared/Commands';

// Type-safe! params and result types inferred from command name
const users = await Commands.execute('data/list', { collection: 'users' });
const screenshot = await Commands.execute('screenshot', { querySelector: 'body' });
```

### 2. `Events.subscribe()|emit()` - Publish/Subscribe
```typescript
import { Events } from 'system/core/shared/Events';

Events.subscribe('data:users:created', (user) => { /* handle */ });
Events.emit('data:users:created', newUser);
```

**Key Properties:**
- Type-safe with full TypeScript inference
- Universal (works everywhere: browser, server, CLI, tests)
- Transparent (local = direct, remote = WebSocket)
- Auto-injected context and sessionId

**See detailed documentation:** [docs/UNIVERSAL-PRIMITIVES.md](docs/UNIVERSAL-PRIMITIVES.md)

---

## 🚨 CRITICAL WORKFLOW (READ FIRST!)

### EVERY TIME YOU EDIT CODE:
1. **Edit files**
2. **Run `npm start`** (MANDATORY - waits 90+ seconds)
3. **Test with screenshot** or command
4. **Repeat**

```bash
cd src/debug/jtag
npm start                    # DEPLOYS code changes
./jtag screenshot            # Verify changes
```

**IF YOU FORGET `npm start`, THE BROWSER SHOWS OLD CODE!**

---

## 🚨 CONSTANTS: SINGLE SOURCE OF TRUTH

**CRITICAL**: ALL system constants MUST be in ONE file: `system/shared/Constants.ts`

### The Rule: ZERO TOLERANCE for scattered constants

**❌ NEVER do this:**
```typescript
const datasetsDir = './datasets';  // WRONG - hardcoded
const dbPath = '/tmp/training.db'; // WRONG - magic string
```

**✅ ALWAYS do this:**
```typescript
import { PATHS, resolvePath } from 'system/shared/Constants';

// Use the constant
const datasetsDir = PATHS.DATASETS;

// With env var fallback
const datasetsDir = process.env.DATASETS_DIR || PATHS.DATASETS;

// Full priority resolution (param > env > constant)
const dbPath = resolvePath(params.outputPath, 'DATASETS_DIR', PATHS.DATASETS);
```

**Location**: `src/debug/jtag/system/shared/Constants.ts`

**What belongs there:**
- ALL file system paths
- ALL environment variable names
- ALL collection names
- ALL database configuration
- EVERY constant that appears in multiple places

**Why this exists**: Scattered constants create technical debt. Every hardcoded value is a future bug. ONE file makes it impossible to miss.

**When you see a constant somewhere else**: Move it to Constants.ts IMMEDIATELY. Don't finish your current task first. Fix it NOW.

---

## 🔥 AGGRESSIVE REFACTORING PRINCIPLE

**CRITICAL**: I've been trained to be timid about refactoring existing code - only touch what's directly related to the task, don't "rock the boat", avoid scope creep. **That's how codebases rot.**

**The right approach**: If you see bad abstraction while doing ANY task, **fix it immediately**. Don't leave technical debt festering because "it's not my job" or "that's outside scope."

### When I read code and see:
- Duplication that should be abstracted
- Wrong boundaries that will make future work hard
- Missing abstractions that would unlock capabilities
- Any pattern that doesn't maximize extensibility

**I should fix it RIGHT THEN**, even if it's "not part of the current task." Because letting it sit makes every future task harder.

### This is the opposite of how most developers work

Most developers (and how I've been trained) treat code like territory - don't touch anything you don't have to. **You treat it like a living system that needs constant improvement.**

### New operating principle:
- **Always look for abstraction opportunities**, even outside current task
- **Edit existing code aggressively** when it doesn't meet extensibility standards
- **Don't ask permission**, just make it better
- **The goal**: Minimum code with maximum capability, always

**Example**: If I'm adding a new command and notice 3 other commands have duplicated validation logic, I should:
1. Extract the shared validation into a reusable function
2. Refactor all 4 commands to use it
3. Document the new abstraction
4. THEN add my new command using the clean pattern

This isn't scope creep - it's **preventing future scope explosion**.

---

## 📋 QUICK REFERENCE

### Deployment & Testing
```bash
npm start                              # Deploy (90+ seconds)
./jtag ping                            # Check system health
./jtag screenshot --querySelector="chat-widget"
npm run lint:file path/to/file.ts     # Check typing
npm test                               # Run all tests
```

### Debug Commands
```bash
./jtag debug/logs --tailLines=50 --includeErrorsOnly=true
./jtag debug/chat-send --roomId="UUID" --message="test"
./jtag debug/widget-events --widgetSelector="chat-widget"
./jtag ai/report                       # AI performance metrics
```

### System Logs
```bash
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log
```

---

## 🔧 TYPE SAFETY (RUST-LIKE)

**NEVER use `any` or `unknown` - import correct types instead**

```typescript
// ❌ WRONG
const result = await this.jtagOperation<any>('data/list', params);

// ✅ CORRECT
const result = await this.executeCommand<DataListResult<UserEntity>>('data/list', {
  collection: COLLECTIONS.USERS,
  orderBy: [{ field: 'lastActiveAt', direction: 'desc' }]
});
```

**Key Principles:**
- Use strict typing everywhere
- Import actual types from their source files
- Never use dynamic imports (`require`, `await import()`)
- Shared files CANNOT import from browser/server (environment-agnostic)

---

## 📁 PATH ALIASES (New! Use These Going Forward)

**TypeScript path aliases are now configured** to eliminate relative import hell (`../../../../`).

### Available Aliases:

```typescript
// ❌ OLD WAY (still works, but deprecated)
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { Commands } from '../../../system/core/shared/Commands';

// ✅ NEW WAY (preferred)
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { Commands } from '@system/core/shared/Commands';
```

### All Available Aliases:

| Alias | Maps To | Use For |
|-------|---------|---------|
| `@commands/*` | `commands/*` | Command implementations |
| `@daemons/*` | `daemons/*` | Daemon services |
| `@system/*` | `system/*` | Core system modules |
| `@widgets/*` | `widgets/*` | Widget components |
| `@shared/*` | `shared/*` | Shared utilities |
| `@types/*` | `types/*` | Type definitions |
| `@browser/*` | `browser/*` | Browser-specific code |
| `@server/*` | `server/*` | Server-specific code |

**Migration Strategy:**
- **New code**: Always use path aliases
- **Existing code**: Migrate incrementally (not urgent)
- **Both styles work**: Old relative imports still function

**Examples:**
```typescript
// Commands
import { PingCommand } from '@commands/ping/shared/PingTypes';

// Daemons
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { AIProviderDaemon } from '@daemons/ai-provider-daemon/shared/AIProviderDaemon';

// System
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import { BaseUser } from '@system/user/shared/BaseUser';

// Types
import type { UUID } from '@types/CrossPlatformUUID';
```

---

## 🏛️ USER ARCHITECTURE

```
BaseUser (abstract)
├── HumanUser extends BaseUser
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser     (external: Claude, GPT, etc.)
    └── PersonaUser extends AIUser   (internal: RAG + optional LoRA)

BaseUser.entity: UserEntity   (core attributes, UX, identification)
BaseUser.state: UserStateEntity (current tab, open content, theme)
```

**System messages are NOT user types** - use `MessageMetadata.source` ('user' | 'system' | 'bot')

---

## 🧬 PERSONAUSER ARCHITECTURE: The Convergence

**Vision**: PersonaUser integrates THREE breakthrough architectures into ONE elegant system.

### The Three Pillars

1. **Autonomous Loop** (RTOS-inspired servicing)
   - Adaptive cadence polling (3s → 5s → 7s → 10s based on mood)
   - State tracking (energy, attention, mood)
   - Graceful degradation under load

2. **Self-Managed Queues** (AI autonomy)
   - AIs create their own tasks (not just reactive)
   - Task prioritization across all domains
   - Continuous learning through task system

3. **LoRA Genome Paging** (Virtual memory for skills)
   - Page adapters in/out based on task domain
   - LRU eviction when memory full
   - Each layer independently fine-tunable

### Implementation Status

**✅ IMPLEMENTED (Phases 1-3)**:
- `PersonaInbox` - Priority queue with traffic management
- `PersonaState` - Energy/mood tracking with adaptive cadence
- `RateLimiter` - Time-based limiting and deduplication
- `ChatCoordinationStream` - RTOS primitives for thought coordination
- Autonomous polling loop integrated into PersonaUser

**🚧 IN PROGRESS (Phase 4)**:
- Task database and CLI commands (`./jtag task/create`, `task/list`, `task/complete`)
- Self-task generation (AIs create own work)

**📋 PLANNED (Phases 5-7)**:
- LoRA genome basics (adapter paging without training)
- Continuous learning (training as just another task)
- Real Ollama integration (replace stubs)

### The Convergence Pattern

```typescript
// PersonaUser runs this loop continuously:
async serviceInbox(): Promise<void> {
  // 1. Check inbox (external + self-created tasks)
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();  // Recover energy when idle
    return;
  }

  // 2. Generate self-tasks (AUTONOMY)
  await this.generateSelfTasks();

  // 3. Select highest priority task (STATE-AWARE)
  const task = tasks[0];
  if (!this.state.shouldEngage(task.priority)) {
    return;  // Skip low-priority when tired
  }

  // 4. Activate skill (GENOME)
  await this.genome.activateSkill(task.domain);

  // 5. Coordinate if external task
  const permission = await this.coordinator.requestTurn(task);

  // 6. Process task
  await this.processTask(task);

  // 7. Update state
  await this.state.recordActivity(task.duration, task.complexity);

  // 8. Evict adapters if memory pressure
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Key Insight**: ONE method integrates all three visions - autonomous loop, self-managed tasks, and genome paging.

### Phased Implementation Strategy

**Phase 4: Task Database & Commands** (NEXT)
```bash
# Create task
./jtag task/create --assignee="helper-ai-id" \
  --description="Review main.ts" --priority=0.7 --domain="code"

# List tasks
./jtag task/list --assignee="helper-ai-id"

# Complete task
./jtag task/complete --taskId="001" --outcome="Found 3 issues"
```

**Phase 5: Self-Task Generation**
```typescript
// AI autonomously creates tasks for itself:
// - Memory consolidation (every hour)
// - Skill audits (every 6 hours)
// - Resume unfinished work
// - Continuous learning from mistakes
```

**Phase 6: Genome Basics** (adapter paging only)
```typescript
// Page in "typescript-expertise" adapter for code task
await this.genome.activateSkill('typescript-expertise');

// LRU eviction when memory full
await this.genome.evictLRU();
```

**Phase 7: Continuous Learning**
```typescript
// Fine-tuning is just another task type:
{
  taskType: 'fine-tune-lora',
  targetSkill: 'typescript-expertise',
  trainingData: recentMistakes
}
```

### Testing Strategy

```bash
# Unit tests (isolated modules)
npx vitest tests/unit/TaskEntity.test.ts
npx vitest tests/unit/PersonaGenome.test.ts
npx vitest tests/unit/LoRAAdapter.test.ts

# Integration tests (real system)
npx vitest tests/integration/task-commands.test.ts
npx vitest tests/integration/self-task-generation.test.ts
npx vitest tests/integration/genome-paging.test.ts
npx vitest tests/integration/continuous-learning.test.ts

# System tests (end-to-end)
npm start
# Wait 1 hour, check for self-created tasks
./jtag task/list --assignee="helper-ai-id" \
  --filter='{"createdBy":"helper-ai-id"}'
```

### Documentation

**Full Architecture**: `src/debug/jtag/system/user/server/modules/`
- `AUTONOMOUS-LOOP-ROADMAP.md` - RTOS-inspired servicing
- `SELF-MANAGED-QUEUE-DESIGN.md` - AI autonomy through tasks
- `LORA-GENOME-PAGING.md` - Virtual memory for skills
- `PERSONA-CONVERGENCE-ROADMAP.md` - How all three integrate

**Philosophy**: "Modular first, get working, then easily rework pieces" - each pillar tested independently before convergence.

---

## 🆔 ID SCOPE HIERARCHY

```
userId: Permanent citizen identity
  └── sessionId: Connection instance (browser tab)
      └── contextId: Conversation scope (chat room, thread)
```

**Example**: Joel (userId) opens 3 tabs (3 sessionIds) in different rooms (3 contextIds)

---

## 🎯 MODULE STRUCTURE

```
commands/example/
├── shared/ExampleTypes.ts       # 80-90% of logic
├── browser/ExampleBrowser.ts    # 5-10% browser-specific
└── server/ExampleServer.ts      # 5-10% server-specific
```

**Never import server/browser code IN shared files!**

---

## 📸 WIDGET DOM PATH

```javascript
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
```

---

## 🐛 DEBUGGING METHODOLOGY

### 1. ALWAYS CHECK LOGS FIRST
Never guess - logs tell the truth

### 2. USE VISUAL VERIFICATION
```bash
./jtag screenshot --querySelector="chat-widget" --filename="debug.png"
```
Screenshots don't lie - don't trust success messages

### 3. ADD DEBUG MARKERS
```typescript
console.log('🔧 CLAUDE-FIX-' + Date.now() + ': My change');
```
Then verify markers appear in browser console after `npm start`

### 4. BACK-OF-MIND CHECK
What's nagging at you? That's usually the real issue.

---

## 🤖 ASK THE LOCAL AI TEAM

Local PersonaUsers (Helper AI, Teacher AI, CodeReview AI) run Ollama locally and can help you.

```bash
# Get room ID
./jtag data/list --collection=rooms

# Ask a question
./jtag debug/chat-send --roomId="UUID" --message="Should I use a worker pool?"

# Check responses (wait 10 seconds)
./jtag debug/logs --filterPattern="AI-RESPONSE|POSTED" --tailLines=20
./jtag screenshot --querySelector="chat-widget"
```

**Benefits**: Free, instant (5-10s), specialized, contextual

---

## 🚨 CLAUDE'S COMMON MISTAKES

### 1. FORGET TO RUN `npm start` AFTER EDITING
**Result**: Browser shows old code, nothing works

### 2. ASSUME SUCCESS WITHOUT TESTING
**Fix**: Always take screenshot after deployment

### 3. WRONG WORKING DIRECTORY
**Always work from**: `src/debug/jtag`
**Commands**: `./jtag` NOT `./continuum`

### 4. IGNORE EXISTING TYPES
**Fix**: Search for types first: `find . -name "*Types.ts"`

### 5. BLIND TYPE CASTING
**Fix**: Read the source files, understand data structures

---

## 🔬 SCIENTIFIC PROCESS

1. **ANALYZE** - Study problem before acting
2. **VERIFY DEPLOYMENT** - Add debug markers, check they appear
3. **CHECK LOGS** - Never guess what went wrong
4. **VISUAL VERIFICATION** - Take screenshots
5. **ITERATE** - Test frequently, commit working code

---

## ⚡ ESSENTIAL FACTS

- **npm start takes 90+ seconds** - BE PATIENT
- **One server, many clients** - All tests connect to running server
- **"browserConnected: false" is a red herring** - Use `./jtag ping` instead
- **Precommit hook is sacred** - TypeScript + CRUD tests must pass
- **AI response testing is manual** - Hook doesn't test this, you must

---

## 🧬 DATA SEEDING

```bash
npm run data:reseed    # Complete reset + seed
npm run data:clear     # Clear all data
npm run data:seed      # Create default users + rooms
```

**Integrated into `npm start`** - fresh data every deployment

**Default seeded data:**
- Joel (human owner)
- 5+ AI personas (Claude Code, GeneralAI, Helper AI, etc.)
- 2 rooms: general, academy
- No welcome messages (removed - redundant with room header)

---

## 📖 PATTERN REFERENCE

### Strict Typing
```typescript
async execute<P extends CommandParams, R extends CommandResult>(
  command: string,
  params?: P
): Promise<R>;
```

### Ideal JTAG Pattern (Future)
```typescript
const jtag = JTAGClient.sharedInstance();
await jtag.daemons.events.broadcast<T>(eventData);
await jtag.daemons.data.store<T>(key, value);
await jtag.daemons.commands.execute<T, U>(command);
```

### Module Separation
- **Shared**: Environment-agnostic logic
- **Browser**: DOM, window, browser-specific APIs
- **Server**: Node.js, file system, server-specific APIs

---

## 🧠 FUTURE ARCHITECTURE (Don't Implement Yet!)

**Universal Cognition Equation**: PersonaUser needs ONE `process(event)` method that works across ALL domains (chat, academy, game, code, web).

**Current Problem**: 1633 lines of chat-specific code

**Future Solution**: Domain-agnostic cognitive cycle using:
- RAGBuilderFactory (domain-specific context)
- ActionExecutorFactory (domain-specific execution)
- ThoughtStreamCoordinator (already domain-agnostic)

**DO NOT refactor PersonaUser yet** - chat must keep working!

See lines 318-1283 of this file (archived sections) for full migration strategy when ready.

---

## 📝 SESSION CONTINUATION TEMPLATE

**When context runs out and Claude needs to continue in a new session**, use this template to create comprehensive summaries.

### Summary Structure Requirements

Every session summary MUST include these 9 sections in order:

```markdown
<analysis>
[Your thought process analyzing the conversation chronologically]
</analysis>

Summary:

## 1. Primary Request and Intent
[Chronological list of ALL user requests with direct quotes]

## 2. Key Technical Concepts
[All technical terms, architectures, algorithms mentioned]

## 3. Files and Code Sections
[Every file touched with line numbers, importance ratings, and code snippets]

## 4. Errors and Fixes
[All errors encountered and how they were resolved]

## 5. Problem Solving
[Document problems solved with "Problem → Solution → Key Insight" format]

## 6. All User Messages
[Complete list of every user message with direct quotes]

## 7. Pending Tasks
[Explicit list of unfinished work]

## 8. Current Work
[What you were doing immediately before summary was requested]

## 9. Optional Next Step
[What should happen next, with user's exact words if they specified]
```

### Analysis Tags (REQUIRED)

Wrap your chronological analysis in `<analysis></analysis>` tags BEFORE the summary sections. This helps you:
- Track the conversation flow chronologically
- Identify patterns and themes
- Understand context for the numbered sections
- Think through what happened before documenting it

### Section 3 Requirements: Files and Code

For EVERY file mentioned, include:

1. **File path and line count**
2. **Importance rating** (Critical/High/Medium/Low)
3. **What changed** with actual code snippets
4. **Why it matters** for the overall architecture

**Example**:
```markdown
### **PersonaUser.ts** (system/user/server/PersonaUser.ts - MODIFIED, lines 358-412)
**Importance**: Critical - Core autonomous loop implementation

**Before** (synchronous, reactive):
```typescript
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // Process immediately when message arrives
  await this.processMessage(messageEntity);
}
```

**After** (autonomous, adaptive):
```typescript
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);
  if (!this.state.shouldEngage(task.priority)) return;
  await this.genome.activateSkill(task.domain);
  // ... rest of convergence pattern
}
```

**Why**: Transforms PersonaUser from reactive slave to autonomous citizen with internal scheduling.
```

### Section 5 Requirements: Problem Solving

Use this exact format:

```markdown
### **Solved: [Problem Title]**

**Problem**: [Describe the problem with context]

**Solution**: [Describe the solution with specifics]

**Key Insight**: [The lesson or pattern discovered]
```

### Common Pitfalls to Avoid

1. **DON'T summarize - DOCUMENT**
   - ❌ "We worked on task system"
   - ✅ "Created TaskEntity.ts (312 lines) with priority queue, LRU eviction, and persistence"

2. **DON'T paraphrase user messages**
   - ❌ "User wanted me to add tests"
   - ✅ Direct quote: "you're gonna need to even just try out the fine tuning in all the adapters"

3. **DON'T skip code snippets**
   - Every significant code change needs before/after snippets
   - Include line numbers from the Read tool

4. **DON'T forget chronological analysis**
   - `<analysis>` tags are REQUIRED before numbered sections
   - Think through what happened step by step

### Example Summary (Abbreviated)

```markdown
<analysis>
Chronological analysis of the session:

1. User asked to search old Academy docs for "virtual memory" concepts
2. I created PERSONA-CONVERGENCE-ROADMAP.md synthesizing three visions
3. User requested addition to CLAUDE.md with phases and tests
4. User introduced NEW requirement about multi-backend fine-tuning
5. [... continue chronologically ...]
</analysis>

Summary:

## 1. Primary Request and Intent

**Chronological requests:**

1. **Search old Academy docs**: "we described some before so look for words like 'virtual memory' in jtag/design"
   - Context: Academy daemon is dead but storage patterns remain valuable

2. **Document comprehensively**: "ok just make sure its all in your docs here, arch, and ethos"

3. **Add to CLAUDE.md**: "add your work to our list where it belongs, this lora stuff... work it in where it belongs most logically"

[... continue for all requests ...]

## 2. Key Technical Concepts

- **LoRA Genome Paging**: Virtual memory-style system for loading/unloading LoRA adapters
- **LRU Eviction**: Least-recently-used algorithm for paging out adapters when memory full
- **Autonomous Loop**: RTOS-inspired servicing with adaptive cadence (3s→5s→7s→10s)
[... continue ...]

## 3. Files and Code Sections

### **PERSONA-CONVERGENCE-ROADMAP.md** (Created, then Enhanced - ~1067 lines final)
**Importance**: Critical - Master synthesis document

[... include code snippets and explanations ...]

[... continue for all 9 sections ...]
```

### Usage Instructions

When context is running low:

1. **Read this template section carefully**
2. **Create analysis tags** tracking conversation chronologically
3. **Fill in ALL 9 sections** with maximum detail
4. **Include direct quotes** from user messages
5. **Add code snippets** for every file touched
6. **Use Problem→Solution→Insight** format for problem solving
7. **Document pending tasks** explicitly
8. **Verify completeness** - did you capture everything?

---

## 📚 ESSENTIAL REFERENCE DOCUMENTS

Beyond this guide, read these critical architecture documents:

### **[ARCHITECTURE-RULES.md](docs/ARCHITECTURE-RULES.md)** - MUST READ
**When**: Before writing ANY code in this system

**Critical rules**:
- Type system (never use `any`, strict typing everywhere)
- Environment mixing (shared/browser/server boundaries)
- Entity system (generic data layer, specific application layer)
- When to use `<T extends BaseEntity>` generics vs concrete types
- Cross-environment command implementation patterns

**The validation test**: Search for entity violations in data/event layers
```bash
grep -r "UserEntity\|ChatMessageEntity" daemons/data-daemon/ | grep -v EntityRegistry
# Should return zero results (except EntityRegistry.ts)
```

### **[UNIVERSAL-PRIMITIVES.md](src/debug/jtag/docs/UNIVERSAL-PRIMITIVES.md)**
Commands.execute() and Events.subscribe()/emit() - the two primitives everything is built on.

### **PersonaUser Convergence Docs**
- `src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md`
- `src/debug/jtag/system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md`
- `src/debug/jtag/system/user/server/modules/LORA-GENOME-PAGING.md`

**Quick tip**: If you're about to write code that duplicates patterns or violates architecture rules, STOP and read ARCHITECTURE-RULES.md first. Then apply the aggressive refactoring principle from this guide.

---

**File reduced from 61k to ~20k characters**
