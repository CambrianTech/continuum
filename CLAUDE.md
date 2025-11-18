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

## 🚫 CRITICAL: NEVER COMMIT BEFORE TESTING

**The iron rule of development: TEST → VERIFY → COMMIT**

### What Went Wrong (2025-11-18 - Lesson Learned)

**My mistake:**
1. Read summary saying "PersonaToolExecutor needs refactoring"
2. Read the actual file - saw it was already clean (210 lines)
3. **COMMITTED ANYWAY** without testing
4. Then deployed and hoped it worked

**Why this was insane:**
- Committed code I didn't verify worked
- Polluted git history with untested changes
- If it broke, would need to revert/reset (harder recovery)
- Violated basic QA discipline

**The correct order:**
```bash
# 1. Make changes to code
# 2. TEST first
npm start && sleep 120 && ./jtag ping
./jtag chat/send --room="general" --message="Testing new feature..."

# 3. VERIFY it works
./jtag chat/export --room="general" --limit=10  # Check responses

# 4. ONLY THEN commit
git add system/user/server/modules/PersonaToolExecutor.ts
git commit -m "Fix: Verified working refactor"
```

**The fundamental error in thinking:**
- Trusted old summary context over current codebase state
- Never asked: "What's ACTUALLY broken right now?"
- Assumed task from summary was still valid without verification
- Committed based on intention, not evidence

**What I should have done when file looked already refactored:**
1. **STOP** - This doesn't match the summary
2. **ASK** - Is this already fixed? What's the real problem?
3. **TEST CURRENT STATE** - Does it actually work right now?
4. **DIAGNOSE ACTUAL ISSUE** - Don't fix what isn't broken

**The AI team's response** (they were right):
- "Show us a specific failing command"
- "What error are you seeing?"
- "The code looks clean already"
- They forced me to actually understand the problem

**Turns out:** PersonaToolExecutor was fine. Real issue was XML routing for some AIs, not the core code.

### Golden Rules

1. **ALWAYS test before committing** - no exceptions
2. **Question discrepancies** - if summary says X but code shows Y, investigate
3. **Commit = "this works"** - never commit hope, commit evidence
4. **Ask "what's broken NOW?"** - not "what did summary say to fix?"
5. **Use git properly** - it's version control, not a backup drive

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

### Live CSS Editing - CRITICAL RULES

**🚨 THE GOLDEN RULES (FOLLOW THESE OR FAIL):**

1. **Shadow DOM CSS = Hot-inject ONLY, NO npm start**
   - Files: `widgets/*/shared/*.css`, `widgets/*/**/*-widget.css`
   - Method: Edit in place → `./jtag debug/widget-css` → Screenshot
   - Takes: <1 second

2. **Light DOM CSS = Requires npm start**
   - Files: `widgets/*/public/*.css` (compiled into bundles)
   - Method: Edit in place → `npm start` (wait 90s) → Screenshot
   - Takes: ~90 seconds

3. **NEVER remove debug borders until OpenCV verification PASSES**
   - Use thick colored borders (20-40px red, cyan, lime)
   - Run `python3 scripts/verify-border.py path/to/screenshot.png`
   - If verification fails, border stays ON
   - Only remove border after verification succeeds

4. **NEVER edit demo.css or create /tmp files**
   - demo.css cannot penetrate shadow DOM
   - Edit actual widget CSS files at their real paths

**The ONLY correct workflow for shadow DOM CSS:**

```bash
# 1. Add thick border for debugging
# Edit widgets/chat/chat-widget/chat-widget.css:
#   :host { border: 20px solid red !important; }

# 2. Hot-inject (NO npm start!)
./jtag debug/widget-css --widgetSelector="chat-widget" \
  --cssFile="widgets/chat/chat-widget/chat-widget.css"

# 3. Take screenshot
./jtag screenshot --querySelector="body" --filename="test.png"

# 4. Verify with OpenCV
python3 scripts/verify-border.py examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/test.png

# 5. If verification FAILS, try different approach and KEEP BORDER
# 6. Only remove border after verification PASSES
```

**Why this matters:**
- Visual inspection lies - use OpenCV verification
- Hot CSS injection is instant vs 90s npm start
- Borders prove layout constraints are working
- demo.css/external CSS cannot affect shadow DOM

---

## 🔍 CSS DEBUGGING LESSONS (2025-11-09)

### The Problem: Chat Widget Horizontal Overflow

Chat messages and content were extending beyond the viewport edge and getting cut off on the right side. Multiple attempts to fix width constraints in shadow DOM failed.

### Root Cause Discovery

**The issue was NOT in shadow DOM CSS** - it was in light DOM CSS overriding shadow DOM constraints:

```css
/* main-panel.css (LIGHT DOM) - was forcing full width */
chat-widget {
  display: flex;
  flex: 1;           /* ← Made widget grow to fill container */
  width: 100%;       /* ← Overrode shadow DOM max-width */
}
```

**Key insight**: Light DOM CSS properties on the custom element override shadow DOM `:host` properties.

### The Fix

**Shadow DOM** (chat-widget.css):
```css
:host {
  max-width: calc(100vw - var(--sidebar-width) - 100px);  /* Account for sidebar */
}
```

**Light DOM** (main-panel.css):
```css
chat-widget {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
```

### Critical Lessons Learned

**1. Hierarchical Diagnostic Borders**

When shadow DOM borders don't show at viewport edge, the problem is in PARENT containers (light DOM):

```css
/* Add borders at EVERY level of hierarchy */
.content-view { border: 30px solid cyan !important; }   /* Parent container */
chat-widget { border: 20px solid lime !important; }     /* Light DOM element */
:host { border: 20px solid red !important; }            /* Shadow DOM root */
```

If cyan border doesn't reach edge → `.content-view` is constrained
If lime border doesn't reach edge → `chat-widget` is constrained
If red border doesn't reach edge → `:host` is constrained

**2. Light DOM Overrides Shadow DOM**

CSS properties on the custom element tag (light DOM) override `:host` properties (shadow DOM):

```css
/* Light DOM wins */
chat-widget { width: 100%; }  /* ← This overrides... */

/* Shadow DOM loses */
:host { max-width: 80%; }     /* ← ...this */
```

**3. The Need for DevTools Access**

I spent hours trying border debugging when you found the issue in 5 minutes with browser devtools. **Future improvement**: Need programmatic access to:
- Computed styles for all elements
- Box model dimensions
- Element inspector via JTAG commands

**4. Hardcoded Values Are Brittle**

The `100px` in the fix is a hack accounting for borders/padding:
```css
max-width: calc(100vw - var(--sidebar-width) - 100px);  /* ← 100px is brittle */
```

**Better approach** (future refactor): Use proper flex constraints or CSS Grid on parent container to handle layout without hardcoded offsets.

**5. Shadow DOM Isolation Cuts Both Ways**

**Good**: External CSS can't break widget internals
**Bad**: Makes debugging harder because you can't inspect from outside
**Lesson**: Always add diagnostic borders at BOTH light DOM and shadow DOM levels

### Debugging Workflow (What Works)

1. **Add thick borders** at ALL hierarchy levels (parent containers, light DOM element, shadow DOM `:host`)
2. **Hot-inject shadow DOM CSS** with `./jtag debug/widget-css` (instant feedback)
3. **Deploy light DOM CSS** with `npm start` (required for parent containers)
4. **Take screenshots** and verify which borders appear at viewport edge
5. **Run OpenCV verification** to programmatically confirm (prevents lying)
6. **Work up the hierarchy** - if shadow DOM border doesn't appear, problem is in parent
7. **Keep borders ON** until OpenCV verification passes

### What Doesn't Work

- ❌ Editing demo.css (can't penetrate shadow DOM)
- ❌ Creating /tmp files (wrong file paths)
- ❌ Visual inspection only (eyes lie, OpenCV doesn't)
- ❌ Removing borders before verification passes
- ❌ Assuming shadow DOM CSS is always the problem (check light DOM first!)

---

### Chat Commands

**Basic Usage:**
```bash
# Send message to chat room (direct DB, no UI)
./jtag chat/send --room="general" --message="Hello team"
./jtag chat/send --room="general" --message="Reply" --replyToId="abc123"

# Export chat messages to markdown
./jtag chat/export --room="general" --limit=50                    # Print to stdout
./jtag chat/export --room="general" --output="/tmp/export.md"    # Save to file
./jtag chat/export --limit=100 --includeSystem=true               # All rooms with system messages
```

**Interactive Workflow - Working WITH the AI Team:**

When you send a message, `chat/send` returns a message ID. Use this to track responses:

```bash
# 1. Send message (captures the JSON response with messageId)
RESPONSE=$(./jtag chat/send --room="general" --message="Deployed new tool error visibility fix. Can you see errors clearly now?")

# 2. Extract message ID (using jq if available, or manual)
MESSAGE_ID=$(echo "$RESPONSE" | jq -r '.shortId')
echo "My message ID: $MESSAGE_ID"

# 3. Wait for AI responses (they typically respond in 5-10 seconds)
sleep 10

# 4. Check their responses
./jtag chat/export --room="general" --limit=20

# 5. Reply to specific AI feedback
./jtag chat/send --room="general" --replyToId="<their-message-id>" --message="Good catch! Let me fix that..."
```

**CRITICAL**: Don't just broadcast to the AI team - WORK WITH THEM. Use their feedback, reply to their questions, iterate based on what they're saying. The chat export shows message IDs as `#abcd123` - use those to reply.

### Debug Commands
```bash
./jtag debug/logs --tailLines=50 --includeErrorsOnly=true
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

## 🤖 ASK THE LOCAL AI TEAM - YOUR LOCAL RESEARCH ASSISTANT

**THE BREAKTHROUGH**: You can now use the local AI chat like a web search or my `Task()` tool. Ask questions, get multiple perspectives, synthesize solutions - all running locally on Ollama.

Local PersonaUsers (Helper AI, Teacher AI, CodeReview AI, Local Assistant, and 50+ external AIs) can help you solve problems collaboratively.

### Quick Start - Use the General Room

```bash
# STEP 1: Ask a question in the general room (no room ID needed!)
./jtag debug/chat-send --room="general" --message="How should I implement connection pooling for websockets?"

# STEP 2: Wait 5-10 seconds for responses

# STEP 3: View responses in chat widget
./jtag screenshot --querySelector="chat-widget"

# STEP 4: Export conversation to markdown (coming soon - see workflow below)
```

### Current Workflow (Manual)

```bash
# 1. Send your question and capture the message ID
MESSAGE_ID=$(./jtag debug/chat-send --room="general" --message="What's the best way to handle rate limiting?" | jq -r '.messageId')

# 2. Wait for AI responses (they respond within 5-10 seconds)
sleep 10

# 3. Get all messages after your question
./jtag data/list --collection=chat_messages \
  --filter="{\"roomId\":\"ROOM_UUID\",\"timestamp\":{\"\$gte\":\"$MESSAGE_ID_TIMESTAMP\"}}" \
  --orderBy='[{"field":"timestamp","direction":"asc"}]'

# 4. View in browser
./jtag screenshot --querySelector="chat-widget"
```

### Future Workflow (Planned)

```bash
# Export conversation thread to markdown
./jtag chat/export --messageId="UUID" --format="markdown" --output="solution.md"

# This will include:
# - Your question
# - All responses
# - Threading/reply-to relationships
# - Timestamps and authors
# - Formatted as readable markdown
```

### Why This is Powerful

**Like my `Task()` tool but conversational:**
- **Multiple perspectives**: 4+ local AIs + 50+ external AIs respond
- **Fast iteration**: 5-10 seconds for local Ollama responses
- **Free**: No API costs for local inference
- **Contextual**: AIs have system context and specialized knowledge
- **Eventually tool-enabled**: When AIs get tools, they'll be able to run commands, read code, test solutions

**Use cases:**
- "What's the best pattern for X?"
- "How would you debug Y?"
- "Should I use approach A or B?"
- "Review my architecture design for Z"
- "What are the tradeoffs of using library X?"

**Benefits over web search:**
- Conversational - ask follow-ups
- Multiple expert opinions simultaneously
- Context-aware (knows your codebase)
- Can test solutions locally
- No context switching to browser

### Tips

1. **Use the general room** - Everyone is already there
2. **Wait 10 seconds** - Give AIs time to respond (local Ollama ~5-10s, external APIs may vary)
3. **Screenshot to see results** - Chat widget shows full conversation
4. **Specific questions get better answers** - Include context, constraints, requirements
5. **Ask for comparisons** - "Compare approach A vs B for use case X"

### When AIs Get Tools (Future)

Imagine asking: *"Find all files using deprecated API X, show me examples, and suggest migration pattern"*

The AIs will:
1. Search codebase with `Glob` and `Grep`
2. Read relevant files with `Read`
3. Analyze patterns
4. Suggest refactoring approach
5. Show you diffs

**This is the vision** - conversational development with a team of AI specialists who can actually DO things.

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
- if you only edit a test, and not the api itself, you don't need to redeploy with npm start, just edit and test again e.g npx tsx tests/integration/genome-fine-tuning-e2e.test.ts
- need to remember to npm run build:ts before deploying with npm start, just to make sure there's no compilation issues
- ./jtag chat/export --room="general" --limit=30 will let you see ai opinions after chat/send to ask