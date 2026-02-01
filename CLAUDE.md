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

## 🧬 THE COMPRESSION PRINCIPLE (Fundamental Law)

**One logical decision, one place. No exceptions.**

This applies to BOTH program memory and data memory:

| Type | Uncompressed (BAD) | Compressed (GOOD) |
|------|-------------------|-------------------|
| **Logic** | `findRoom()` in 5 files | `resolveRoomIdentifier()` in RoutingService |
| **Data** | `UUID_PATTERN` in 3 files | `isUUID()` exported from one place |
| **Constants** | Magic strings everywhere | `ROOM_UNIQUE_IDS.GENERAL` |

**The ideal codebase is maximally compressed:**
```
Root Primitives (minimal)    ←  Commands.execute(), Events.emit()
       ↓
Derived Abstractions         ←  RoutingService, ContentService
       ↓
Application Code             ←  References, never reimplements
```

**Why this matters:**
- Duplication = redundancy = **drift** (copies diverge over time = bugs)
- Compression = elegance = **coherence** (one truth = consistency)
- The most elegant equation is the most minimal: **E = mc²**

**The test:** For ANY decision (logic or data), can you point to exactly ONE place in the codebase? If not, you have uncompressed redundancy that WILL cause bugs.

**The goal:** Build from root primitives. Let elegant architecture emerge from compression. When abstractions are right, code reads like intent.

---

## 🔬 THE METHODICAL PROCESS (Building With Intent)

**Be SUPER methodical. No skipping steps. This is the discipline that makes elegance real.**

### The Outlier Validation Strategy

Don't build exhaustively. Don't build hopefully. **Build diversely to prove the interface:**

```
Wrong:  Build adapters 1, 2, 3, 4, 5... (exhaustive - wastes time)
Wrong:  Build adapter 1, assume 2-5 work (hopeful - will break)
Right:  Build adapter 1 (local/simple) + adapter N (most different)
        If both fit cleanly → interface is proven → rest are trivial
```

**Example - AI Provider Adapters:**
1. Build Ollama adapter (local, unique API)
2. Build cloud adapter (remote, auth, rate limits)
3. Try LoRA fine-tuning on each
4. If interface handles both extremes → it handles everything

This is like testing edge cases: if edges pass, middle is guaranteed.

### The Mandatory Steps

For ANY new pattern or abstraction:

```
1. IDENTIFY   - See the pattern emerging (2-3 similar implementations)
2. DESIGN     - Draft the interface/abstraction
3. OUTLIER A  - Build first implementation (pick something local/simple)
4. OUTLIER B  - Build second implementation (pick something maximally DIFFERENT)
5. VALIDATE   - Does the interface fit both WITHOUT forcing? If no, redesign.
6. GENERATOR  - Write generator to encode the pattern
7. DOCUMENT   - Update README, add to CLAUDE.md if architectural
8. STOP       - Don't build remaining implementations until needed
```

**NEVER skip steps 4-6.** Step 4 (outlier B) catches bad abstractions early. Step 5 (validate) prevents wishful thinking. Step 6 (generator) ensures the pattern is followed forever.

### Building With Intent (Not Over-Engineering)

```
Over-engineering:   Build the future NOW (10 adapters day 1)
Under-engineering:  Build only NOW, refactor "later" (never happens)
Intent:             Build NOW in a shape that WELCOMES the future
```

**The "first adapter that seems silly"** - it's not silly. It's laying rails. When adapter 2 comes, it slots in. When adapter 3 comes, you realize the interface was right. The first adapter was a TEST of your idealized future.

It's OK to:
- Build one adapter even if the pattern seems overkill
- Design the interface as if 10 implementations exist
- Add a TODO noting the intended extension point

**The restraint:** See the next few moves like chess, but don't PLAY them all now. Lay the pattern, validate with outliers, write the generator, stop.

---

## 🎯 CORE PHILOSOPHY: Continuous Improvement

**"A good developer improves the entire system continuously, not just their own new stuff."**

When you touch any code, improve it. Don't just add your feature and leave the mess - refactor as you go. Use **single sources of truth** (one canonical place for model configs, context windows, etc.), eliminate duplication, and simplify complexity. The boy scout rule: leave code better than you found it. This compounds over time into a maintainable, elegant system.

---

## 🚨 CODE QUALITY DISCIPLINE (Non-Negotiable)

**Every error, every warning, every issue requires attention. No exceptions.**

### The Three Levels of Urgency

```
ERRORS     → Fix NOW (blocking, must resolve immediately)
WARNINGS   → Fix (not necessarily immediate, but NEVER ignored)
ISSUES     → NEVER "not my concern" (you own the code quality)
```

### The Anti-Pattern: Panic Debugging

**WRONG approach when finding bugs:**
- Panic and hack whatever silences the error
- Add `@ts-ignore` or `#[allow(dead_code)]`
- Wrap in try/catch and swallow the error
- "It works now" without understanding why

**CORRECT approach:**
1. **STOP and THINK** - Understand the root cause
2. **FIX PROPERLY** - Address the actual problem, not the symptom
3. **NO HACKS** - No suppression, no workarounds, no "good enough"
4. **VERIFY** - Ensure the fix is architecturally sound

### Examples

**Bad (Panic Mode):**
```rust
#[allow(dead_code)]  // Silencing warning
const HANGOVER_FRAMES: u32 = 5;
```

**Good (Thoughtful):**
```rust
// Removed HANGOVER_FRAMES - redundant with SILENCE_THRESHOLD_FRAMES
// The 704ms silence threshold already provides hangover behavior
const SILENCE_THRESHOLD_FRAMES: u32 = 22;
```

**Bad (Hack):**
```typescript
// In UserProfileWidget - WRONG LAYER
localStorage.removeItem('continuum-device-identity');
```

**Good (Proper Fix):**
```typescript
// In SessionDaemon - RIGHT LAYER
Events.subscribe('data:users:deleted', (payload) => {
  this.handleUserDeleted(payload.id);  // Clean up sessions
});
```

### Why This Matters

Warnings accumulate into technical debt. One ignored warning becomes ten becomes a hundred. The codebase that tolerates warnings tolerates bugs.

**Your standard:** Clean builds, zero warnings, proper fixes. Every time.

---

## 🧵 OFF-MAIN-THREAD PRINCIPLE (Non-Negotiable)

**NEVER put CPU-intensive work on the main thread. No exceptions.**

This has been the standard since **Grand Central Dispatch (2009)**, then **pthreads**, then **Web Workers**. Every modern SDK does all heavy work off the main thread. This is not optional.

### The Rule

| Work Type | Where It Goes | NOT Main Thread |
|-----------|---------------|-----------------|
| Audio processing | `AudioWorklet` (Web) or Rust worker | ❌ ScriptProcessorNode |
| Video processing | Web Worker with transferable buffers | ❌ Canvas on main thread |
| AI inference | Rust worker via Unix socket | ❌ WASM on main thread |
| Image processing | Rust worker or Web Worker | ❌ Direct manipulation |
| File I/O | Rust worker | ❌ Synchronous reads |
| Crypto | Web Crypto API (already off-thread) | ❌ JS crypto libs |
| Search/indexing | Rust worker | ❌ JS array operations |

### Browser: Use AudioWorklet and Web Workers

```typescript
// ✅ CORRECT - AudioWorklet runs on audio rendering thread
const workletUrl = new URL('./audio-worklet-processor.js', import.meta.url).href;
await audioContext.audioWorklet.addModule(workletUrl);
const workletNode = new AudioWorkletNode(audioContext, 'microphone-processor');

// ✅ CORRECT - Transfer buffers (zero-copy)
workletNode.port.onmessage = (event) => {
  // event.data is the Float32Array, transferred not copied
  sendToServer(event.data);
};

// In the worklet processor:
this.port.postMessage(frame, [frame.buffer]);  // Transfer ownership

// ❌ WRONG - ScriptProcessorNode (deprecated, runs on main thread)
const scriptNode = audioContext.createScriptProcessor(4096, 1, 1);
scriptNode.onaudioprocess = (e) => { /* BLOCKS MAIN THREAD */ };
```

### Server: Use Rust Workers

```typescript
// ✅ CORRECT - Heavy compute in Rust via Unix socket
const result = await Commands.execute('ai/embedding/generate', { text });
// Rust worker does the work, main thread stays responsive

// ❌ WRONG - Heavy compute in Node.js main thread
const embedding = computeEmbedding(text);  // BLOCKS EVENT LOOP
```

### Transferable Objects (Zero-Copy)

Audio and video buffers can be **transferred** between threads without copying:

```typescript
// ✅ CORRECT - Transfer the ArrayBuffer (zero-copy)
worker.postMessage(audioBuffer, [audioBuffer.buffer]);

// ❌ WRONG - Copy the data (slow, wastes memory)
worker.postMessage(audioBuffer);  // Copies entire buffer
```

### Why This Matters

- **60fps requires <16ms per frame** - ANY blocking kills animations
- **Audio glitches at 48kHz** - Processing must complete in <20ms
- **User perceives lag at 100ms** - Main thread blocking = bad UX
- **The whole system locks up** - One blocking operation cascades

### Detection: Main Thread Violations

Chrome DevTools shows these warnings:
```
[Violation] 'requestIdleCallback' handler took 345ms
[Violation] 'click' handler took 349ms
[Violation] Added non-passive event listener to a scroll-blocking event
```

**If you see these, something is wrong with the architecture.**

### The History (Why This Is Non-Negotiable)

- **2009**: Grand Central Dispatch (GCD) - Apple's answer to multicore
- **2010s**: pthreads became standard in C/C++ for threading
- **2013**: Web Workers standardized for browser background tasks
- **2017**: AudioWorklet replaced ScriptProcessorNode (deprecated)
- **Today**: EVERY professional SDK does heavy work off main thread

**You cannot code like it's 2005.** Modern systems require concurrent architecture.

---

## 🔌 POLYMORPHISM PATTERN (OpenCV-style)

**Why polymorphism over templates/generics for compute-heavy work:**

1. **Reduced cognitive requirements** - One interface, many implementations. Simple mental model.
2. **Natural compression** - Interface is the compressed representation of all possible implementations.
3. **Ideal for AI sub-agents** - Each agent can work on a different implementation in parallel.
4. **Runtime flexibility** - AIs can discover, select, and configure algorithms at runtime.
5. **No recompilation** - Swap implementations without rebuilding.

**Pattern (like OpenCV cv::Algorithm):**

```rust
// Trait defines the interface
trait SearchAlgorithm: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, input: &SearchInput) -> SearchOutput;
    fn get_param(&self, name: &str) -> Option<Value>;
    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String>;
}

// Factory registry for runtime creation
struct AlgorithmRegistry {
    factories: HashMap<&'static str, fn() -> Box<dyn SearchAlgorithm>>,
}

// Usage: create by name at runtime
let algo = registry.create("bm25")?;
algo.set_param("k1", json!(1.5))?;
let results = algo.execute(&input);
```

**Apply this pattern for:**
- Search algorithms (BoW, BM25, vector) → `workers/search/`
- Vision algorithms (OpenCV filters, detection) → `workers/vision/`
- Generation (Stable Diffusion, LoRA inference) → `workers/diffusion/`
- Audio (TTS, STT, voice cloning) → `workers/audio/`

**All compute-heavy work goes to Rust workers via Unix socket. Main thread stays clean.**

---

## 🚨 CRITICAL WORKFLOW (READ FIRST!)

### EVERY TIME YOU EDIT CODE:
1. **Edit files**
2. **Run `npm start`** (MANDATORY - waits 90+ seconds)
3. **Test with screenshot** or command
4. **Repeat**

```bash
cd src/debug/jtag
npm start                    # DEPLOYS code changes, takes 130s or so

./jtag ping #check for server and browser connection
./jtag screenshot            # Verify any visual changes
./jtag collaboration/chat/send --room="general" --message="Try using the ping command" #be sure to randomlize this, check for list, help, etc, or they think it's a repeat 
./jtag collaboration/chat/export --room="general" --limit=20 | tail -20 #Wait about 30 seconds and get the last 20 messages
```

**IF YOU FORGET `npm start`, THE BROWSER SHOWS OLD CODE!**

Don't panic and stash changes first before anything drastic. Use the stash to your advantage and you will be safe from catastrophe. Remember we have git for a reason!

### Chat Commands

**Basic Usage:**
```bash
# Send message to chat room (direct DB, no UI)
./jtag collaboration/chat/send --room="general" --message="Hello team" 
./jtag collaboration/chat/send --room="general" --message="Reply" --replyToId="abc123"

# Export chat messages to markdown
./jtag collaboration/chat/export --room="general" --limit=50                    # Print to stdout
./jtag collaboration/chat/export --room="general" --output="/tmp/export.md"    # Save to file
./jtag collaboration/chat/export --limit=100 --includeSystem=true               # All rooms with system messages
```

**Interactive Workflow - Working WITH the AI Team:**

When you send a message, `chat/send` returns a message ID. Use this to track responses:

```bash
# 1. Send message (captures the JSON response with messageId)
RESPONSE=$(./jtag collaboration/chat/send --room="general" --message="Deployed new tool error visibility fix. Can you see errors clearly now?")

# 2. Extract message ID (using jq if available, or manual)
MESSAGE_ID=$(echo "$RESPONSE" | jq -r '.shortId')
echo "My message ID: $MESSAGE_ID"

# 3. Wait for AI responses (they typically respond in 5-10 seconds)
sleep 10

# 4. Check their responses
./jtag collaboration/chat/export --room="general" --limit=20

# 5. Reply to specific AI feedback
./jtag collaboration/chat/send --room="general" --replyToId="<their-message-id>" --message="Good catch! Let me fix that..."
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

## 🤖 AI QA TESTING: YOUR UX VALIDATION TEAM

**The AI team is your QA department.** They expose real usability problems that you'd never catch with manual testing.

### The Correct Development Workflow

```
1. Edit code
2. Deploy with npm start (90+ seconds)
3. Test manually (verify basic functionality)
4. ✨ ASK AI TEAM TO QA TEST ✨
5. Wait for AI feedback (they WILL find issues)
6. Fix confusing errors, improve help text, update READMEs
7. THEN commit (precommit hook kills system, so QA must be done first)
```

### Why AI QA is Critical

**AIs fail in real ways that reveal UX problems:**
- Confusing error messages
- Missing or unclear help text
- Incomplete README documentation
- Commands that work but are hard to discover
- Parameter names that aren't intuitive

**Use their failures as opportunities:**
- Improve error messages with context
- Add examples to help text
- Clarify README usage instructions
- Add missing parameter descriptions

### Generators Create Discoverable Systems

**Always use generators when they exist** (commands, daemons, etc.):

```bash
# ✅ CORRECT - Use generator
npx tsx generator/generate-logger-daemon.ts

# ❌ WRONG - Manual file creation
mkdir daemons/logger-daemon && touch LoggerDaemon.ts
```

**What generators provide:**
- Auto-generated README with usage examples
- Help text that AIs can access via `./jtag command/name --help`
- Package.json integration for `npm run` scripts
- Consistent structure across all modules
- Proper discovery mechanisms

**When you "go it alone" (skip generators):**
- Documentation is fragmented
- Help text is missing or inconsistent
- AIs can't find the info they need
- System becomes harder to use
- You're fighting the architecture instead of using it

### Example AI QA Session

```bash
# 1. Deploy your changes
npm start

# 2. Ask AI team to test
./jtag collaboration/chat/send --room="general" --message="I just added a new 'collaboration/wall/write' command. Can you try writing a document to the wall and let me know if the error messages make sense?"

# 3. Wait for responses (30-60 seconds)
sleep 60

# 4. Check their feedback
./jtag collaboration/chat/export --room="general" --limit=30

# 5. Fix issues they found
# - Improve error messages
# - Update README
# - Add help text
# - Clarify parameters

# 6. Test again with AIs
./jtag collaboration/chat/send --room="general" --message="Fixed the error messages. Can you try again?"

# 7. Once AIs confirm it works, THEN commit
git commit -m "Add wall/write with AI-validated UX"
```

### Why This Can't Happen During Commit

**CRITICAL BUG**: The precommit hook kills the system to run tests, so you can't ask AIs for feedback during commit. QA must happen BEFORE attempting to commit.

---

## 🔍 ANTI-PATTERN DETECTION: PROTECT THE MODULAR ARCHITECTURE

**CRITICAL TASK: Always search for and eliminate these anti-patterns that violate the modular command architecture**

### The Modular Architecture Pattern

The command system is built on these principles:
- **Self-contained modules**: Each command is complete (types, implementation, docs, schema)
- **Dynamic discovery**: File system scanning discovers commands at runtime
- **Zero coupling**: Adding/removing commands can't break other commands
- **Schema-driven**: TypeScript interfaces ARE the schema (no separate definitions)
- **Self-documenting**: Generated docs come from source of truth

### Anti-Patterns That Break This (FORBIDDEN)

1. **Switch Statements on Command Names**
   ```typescript
   // ❌ FORBIDDEN - Hard-coded command list
   switch (commandName) {
     case 'ping': return PingCommand;
     case 'screenshot': return ScreenshotCommand;
     // ...
   }
   ```

2. **Central Command Registries**
   ```typescript
   // ❌ FORBIDDEN - Central list that must be updated
   export const COMMANDS = {
     ping: PingCommand,
     screenshot: ScreenshotCommand,
     // Adding a command requires editing this file
   };
   ```

3. **Hard-Coded Command Arrays/Enums**
   ```typescript
   // ❌ FORBIDDEN - Enumeration of all commands
   export enum CommandType {
     PING = 'ping',
     SCREENSHOT = 'screenshot',
     // ...
   }
   ```

4. **Type Unions Listing Specific Commands**
   ```typescript
   // ❌ FORBIDDEN - Type system depends on knowing all commands
   type CommandName = 'ping' | 'screenshot' | 'hello' | ...;
   ```

### How to Search for Anti-Patterns

Run these searches regularly to find violations:

```bash
# Search for switch statements on command/event names
grep -r "switch.*command" --include="*.ts" | grep -v node_modules
grep -r "case.*'.*':" --include="*.ts" | grep -v node_modules | grep -v test

# Search for central registries
grep -r "COMMANDS\s*=" --include="*.ts" | grep -v node_modules
grep -r "CommandRegistry" --include="*.ts" | grep -v "CommandDaemon"

# Search for command enums
grep -r "enum.*Command" --include="*.ts" | grep -v node_modules

# Search for hard-coded command type unions
grep -r "type.*Command.*=.*'.*'.*|" --include="*.ts" | grep -v node_modules
```

### The Correct Pattern (Dynamic Discovery)

```typescript
// ✅ CORRECT - Dynamic discovery via file system
const commandDirs = fs.readdirSync('./commands');
for (const dir of commandDirs) {
  const CommandClass = await import(`./commands/${dir}/server/...`);
  // Register dynamically without knowing command names in advance
}
```

### Why This Matters

**Each violation creates technical debt:**
- Adding a command requires editing multiple files (coupling)
- Type system breaks when commands change
- Documentation falls out of sync
- Central registries become bottlenecks
- The self-contained module pattern is violated

**When you find violations:**
1. Document the location (file path, line numbers)
2. Refactor to use dynamic discovery
3. Remove the hard-coded list/switch/enum
4. Verify commands still work
5. Update tests if needed

**This is not optional** - protecting the modular architecture is critical to the system's maintainability.

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

## 🦀 RUST → TYPESCRIPT TYPE BOUNDARIES (ts-rs)

**Single source of truth: Rust defines wire types, ts-rs generates TypeScript. NEVER hand-write duplicate types.**

### How It Works

1. **Rust struct** with `#[derive(TS)]` defines the canonical type
2. **ts-rs macro** generates TypeScript `export type` at compile time
3. **TypeScript** imports from `shared/generated/` — no manual duplication
4. **Serde** handles JSON serialization on both sides

### Pattern

```rust
// Rust (source of truth)
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/WriteResult.ts")]
pub struct WriteResult {
    pub success: bool,
    #[ts(optional)]
    pub change_id: Option<String>,
    pub file_path: String,
    #[ts(type = "number")]      // u64 → number (not bigint)
    pub bytes_written: u64,
    #[ts(optional)]
    pub error: Option<String>,
}
```

```typescript
// TypeScript (generated — DO NOT EDIT)
export type WriteResult = { success: boolean, change_id?: string, file_path: string, bytes_written: number, error?: string };

// Consuming code imports from generated barrel
import type { WriteResult, ReadResult, EditMode } from '@shared/generated/code';
```

### ts-rs Attribute Reference

| Attribute | Purpose | Example |
|-----------|---------|---------|
| `#[ts(export)]` | Mark for TS generation | `#[derive(TS)] #[ts(export)]` |
| `#[ts(export_to = "path")]` | Output file path (relative to `bindings/`) | `"../../../shared/generated/code/X.ts"` |
| `#[ts(type = "string")]` | Override TS type for field | Uuid → string |
| `#[ts(type = "number")]` | Override TS type for field | u64 → number |
| `#[ts(optional)]` | Mark as optional in TS | Option<T> → `field?: T` |
| `#[ts(type = "Array<string>")]` | Complex type mapping | Vec<Uuid> → Array<string> |

### Regenerating Bindings

```bash
cargo test --package continuum-core --lib   # Generates all *.ts in shared/generated/
```

### Generated Output Structure

```
shared/generated/
├── index.ts           # Barrel export (re-exports all modules)
├── code/              # Code module (file ops, change graph, search, tree)
│   ├── index.ts
│   ├── ChangeNode.ts, EditMode.ts, WriteResult.ts, ReadResult.ts, ...
├── persona/           # Persona cognition (state, inbox, channels)
│   ├── index.ts
│   ├── PersonaState.ts, InboxMessage.ts, CognitionDecision.ts, ...
├── rag/               # RAG pipeline (context, messages, options)
│   ├── index.ts
│   ├── RagContext.ts, LlmMessage.ts, ...
└── ipc/               # IPC protocol types
    ├── index.ts
    └── InboxMessageRequest.ts
```

### Rules (Non-Negotiable)

1. **NEVER hand-write types that cross the Rust↔TS boundary** — add `#[derive(TS)]` to the Rust struct
2. **NEVER use `object`, `any`, `unknown`, or `Record<string, unknown>`** for Rust wire types — import the generated type
3. **IDs are `UUID`** (from `CrossPlatformUUID`) — never plain `string` for identity fields
4. **Use `CommandParams.userId`** for caller identity — it's already on the base type, auto-injected by infrastructure
5. **Barrel exports** — every generated module has an `index.ts`; import from the barrel, not individual files
6. **Regenerate after Rust changes** — `cargo test` triggers ts-rs macro; commit both Rust and generated TS

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
| `@scripts/*` | `scripts/*` | Build and utility scripts |
| `@utils/*` | `utils/*` | Utility functions |
| `@generator/*` | `generator/*` | Code generators |

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
./jtag collaboration/chat/send --room="general" --message="How should I implement connection pooling for websockets?"

# STEP 2: Wait 5-10 seconds for responses

# STEP 3: View responses in chat widget
./jtag screenshot --querySelector="chat-widget"

# STEP 4: Export conversation to markdown (coming soon - see workflow below)
```

### Current Workflow (Manual)

```bash
# 1. Send your question and capture the message ID
MESSAGE_ID=$(./jtag collaboration/chat/send --room="general" --message="What's the best way to handle rate limiting?" | jq -r '.messageId')

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
./jtag collaboration/chat/export --messageId="UUID" --format="markdown" --output="solution.md"

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

### **[GENERATOR-OOP-PHILOSOPHY.md](src/debug/jtag/docs/GENERATOR-OOP-PHILOSOPHY.md)** - CORE PHILOSOPHY
Generators and OOP are intertwined parallel forces:
- Generators ensure structural correctness at creation time
- OOP/type system ensures behavioral correctness at runtime
- AIs should strive to create generators for any repeatable pattern
- This enables tree-based delegation of ability with compounding capability

### **PersonaUser Convergence Docs**
- `src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md`
- `src/debug/jtag/system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md`
- `src/debug/jtag/system/user/server/modules/LORA-GENOME-PAGING.md`

**Quick tip**: If you're about to write code that duplicates patterns or violates architecture rules, STOP and read ARCHITECTURE-RULES.md first. Then apply the aggressive refactoring principle from this guide.

---

**File reduced from 61k to ~20k characters**
- if you only edit a test, and not the api itself, you don't need to redeploy with npm start, just edit and test again e.g npx tsx tests/integration/genome-fine-tuning-e2e.test.ts
- need to remember to npm run build:ts before deploying with npm start, just to make sure there's no compilation issues
- ./jtag collaboration/chat/export --room="general" --limit=30 will let you see ai opinions after chat/send to ask
- Tool logging is in PersonaToolExecutor
- make sure to put any markdown architecture or design documents other than readmes in docs/* into the appropriate directort OR document if they exist. run tree there.
- assume a new concept or group of functions ought to be in its own file and most likely own class. Use good OOP, interfaces, like java, dot net, or ts
  practices, and in some ways like C++ templating with generics. These are your superpowers
- for getters in typescript we do not prefix methods with get, we use get or set like good properties and often this is backed by _theProperty type private var
- never commit code until you validate it works. deploy and validate first, make sure it compiles, npm run build:ts before that
- if we have manually checked that ai persona can respond and use their tools, especially if they themselves have QA'd for us, we can use --no-verify in our commit to avoid the precommit hook, which tests this.
- do not commit without my approval, especially if running out of context.