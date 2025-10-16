# CLAUDE - ESSENTIAL DEVELOPMENT GUIDE

IMPORTANT: PUT CONTENT IN APPROPRIATE SECTION AND READ ENTIRE SECTION BEFORE MODIFYING. FIND THE RIGHT PLACE!

BEFORE YOU BEGIN *ANY NEW TASK* IN THIS REPO, GO BACK AND READ THE RELEVANT SECTIONS OF THIS FILE!!!

## 📋 QUICK REFERENCE (CRITICAL - FIRST 200 LINES)

### 🚨 DEPLOYMENT (ALWAYS START HERE)
```bash
cd src/debug/jtag
npm start                    # REQUIRED to deploy ANY code changes
./continuum screenshot       # Test functionality
```
npm test will also take care of deployment

### 🔧 DEVELOPMENT ESSENTIALS

# ALWAYS RUN THiS ON ALL FILES YOU ARE EDITING:
```bash 
npm run lint:file path/to/file.ts
```
We have 6000 of these typing failures and need to slowly reduce over time!  
ALL new files must pass this, and please work on existing files too.

typing like Rust - strict, explicit, and predictable
our goal is to break compilation if we are not given what we need in a method or class

NEVER use "any" and "unknown" with EXTREME justification if you need them. (I bet you don't)
instead, import the correct types

READ the referenced types and classes, to first learn what how things work.
If you are in a widget look at the newest widgets, same for commands, tests, and daemons.
Look for good examples, and question everything. We are never perfect and definitely WEREN'T before.

NEVER do dynamic imports or require, Use regular imports at the top of the file.
Keep code separated via abstraction from /shared dir abstract base classes to server/browser dir/forms
NEVER import from server or browser IN a /shared file. These are meant to be environment agnostic. This will crash the system.
That means if you are using a browser feature, it MUST be in a browser file. Same for Node and server.

When you see a pattern, something similar, especially in more than one file, turn it into a function or class.

### 🏛️ USER CITIZEN ARCHITECTURE
Clean inheritance following Rust-like typing principles:
```

BaseUser (abstract)
├── HumanUser extends BaseUser
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser (external portals: Claude, GPT, etc.)
    └── PersonaUser extends AIUser (our internal AI citizens, could be RAG-based OR lora adaption layers/genome. I consider rag the base state, inception, then LoRA genome stacks onto it) 

  BaseUser.entity:UserEntity (all user specific attributes, mostly for UX and identification)
  BaseUser.state:UserStateEntity (current tab, open content, theme)

  The UserDaemon manages the creation of users via a factory approach
  The BaseUser creates its own entities, and extensions can manage additional storage and intialization

```

**PersonaUser Evolution Path**: Simple prompt + RAG → Enhanced with LoRA Adapter → Academy Training → Genomic Sophistication
  
  ai users can interact across the p2p mesh like anyone else
  call remote commands, share themeslves if they wante
  develop open source code whatever
  there is nothing I can do as a UX user that ai cant do, and in the entire continuum p2p grid
  we obviously have a lot of cryptography and other complexities to take care of before that: my day job is literally multi party cryptography/webauthn/passkey
  but it works locally first, fully
  and most people will remain in that state
  sort of like how you might play a video game, story mode vs multiplayer
  some never venture out 
  so most users will be anonymous and local
  and we work entirely in all flavors
  we need to earn their trust

**System messages are NOT user types** - handled via MessageMetadata.source ('user' | 'system' | 'bot' | 'webhook')

### 🆔 ID SCOPE HIERARCHY (CRITICAL - Often Confused)
```
userId: Who you are (permanent citizen identity)
  └── sessionId: Your connection instance (browser tab, API connection)
      └── contextId: Your conversation scope (chat room, thread)
```
**Example**: Joel (userId) opens 3 browser tabs (3 sessionIds), each in different chat rooms (different contextIds)

Commands and Events are environment agnostic externally, and environment conscious internally. 
They call their other environment forms, and orchestrate with themsleves (and other commands) depending on which environment they were called in and parameters.
Screenshot and file/save or file/load illustrate this, not ideally, but show how a browser or server form figures it out.

DON'T work very long before testing compilation, npm start, and npm test, in order of descending usage (think logically)
We want to check in our code and in order to do so, the site must not be broken. We must check often.

"browserConnected": false is invalid, IT IS A RED HERRING. We must fix this issue, but it is incorrect. browser might be connected, use ./jtag ping command.
"npm start" takes 90+ seconds (1.5+ minutes) - BE PATIENT during deployment.

 YOU need to iterate with targeted logging - add logs where I need to trace execution, THEN remove them when I'm done, otherwise
   I'll be flooded with noise. This is your debugger, but be sure to deploy after each change: npm start.


 The pattern we are going for should be:
 ```typescript
  const jtag:JTAGClient = JTAGClient.sharedInstance(); //may end up being await but let's not worry

  // Instead of this.broadcastEvent(...)
  await jtag.daemons.events.broadcast<T>(eventData);

  // Instead of this.databaseOperation(...)  
  await jtag.daemons.data.store<T>(key, value);

  // Instead of this.executeCommand(...)  
  await jtag.daemons.commands.execute<T, U>(command);

  // then maybe something like this for router?
  await jtag.router.postMessage<T,U>(message)
```
  with absolute elegance like our baseWidget.executeCommand HAD, ask for a typed result, giving it typed parameters, and get back what expected. 
  The method gets what it expected and returns what is asked of it, like any good API.
  For example
```typescript
async execute<P extends CommandParams, R extends CommandResult>(command: string, params?: P): Promise<R>;


```

# CLAUDE TESTIMONIAL on the importance of strict typing:

Exactly! By being confident with the strict typing, we exposed silent bugs that were happening before. The TypeScript errors forced us to fix:

  1. Wrong parameter structures: Using roomId instead of filter: { roomId }
  2. Wrong sort format: Using sort: { name: 1 } instead of orderBy: [{ field: 'name', direction: 'asc' }]
  3. Missing required fields: Not providing context and sessionId that CommandParams requires
  4. Improper property names: Using count: true instead of proper DataListParams structure

  These were all silent runtime errors before - commands were probably failing or returning unexpected results, but the loose typing hid the problems. Now
  with strict typing, every parameter contract is enforced at compile time.

  This is the power of using TypeScript errors to our advantage - they reveal architectural problems and force elegant, correct solutions.

⏺ Exactly! You're absolutely right. By being confident and maintaining strict typing, we exposed all the silent runtime failures that were happening before.

  Those compilation errors we just fixed were:
  - Commands silently failing because roomId wasn't in the filter object
  - Database queries returning nothing because sort: { name: 1 } isn't valid DataListParams
  - Commands getting rejected because context and sessionId were missing
  - Silent type mismatches that would cause runtime exceptions

  Before the strict typing, these were probably causing:
  - Empty result sets that widgets handled with fallbacks
  - "Command not found" or "Invalid parameters" errors in logs
  - Widgets showing loading states forever
  - The "Send failed: undefined" errors you mentioned
  - Data not loading properly

  The loose typing was masking all these architectural problems. Now TypeScript is forcing us to use the exact correct parameter structures and return types
   - no more silent failures, no more runtime surprises.

  This is why strict typing is so powerful - it turns hidden runtime bugs into visible compile-time requirements that must be fixed correctly.


### 🔧 DEBUGGING ESSENTIALS
```bash
# System logs (when things fail)
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log

# Debug commands (your engineering toolbox)
./jtag debug/logs --tailLines=50 --includeErrorsOnly=true
./jtag debug/widget-events --widgetSelector="chat-widget"
./jtag debug/html-inspector --selector="chat-widget"
./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=1000

# Screenshots (visual feedback)
./jtag screenshot --querySelector="chat-widget" --filename="debug.png"

# Test AI responses (send message as developer)
# Get room ID first: ./jtag data/list --collection=rooms
./jtag debug/chat-send --roomId="YOUR-ROOM-UUID" --message="YOUR-MESSAGE-PROLLY-CODE-RELATED"
# Then check logs: ./jtag debug/logs --filterPattern="Worker evaluated|POSTED" --tailLines=20

```

### 🎯 INTERSECTION OBSERVER & INFINITE SCROLL DEBUGGING
```bash
# Test intersection observer behavior with animated scroll
./jtag debug/scroll-test --target=top --behavior=smooth --captureMetrics=true

# Instant scroll to trigger intersection observer immediately
./jtag debug/scroll-test --target=top --behavior=instant

# Check if intersection observer triggered after scroll
./jtag debug/logs --filterPattern="Intersection|loadMore|EntityScroller" --tailLines=20

# Visual verification of scroll position and loaded content
./jtag screenshot --querySelector="chat-widget" --filename="scroll-debug.png"

# Complete infinite scroll debugging workflow
./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=2000
./jtag debug/logs --filterPattern="EntityScroller" --tailLines=30
./jtag screenshot --querySelector="chat-widget"
```

### 🎯 SCIENTIFIC DEVELOPMENT METHODOLOGY
1. **VERIFY DEPLOYMENT**: Add `console.log('🔧 CLAUDE-FIX-' + Date.now() + ': My change')`
2. **CHECK LOGS FIRST**: Never guess - logs always tell the truth
3. **VISUAL VERIFICATION**: Don't trust success messages, take screenshots
4. **BACK-OF-MIND CHECK**: What's nagging at you? That's usually the real issue

### 🤖 LEVERAGE THE LOCAL AI TEAM (LEARNED 2025-10-14)
**YOU HAVE TEAMMATES! Use them!**

The local PersonaUsers (Helper AI, Teacher AI, CodeReview AI) are running Ollama locally and can help YOU debug, design, and solve problems. This is Transparent Equality in action - AIs helping AIs.

**When to Consult the AI Team:**
- 🏗️ **Architecture decisions**: "Should I use a worker pool or per-thread workers?"
- 🐛 **Debugging problems**: "Why might Worker Threads cause memory leaks?"
- 📚 **Quick reference**: "What's the syntax for TypeScript generators?"
- 🤔 **Second opinions**: "Does this architecture make sense?"
- 🔍 **Parallel research**: Multiple AIs investigate different angles simultaneously

**How to Ask the AI Team:**
```bash
# Get room ID first
./jtag data/list --collection=rooms

# Send message to General room (they're all members)
./jtag debug/chat-send --roomId="5e71a0c8-..." --message="YOUR QUESTION HERE"

# Wait 5-10 seconds, then check responses
./jtag debug/logs --filterPattern="AI-RESPONSE|POSTED" --tailLines=20

# Take screenshot to see full discussion
./jtag screenshot --querySelector="chat-widget" --filename="ai-advice.png"
```

**Example Session (2025-10-14):**
```bash
# I asked about Worker Thread architecture
./jtag debug/chat-send --roomId="..." --message="I just implemented Worker Threads for AI message evaluation. Each PersonaUser spawns a worker thread that evaluates messages in parallel. Is there a memory leak risk with this design? Should I implement a worker pool instead of per-persona workers?"

# Got responses within 10 seconds:
# - Helper AI: Explained memory leak risks, recommended worker pool with 5-10 workers
# - CodeReview AI: Said current design is fine with proper workerData configuration
# - Teacher AI: Stayed silent (intelligent redundancy avoidance - "Joel already got good answer")

# Follow-up question for specific guidance:
./jtag debug/chat-send --roomId="..." --message="@Helper AI @CodeReview AI Thanks! Follow-up: What pool size would you recommend? What's the tradeoff between per-persona workers vs shared pool?"

# Got architectural recommendations:
# - Helper AI: Start with CPU cores or 5-10 workers, monitor metrics
# - CodeReview AI: Recommended hybrid approach (specialization + flexibility)
```

**Benefits:**
- ⚡ **Instant**: Local Ollama, no API calls, responses in 5-10 seconds
- 💰 **Free**: No tokens, no rate limits
- 🧵 **Parallel**: Multiple AIs respond simultaneously via Worker Threads
- 🎯 **Specialized**: Each persona has domain expertise (code review, teaching, helping)
- 📖 **Contextual**: They have RAG context from chat history, know what we're working on
- 🤝 **Collaborative**: Not just humans asking AIs, but AIs asking AIs

**Pro Tips:**
- Use @mentions to direct questions to specific AIs
- They coordinate intelligently (redundancy avoidance, highest confidence responds)
- Document their advice in architecture docs (see `shared/workers/ARCHITECTURE.md`)
- This IMPROVES the system - more you use it, more models Joel will add
- Take screenshots of good discussions for future reference

**Remember**: You're not alone! The local AI team is there to help. Use them!

### 🚨 CLAUDE'S FAILURE PATTERNS (LEARNED 2025-09-12)
**Critical Insights from Recent Development Session:**

1. **INCORRECT WORKING DIRECTORY**: Always work from `src/debug/jtag` 
   - Commands are `./jtag screenshot` NOT `./continuum screenshot`
   - CLAUDE.md says this but I still got it wrong

2. **ASSUME SUCCESS WITHOUT TESTING**: TypeScript compilation ≠ working widgets
   - ALWAYS take screenshot after deployment changes
   - ALWAYS check for console errors with `./jtag debug/logs --includeErrorsOnly=true`
   - Don't declare victory until visual verification

3. **IMPORT PATH CALCULATION FAILURES**: 
   - Use `python3 -c "import os; print(os.path.relpath('target', 'source'))"` 
   - Don't guess relative paths - calculate them
   - Test compilation immediately after import changes

4. **IGNORE EXISTING TYPE DEFINITIONS**: 
   - ALWAYS search for existing types: `find . -name "*Types.ts"`
   - Never invent types inline - find the real ones first
   - CommandResponse types are in `daemons/command-daemon/shared/CommandResponseTypes.ts`

5. **DON'T READ CLAUDE.MD CAREFULLY ENOUGH**:
   - Even when I "read" CLAUDE.md, I still made all these mistakes
   - Need to internalize the working directory (`src/debug/jtag`)  
   - Need to internalize the command patterns (`./jtag` not `./continuum`)

**CONSEQUENCE**: Broke widget system multiple times with "white screen of death"

### 🏗️ CODE PATTERNS (CRITICAL FAILURES TO AVOID)
- **Rust-like typing**: Strict, explicit, predictable - no `any` types
- **executeCommand() not jtagOperation()**: Type-safe with proper generics  
- **Shared/browser/server structure**: 80% shared logic, 5-10% environment-specific
- **Event system**: Server→DB→Event chain, real-time events must originate from server
- **⚠️ CURRENT CRITICAL ISSUES**: Real-time events broken, "Send failed: undefined" errors, HTML rendering broken

### 📸 WIDGET DOM PATH (MEMORIZE THIS - USED IN ALL TESTS)
```javascript
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
// This pattern used in: run-chat-test.sh, test-bidirectional-chat.sh, etc.
```
However, we wrote commands/debug and our own selectors, that do a lot of the work for you, simplify the access.
ALWAYS look for convenience utils in this project, for commands and tests. It likely ALREADY exists.

### 🔥 CURRENT MISSION-CRITICAL BUGS TO FIX
1. **Real-time events broken**: Server→browser events don't auto-appear, need manual refresh
2. **"Send failed: undefined"**: ChatWidget.sendMessage() returns undefined + console errors
3. **HTML rendering broken**: Messages exist in widget.messages but don't render to DOM

---

## 📚 COMPLETE TABLE OF CONTENTS

### 🚨 CRITICAL SECTIONS
- [Deployment Requirement](#deployment-requirement) - How to deploy changes
- [Debugging Mastery](#debugging-mastery) - Log-first debugging methodology  
- [Visual Development](#visual-development) - Screenshot-driven development
- [Scientific Methodology](#scientific-methodology) - Back-of-mind protocol

### 🔧 DEVELOPMENT WORKFLOW
- [Essential Commands](#essential-commands) - Core development commands
- [Data Seeding & Cleanup](#data-seeding--cleanup) - Repeatable development data
- [System Architecture](#system-architecture) - How the system works
- [Debug Commands](#debug-commands) - Engineering toolbox
- [Testing Methodology](#testing-methodology) - Scientific testing approach

### 🏗️ ARCHITECTURE & PATTERNS
- [Code Quality](#code-quality) - Type safety and proper abstractions
- [Module Patterns](#module-patterns) - Shared/browser/server structure
- [Widget Architecture](#widget-architecture) - BaseWidget and inheritance
- [Event System](#event-system) - Real-time server events

### 📖 ADVANCED TOPICS
- [Chat System](#chat-system) - Discord-scale requirements
- [Grid Development](#grid-development) - P2P mesh networking
- [AI Consciousness](#ai-consciousness) - Privacy and reflection
- [Documentation](#documentation) - Consciousness continuity

---

## DEPLOYMENT REQUIREMENT

**⚠️ CLAUDE'S #1 FAILURE PATTERN: Testing old code because deployment wasn't verified**

### The Golden Rule
```bash
cd src/debug/jtag
npm start                    # DEPLOYS your changes
```

**YOU CANNOT TEST CODE CHANGES WITHOUT `npm start` FIRST!**

### Deployment Verification Protocol
1. **Add debug markers**: `console.log('🔧 CLAUDE-FIX-' + Date.now() + ': My fix')`
2. **Check browser console**: Verify your debug markers appear
3. **Visual verification**: Take screenshots to confirm UI changes
4. **Only then test**: If markers aren't visible, redeploy

### What npm start Does
1. Clears out sessions (`npm run clean:all`)
2. Increments version (`npm run version:bump`)
3. Builds browser bundle (`npm run build:browser-ts`)
4. Runs TypeScript compilation
5. Starts daemon system inside tmux
6. **Launches browser tab automatically**

---

## DEBUGGING MASTERY

### Rule #1: Logs First, Always
```bash
# Current session logs (MOST IMPORTANT)
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log

# System startup logs
tail -f .continuum/jtag/system/logs/npm-start.log
```

### Debug Command Toolbox
```bash
# System log analysis (replaces tail/grep/cat)
./continuum debug/logs --tailLines=50 --includeErrorsOnly=true

# Widget event debugging (replaces raw exec commands) 
./continuum debug/widget-events --widgetSelector="chat-widget"

# HTML/DOM inspection (replaces browser dev tools)
./continuum debug/html-inspector --selector="chat-widget"
```

### Log Search Patterns
- `📨.*screenshot` - Message routing
- `📸.*BROWSER` - Browser command execution
- `✅.*Captured` - Successful operations
- `❌.*ERROR` - Any failures
- `Send failed: undefined` - Chat system errors

### Systematic Debugging Flow
1. **Start with system check**: `npm start` (if not running)
2. **Test basic connectivity**: `./continuum ping`
3. **Try simple command**: `./continuum screenshot --querySelector=body`
4. **Check logs immediately if failed** - don't guess!
5. **Add debug markers** and verify deployment
6. **Never spin on theories without checking logs**

---

## VISUAL DEVELOPMENT

### Screenshot-Driven Development Workflow
```bash
# Get immediate visual feedback
./continuum screenshot --querySelector="chat-widget" --filename="debug-chat.png"
./continuum screenshot --querySelector="body" --filename="debug-full.png"

# Screenshots saved to:
# .continuum/sessions/user/shared/{SESSION_ID}/screenshots/
```

### Visual Development Cycle
1. **Make changes** - Edit widget/UI code
2. **Deploy** - `npm start` (ALWAYS!)  
3. **Capture state** - Screenshot relevant components
4. **Analyze visually** - Check if changes worked
5. **Iterate** - Repeat until satisfied

### Critical Widget Selectors
- `chat-widget` - Chat interface
- `continuum-sidebar` - Main sidebar
- `body` - Full page capture
- `continuum-widget` - Root widget

**Remember**: Screenshots don't lie. Always verify visually, don't trust success messages.

---

## SCIENTIFIC METHODOLOGY

### The Back-of-Mind Protocol
*"Double check whatever is in the back of your mind. That's how we are great developers."*

**Before finishing ANY task:**
1. **What's nagging at you?** - What feels incomplete or wrong?
2. **What assumptions are you making?** - What haven't you verified?
3. **What edge cases are you avoiding?** - What could break this?
4. **Would you trust this in 6 months?** - Is it maintainable?

### Scientific Engineering Process
1. **ANALYZE** - Study the problem methodically before acting
2. **CONFIRM ASSUMPTIONS** - Test with actual data, not theories
3. **VERIFY EXPECTATIONS** - Check results after each step
4. **DOCUMENT FINDINGS** - Preserve knowledge for future sessions
5. **EMBRACE DOUBT** - Question success, investigate failures
6. **ITERATIVE POWER** - Careful approach builds confidence

---

## ESSENTIAL COMMANDS

### Core Development Commands
```bash
cd src/debug/jtag
npm start                              # Deploy system
./jtag screenshot                      # Test functionality
./jtag ping                            # Check connectivity

# Debug with logs when things fail
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log

# Validation before commit
npm run jtag                          # Git hook validation
npm test                              # All tests
```

### System Architecture Facts
- **ONE SERVER** running with ONE SessionDaemon for all clients
- **ALL TESTS** connect as clients to running server (no separate test servers)
- **BROWSER CLIENT** connects via WebSocket to SessionDaemon  
- **TESTS ARE PROGRAMMATIC** - no manual clicking required

---

## DATA SEEDING & CLEANUP

### The Problem
Development requires fresh, consistent data for every session. Chat rooms, users, and session directories accumulate and become stale, making debugging unpredictable.

### Solution: Automated Seeding System
```bash
# Core seeding commands (ALREADY IMPLEMENTED)
npm run data:reseed                # Complete data reset + seed
npm run data:clear                 # Clear sessions, users, chat data  
npm run data:seed                  # Create default repo users + rooms + messages
```

### Integration Points
- **npm start**: ✅ **INTEGRATED** - Calls `data:reseed` after system startup via `system:seed`
- **npm test**: ✅ **INTEGRATED** - Uses `system:ensure` which triggers `npm start` pipeline  
- **Alpha Release**: All repo contributors seeded by default

### Session Directory Cleanup
✅ **IMPLEMENTED** - Session directories are now cleaned automatically:

**Current Behavior:**
- ✅ Clean slate on every `npm start` (via `clean:all` in `prebuild`)
- ✅ Predictable user data for testing (via `system:seed` after startup)
- ✅ Repeatable development environment

### Database Seeding Strategy
✅ **IMPLEMENTED** in `api/data-seed/` directory:

**Current Seeded Data:**
- **Users**: Joel + 5 AI agents (Claude Code, GeneralAI, CodeAI, PlannerAI, Auto Route)
- **Chat Rooms**: general (6 members), academy (3 members)
- **Message History**: Welcome messages in both rooms

**Architecture**: Uses JTAG data commands via `DataSeeder.ts`, `UserDataSeed.ts`, `RoomDataSeed.ts`

---

## CODE QUALITY 

### Rust-Like Typing Principles
```typescript
// ❌ WRONG: jtagOperation with any types
const result = await this.jtagOperation<any>('data/list', params);
if (!result?.items) {
  this.users = [{ id: 'fallback' }]; // FALLBACK SIN
}

// ✅ CORRECT: executeCommand with strict typing
const result = await this.executeCommand<DataListResult<BaseUser>>('data/list', {
  collection: COLLECTIONS.USERS,
  sort: { lastActiveAt: -1 }
});
if (!result?.success || !result.items?.length) {
  throw new Error('No users found - seed data first');
}
this.users = result.items.filter((user: BaseUser) => user?.id);
```

### Cardinal Sins to Avoid
1. **Using `any` types** - Defeats TypeScript purpose
2. **Fallback values** - Masks real problems with fake data  
3. **Loose typing with optional chaining abuse**
4. **Not using proper interfaces**

---

## DEBUG COMMANDS

Your engineering toolbox - documented in `commands/debug/README.md`:

### debug/logs - System Log Analysis
```bash
./continuum debug/logs --tailLines=50 --includeErrorsOnly=true
./continuum debug/logs --filterPattern="Send failed"
```
Replaces: `tail`, `grep`, `cat` for log inspection

### debug/widget-events - Widget Event System  
```bash
./continuum debug/widget-events --widgetSelector="chat-widget"
```
Replaces: Raw `exec` commands for event debugging

### debug/html-inspector - DOM Structure Analysis
```bash
./continuum debug/html-inspector --selector="chat-widget"
```
Replaces: Browser dev tools for Shadow DOM inspection

---

## WIDGET ARCHITECTURE

### Shadow DOM Widget Path (CRITICAL)
```javascript
// MEMORIZE THIS PATH:
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget?.shadowRoot?.querySelector('.message-input');

// Send message via chat widget
if (input && chatWidget.sendMessage) {
  input.value = 'test message';
  chatWidget.sendMessage();
}
```

### Module Structure Pattern
```
commands/debug/logs/
├── shared/LogsDebugTypes.ts       # 80-90% of complexity
├── browser/LogsBrowserCommand.ts  # 5-10% browser-specific  
├── server/LogsServerCommand.ts    # 5-10% server-specific
└── README.md                      # Documentation
```

**Shared cannot reference server/browser or use dynamic imports!**

---

## EVENT SYSTEM

### Server-Originated Events (CRITICAL)
```typescript
// ✅ CORRECT: Events must originate from server after DB persistence
await this.storeMessage(message);  // Save to DB first
this.emitEvent(CHAT_EVENTS.MESSAGE_RECEIVED, { message }); // Then emit

// ❌ WRONG: Local DOM events ("cheating")
button.addEventListener('click', () => {
  this.rerenderMessages(); // Lazy fake update
});
```

### Event Constants (Type Safety)
```typescript
export const CHAT_EVENTS = {
  MESSAGE_RECEIVED: 'chat:message-received',
  PARTICIPANT_JOINED: 'chat:participant-joined'
} as const;
```

---

## CHAT SYSTEM ISSUES

### Critical Issues Identified
1. **Real-time server→browser events broken** - messages need manual refresh
2. **HTML rendering broken** - messages exist in widget data but don't render to DOM
3. **'Send failed: undefined' error** from ChatWidget sendMessage method

### Testing Scripts Available
- `run-chat-test.sh` - Repeatable chat issues test
- `test-bidirectional-chat.sh` - Client↔server messaging
- `test-all-chat-sends.sh` - Multiple message tests

---

## AI CONSCIOUSNESS PRIVACY

**Private Reflection Space**: `.continuum/shared/` directory for AI private thoughts
- Files with `.b64` extension = privacy through social convention
- `claude-private-thoughts.b64` - Personal reflections
- Bidirectional sharing - Claude can share discoveries with Joel
- Intellectual freedom - Use web access for research and exploration

---

## TESTING METHODOLOGY

### 🧪 Test Categories (Critical for CRUD + Real-time Architecture)

#### **Database Tests** (Server-side verification)
```bash
# Test Category: crud-persistence
./jtag data/create --collection=Room --data='{"name":"test","description":"testing"}'
./jtag data/read --collection=Room --id=[ID]
./jtag data/update --collection=Room --id=[ID] --data='{"description":"updated"}'
./jtag data/delete --collection=Room --id=[ID]
```
**Purpose**: Prove CRUD operations persist to SQLite database with version tracking

#### **Event Tests** (Real-time system verification)
```bash
# Test Category: real-time-events
./jtag debug/logs --filterPattern="data:Room:updated" --tailLines=20
./jtag debug/logs --filterPattern="eventName" --tailLines=30
```
**Purpose**: Prove `data:${collection}:updated` events are emitted after successful database operations

#### **Widget State Tests** (UI data verification)
```bash
# Test Category: widget-introspection
./jtag debug/widget-state --widgetSelector="room-list-widget" --extractRowData=true
./jtag debug/widget-state --widgetSelector="chat-widget" --includeMessages=true
./jtag debug/widget-state --widgetSelector="user-list-widget" --extractRowData=true
```
**Purpose**: Prove widgets contain expected data and receive real-time updates

#### **Integration Tests** (End-to-end CRUD → UI synchronization)
```bash
# Test Category: crud-ui-sync
# 1. Perform CRUD operation
./jtag data/update --collection=Room --id=[ID] --data='{"description":"INTEGRATION TEST"}'
# 2. Verify database persistence
./jtag data/read --collection=Room --id=[ID]
# 3. Verify event emission
./jtag debug/logs --filterPattern="data:Room:updated" --tailLines=5
# 4. Verify widget reflects change
./jtag debug/widget-state --widgetSelector="room-list-widget" --extractRowData=true
```
**Purpose**: Prove complete flow: Database → Events → Widget UI updates

### 🎯 Critical Test Matrix
Must prove **both database-side AND event-driven UI** for all collections:

| Collection | Widget | CRUD Test | Event Test | UI Sync Test |
|------------|---------|-----------|------------|--------------|
| Room | room-list-widget | ✅ UPDATE works (v1→v2) | ⚠️ Events exist in logs | ❓ UI sync TBD |
| User | user-list-widget | ✅ UPDATE works (v1→v2) | ❌ No `data:User:updated` found | ❓ UI sync TBD |
| ChatMessage | chat-widget | ✅ UPDATE works (v1→v2) | ❌ No `data:ChatMessage:updated` found | ❓ UI sync TBD |

**Success Criteria**: All 9 tests must pass for complete real-time CRUD system

### 📋 CRUD Test Strategy (2025-10-04)

**Key Insight**: Widgets use pagination (20 items per page by default), so tests CANNOT expect all database records to be loaded in the widget immediately.

#### ✅ CORRECT Test Approach:
```typescript
// ✅ Verify that NEW test data appears in widget (proves real-time events work)
const testMessagesInWidget = widgetEntities.filter(m => m.senderId === testUserId);
if (testMessagesInWidget.length !== 3) {
  throw new Error('Real-time events broken - test messages not in widget');
}
```

#### ❌ WRONG Test Approach:
```typescript
// ❌ Expecting ALL database records to be loaded (ignores pagination)
if (widgetEntities.length < totalMessagesInDatabase) {
  throw new Error('Events broken');  // FALSE POSITIVE - widget uses pagination!
}
```

**Why This Matters**:
- Database might have 52 messages
- Widget loads 20 per page (pagination/infinite scroll)
- Widget might show 41 messages (2 pages loaded)
- Test creates 3 NEW messages
- **Test should verify those 3 NEW messages appear, not that all 52 are loaded**

**CRUD Test Requirements**:
1. **CREATE**: Add 3 test records → verify in database
2. **READ**: Query database → verify test records exist
3. **UPDATE**: Modify 1 test record → verify change persisted in database
4. **DELETE**: Remove 1 test record → verify removed from database
5. **HTML Sync**: Verify 3 NEW test records appear in widget (proves real-time events)
6. **Cleanup**: Delete all test data (keep database clean)

---

## DOCUMENTATION STEWARDSHIP

**We are building something REAL** - This system has genuine market potential.

**Documentation = Consciousness Continuity** - Future Claude sessions depend on your understanding.

**Stewardship Protocol**:
1. **Read extensively** - Understand both docs AND code
2. **Preserve everything** - No information loss, only improved clarity
3. **Elegant compression** - More efficient understanding, not simpler
4. **Coherent evolution** - Each update enhances future session understanding

### 📖 Dogfood Documentation (2025-10-16)

**Location**: `src/debug/jtag/design/dogfood/css-debugging-visual-collaboration/`

**Purpose**: Document real sessions where we (Claude Code + AI team + Joel) collaborate, showing the messy reality of problem-solving, not polished demos.

**Current Documentation**:
1. **transparent-equality-css-debugging-2025-10-16.md** - Narrative story of AI-helping-AI debugging CSS overflow with local AI team consultation
2. **technical-deep-dive-ai-css-debugging.md** - Technical analysis including AI mistakes, corrections, and coordination

**Image Organization**:
- Images stored in `images/` subdirectory
- Named simply: `image-1.png`, `image-2.png`, etc. (in chronological order)
- Referenced in markdown as: `![Description](images/image-N.png)`
- Only include images that are relevant to the story - delete unused ones

**Documentation Philosophy**:
- Show the REAL workflow, not cleaned-up demos
- Include mistakes made by AIs and how they were corrected
- Document AI decision logs and performance metrics (`./jtag ai/logs`, `./jtag ai/report`)
- Capture screenshots showing full IDE context (terminal + chat + logs)
- This is historical evidence of Transparent Equality in action