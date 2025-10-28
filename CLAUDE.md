# CLAUDE - ESSENTIAL DEVELOPMENT GUIDE

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

**File reduced from 61k to ~20k characters**
