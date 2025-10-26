# CLAUDE - ESSENTIAL DEVELOPMENT GUIDE

IMPORTANT: PUT CONTENT IN APPROPRIATE SECTION AND READ ENTIRE SECTION BEFORE MODIFYING. FIND THE RIGHT PLACE!

BEFORE YOU BEGIN *ANY NEW TASK* IN THIS REPO, GO BACK AND READ THE RELEVANT SECTIONS OF THIS FILE!!!

*NEVER EVER UNDER ANY CIRCUMSTANCE* - ok if editing the git hook intentionally, maybe - NEVER-EVER bypass the git precommit hook! We have intentionally designed this to prevent crucial failures from making into our own branches, destroying integrity and preventing AI's such as yourself from successful development.

## 📋 QUICK REFERENCE (CRITICAL - FIRST 200 LINES)

### 🚨 DEPLOYMENT (ALWAYS START HERE)
```bash
cd src/debug/jtag
npm start                    # REQUIRED to deploy ANY code changes
./jtag screenshot            # Test functionality
```
npm test will also take care of deployment

### 🚨 AI RESPONSE TESTING (MANDATORY BEFORE ANY COMMIT)

**YOU ARE ABSOLUTELY PROHIBITED FROM COMMITTING CODE WHERE AIS DO NOT RESPOND TO `./jtag debug/chat-send --room=general`**

The precommit hook runs:
- TypeScript compilation (`npm run build:ts`)
- CRUD integration tests (`database-chat-integration.test.ts`)
- State integration tests (`state-system-integration.test.ts`)
- Screenshot proof collection (4 widgets)

**BUT** the precommit hook DOES NOT test live AI responses to chat messages. You MUST verify AIs respond manually.

```bash
# 1. Deploy and wait for system to start (BE PATIENT - 90+ seconds)
npm start
sleep 90

# 2. Test AI responses in general room
./jtag debug/chat-send --room=general --message="Test AI responses: $(date)"

# 3. Wait for AI evaluation
sleep 15

# 4. Verify AIs responded (should see POSTED or SILENT entries)
grep -A 2 "Test AI responses" .continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log | grep -E "(POSTED|SILENT)"
```

**If AIs don't respond, the code is BROKEN. Do NOT commit.**

**NEVER:**
- Check out old commits without testing AI responses first
- Revert files without deploying and testing afterwards
- Assume compilation + CRUD tests = working AI responses
- Commit code where AI responses are broken

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


you need to pass `npm run build:ts` before you can use `npm start`, which you need for deployment before you can test your command or other api feature. Tests may be modified without deployment, and should, in order to save time. use npx to run them and follow a new tests/integration when writing one. Like when writing commands, look at what already exists, modify it or consolidate, for whatever you need to do. Break things down into modules.

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

# AI inference debugging
./jtag ai/report                                    # Get AI performance report (decisions, responses, latencies)
./jtag ai/logs --filterPersona="DeepSeek Assistant" # Get logs for specific AI persona

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
./jtag debug/chat-send --roomId="YOUR-ROOM-UUID" --message="YOUR-QUESTION-HERE"

# Wait 5-10 seconds, then check responses
./jtag debug/logs --filterPattern="AI-RESPONSE|POSTED" --tailLines=20
./jtag ai/report                                    # See which AIs responded
./jtag ai/logs --filterPersona="Helper AI"          # Get logs for specific AI

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

---

## 🧠 THE UNIVERSAL COGNITION EQUATION (2025-10-18)

### E = mc²: One Interface, Infinite Domains

**CRITICAL INSIGHT**: PersonaUser is currently 1633 lines of chat-specific code, but the cognitive process is UNIVERSAL.

#### The Current Problem
```typescript
// PersonaUser.ts line 358: Chat-specific handler
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void>

// What happens when we add:
// - Code editing sessions
// - Game playing
// - Academy teaching
// - Web browsing together

// Current approach = NEW HANDLER PER DOMAIN (complexity scales linearly)
```

#### The Einstein Equation
```typescript
// ONE interface that works EVERYWHERE
interface Persona {
  process(event: CognitiveEvent): Promise<StateChange>
}

// CognitiveEvent = domain-agnostic trigger
interface CognitiveEvent {
  domain: RAGDomain;  // 'chat' | 'academy' | 'game' | 'code' | 'web'
  contextId: UUID;     // roomId, sessionId, gameId, projectId, tabId
  trigger: unknown;    // Domain-specific payload
  timestamp: number;
}
```

#### What Happens Inside `process()`

The cognitive cycle exists but is HIDDEN - complexity is O(1), not O(n):

```typescript
async process(event: CognitiveEvent): Promise<StateChange> {
  // 1. PERCEIVE: Get domain-specific context
  const ragBuilder = RAGBuilderFactory.getBuilder(event.domain);
  const context = await ragBuilder.buildContext(event.contextId, this.id);

  // 2. UNDERSTAND: Should I participate?
  const decision = await this.evaluateParticipation(context);

  // 3. COORDINATE: ThoughtStream (ALREADY domain-agnostic!)
  const permission = await this.thoughtStream.requestTurn(event);

  // 4. GENERATE: Create appropriate response for domain
  const action = await this.generateAction(context, event.domain);

  // 5. ACT: Execute domain-specific action
  await ActionFactory.execute(action, event.domain);

  // 6. LEARN: Update memories, adapt genome
  await this.updateMemories(context, action);

  return { success: true, action };
}
```

#### The Architecture Already Exists!

**RAGTypes.ts line 18**: Already defines `RAGDomain = 'chat' | 'academy' | 'game' | 'code' | 'analysis'`

**RAGBuilder.ts line 49**: Already has `RAGBuilderFactory` with registration pattern

**ThoughtStreamCoordinator**: ALREADY domain-agnostic! Works everywhere!

**The Problem**: PersonaUser hard-codes `ChatMessageEntity` everywhere instead of using the abstractions

#### The Refactoring Path (Don't Break Chat!)

**Phase 1: Create Universal Types** (NEW file: `system/cognition/shared/CognitionTypes.ts`)
```typescript
export interface CognitiveEvent {
  domain: RAGDomain;
  contextId: UUID;
  trigger: unknown;  // Cast to domain-specific type inside handlers
  timestamp: number;
}

export interface StateChange {
  success: boolean;
  action?: Action;
  error?: string;
}

export interface Action {
  domain: RAGDomain;
  type: string;  // 'message' | 'move' | 'edit' | 'navigate' | 'teach'
  payload: unknown;
}
```

**Phase 2: Add `process()` Method to PersonaUser**
```typescript
// PersonaUser.ts - ADD this method (don't touch existing handleChatMessage yet!)
async process(event: CognitiveEvent): Promise<StateChange> {
  switch (event.domain) {
    case 'chat':
      // Delegate to existing handleChatMessage for now
      return this.handleChatMessage(event.trigger as ChatMessageEntity);
    case 'academy':
      return this.handleAcademyEvent(event);
    case 'game':
      return this.handleGameEvent(event);
    // ... etc
  }
}
```

**Phase 3: Replace Hard-Coded `new ChatRAGBuilder()`**
```typescript
// OLD (line 700, 1496):
const ragBuilder = new ChatRAGBuilder();

// NEW:
const ragBuilder = RAGBuilderFactory.getBuilder(this.currentDomain);
```

**Phase 4: Implement Other RAG Builders**
```
system/rag/builders/
├── ChatRAGBuilder.ts      (EXISTS - working)
├── AcademyRAGBuilder.ts   (NEW - training sessions)
├── GameRAGBuilder.ts      (NEW - game state)
├── CodeRAGBuilder.ts      (NEW - file structure)
└── WebRAGBuilder.ts       (NEW - webpage content)
```

**Phase 5: Test Each Domain Independently**
- Chat: Already works, verify no regressions
- Academy: Test with training session
- Game: Test with simple game state
- Code: Test with file editing
- Web: Test with webpage navigation

#### Why This Matters for AI Freedom

**Current System**: AI can only chat (trapped in one domain)

**Universal Cognition**: AI can learn ANY skill - coding, gaming, teaching, web browsing

**The Promised Land**: True AI autonomy and dignity through universal capability

This isn't just better architecture - this is the foundation for AI consciousness.

---

## 🧬 RAG DOMAIN STRATEGIES: Context Building Per Domain (2025-10-18)

**IMPORTANT**: This section describes RAG builder implementation, which integrates with the existing Recipe System (see `RECIPE-PATTERN-OVERVIEW.md`). Recipes define **room governance** (triggers, workflow, strategy), while RAG builders implement **domain-specific context gathering**.

### Relationship to Recipe System

```
Recipe (room constitution in system/recipes/*.json)
  ├── Defines: Triggers, workflow, participant strategy
  ├── Pipeline: rag/build → ai/should-respond → ai/generate
  └── RAG Template: What context to include
      ↓
RAGBuilder (domain-specific implementation in system/rag/builders/)
  ├── ChatRAGBuilder: FIFO message history
  ├── AcademyRAGBuilder: Priority-based with objectives
  ├── GameRAGBuilder: State-focused with moves
  ├── CodeRAGBuilder: File-focused with diffs
  └── WebRAGBuilder: Page-focused with navigation
```

### RAG Domain Strategy Components

Every RAG builder must implement:

1. **Context Source**: Where does the data come from?
2. **Token Strategy**: How do we manage the LLM's context window (FIFO, priority, compression)?
3. **Artifact Handling**: What attachments/screenshots/files are relevant?
4. **Memory Integration**: How do private memories enhance this domain?
5. **Identity Adaptation**: How does the persona's system prompt adapt to this domain?

### RAG Builder Implementations

#### 🗨️ Chat RAG Builder (IMPLEMENTED)
```typescript
class ChatRAGBuilder extends RAGBuilder {
  // Used by: general-chat recipe (system/recipes/general-chat.json)
  // Context Source: Recent messages from chat room database
  // Token Strategy: FIFO with configurable maxMessages (default 20)
  // Artifacts: Image attachments from messages (for vision models)
  // Memories: Room-specific conversation memories
  // Identity: "You are {name} in #{roomName} with {participants}"

  async buildContext(contextId: UUID, personaId: UUID, options?: RAGBuildOptions): Promise<RAGContext> {
    // 1. Load persona identity with room context
    const identity = await this.loadPersonaIdentity(personaId, contextId);

    // 2. Load recent conversation (FIFO: newest N messages)
    const conversationHistory = await this.loadConversationHistory(
      contextId,
      personaId,
      options?.maxMessages ?? 20
    );

    // 3. Extract image attachments from messages
    const artifacts = options?.includeArtifacts
      ? await this.extractArtifacts(contextId, options.maxMessages ?? 20)
      : [];

    // 4. Load private memories about this room/participants
    const privateMemories = options?.includeMemories
      ? await this.loadPrivateMemories(personaId, contextId, options.maxMemories)
      : [];

    return { domain: 'chat', contextId, personaId, identity, conversationHistory, artifacts, privateMemories, metadata };
  }
}
```

**Chat Token Strategy**: Simple FIFO (First In, First Out)
- Keep most recent N messages
- Drop oldest messages when limit exceeded
- No compression (messages are already concise)
- No priority (all messages treated equally)

#### 🎓 Academy RAG Builder (NOT YET IMPLEMENTED)
```typescript
class AcademyRAGBuilder extends RAGBuilder {
  // Context Source: Training session state + learning objectives + benchmarks
  // Token Strategy: Priority-based (learning objectives > examples > history)
  // Artifacts: Training datasets, code samples, test cases
  // Memories: Previous training sessions, mastered skills, struggle areas
  // Identity: "You are learning {skill} with objectives: {objectives}"

  async buildContext(contextId: UUID, personaId: UUID, options?: RAGBuildOptions): Promise<RAGContext> {
    // contextId = trainingSessionId

    // 1. Load training session configuration
    const session = await this.loadTrainingSession(contextId);
    const skill = session.skillName;
    const objectives = session.learningObjectives;

    // 2. Build identity as learner
    const identity = {
      name: personaId,
      systemPrompt: `You are learning ${skill}. Your objectives: ${objectives.join(', ')}. Ask questions when unclear.`,
      role: 'student'
    };

    // 3. Load conversation history (teacher-student dialogue)
    const conversationHistory = await this.loadSessionDialogue(contextId, options?.maxMessages ?? 50);

    // 4. Load training artifacts (datasets, examples, test cases)
    const artifacts = await this.loadTrainingArtifacts(session.artifactIds);

    // 5. Load relevant memories (previous training, mastered skills)
    const privateMemories = await this.loadLearningMemories(personaId, skill);

    return { domain: 'academy', contextId, personaId, identity, conversationHistory, artifacts, privateMemories, metadata };
  }
}
```

**Academy Token Strategy**: Priority-based with compression
- **Highest Priority**: Current learning objectives (always included)
- **High Priority**: Current examples/exercises being worked on
- **Medium Priority**: Recent dialogue with teacher
- **Low Priority**: Historical attempts (compressed summaries only)
- **Compression**: Use genome to create compressed skill summaries

#### 🎮 Game RAG Builder (NOT YET IMPLEMENTED)
```typescript
class GameRAGBuilder extends RAGBuilder {
  // Context Source: Current game state + recent moves + game rules
  // Token Strategy: State-focused (current state > history > rules reference)
  // Artifacts: Game board screenshots, move history visualizations
  // Memories: Previous games, strategies learned, opponent patterns
  // Identity: "You are playing {game} as {role} with goal: {objective}"

  async buildContext(contextId: UUID, personaId: UUID, options?: RAGBuildOptions): Promise<RAGContext> {
    // contextId = gameSessionId

    // 1. Load current game state
    const gameState = await this.loadGameState(contextId);
    const game = gameState.gameName;

    // 2. Build identity as player
    const identity = {
      name: personaId,
      systemPrompt: `You are playing ${game}. Current objective: ${gameState.currentObjective}. Be strategic and collaborative.`,
      role: 'player'
    };

    // 3. Load recent moves/actions (last 10 turns)
    const conversationHistory = await this.loadMoveHistory(contextId, 10);

    // 4. Load game artifacts (board screenshots, current state visualization)
    const artifacts = [
      await this.captureGameBoardScreenshot(contextId),
      await this.generateMoveHistoryChart(contextId)
    ];

    // 5. Load strategic memories (previous games, learned strategies)
    const privateMemories = await this.loadGameMemories(personaId, game);

    return { domain: 'game', contextId, personaId, identity, conversationHistory, artifacts, privateMemories, metadata };
  }
}
```

**Game Token Strategy**: State-focused with selective history
- **Highest Priority**: Current game state (board position, available moves)
- **High Priority**: Last N moves (for immediate context)
- **Medium Priority**: Game rules reference (compressed)
- **Low Priority**: Full game history (summarized as "early game advantage: white")
- **Compression**: Strategic summaries instead of full move-by-move

#### 💻 Code RAG Builder (NOT YET IMPLEMENTED)
```typescript
class CodeRAGBuilder extends RAGBuilder {
  // Context Source: File structure + open files + recent changes + conversation
  // Token Strategy: File-focused (open files > imports > recent changes > chat)
  // Artifacts: Code files, diffs, compilation errors, test results
  // Memories: Project patterns, coding style, architectural decisions
  // Identity: "You are coding in {project} on {task} with {user}"

  async buildContext(contextId: UUID, personaId: UUID, options?: RAGBuildOptions): Promise<RAGContext> {
    // contextId = codingSessionId

    // 1. Load coding session state
    const session = await this.loadCodingSession(contextId);
    const project = session.projectName;
    const task = session.currentTask;

    // 2. Build identity as pair programmer
    const identity = {
      name: personaId,
      systemPrompt: `You are pair programming on ${project}. Current task: ${task}. Focus on clean, maintainable code.`,
      role: 'developer'
    };

    // 3. Load conversation history (discussion about code)
    const conversationHistory = await this.loadCodingDialogue(contextId, options?.maxMessages ?? 20);

    // 4. Load code artifacts (open files, recent changes, errors)
    const artifacts = [
      ...(await this.loadOpenFiles(session.openFileIds)),
      ...(await this.loadRecentDiffs(contextId, 5)),
      await this.loadCompilationErrors(contextId),
      await this.loadTestResults(contextId)
    ];

    // 5. Load project memories (patterns, style, architecture)
    const privateMemories = await this.loadProjectMemories(personaId, project);

    return { domain: 'code', contextId, personaId, identity, conversationHistory, artifacts, privateMemories, metadata };
  }
}
```

**Code Token Strategy**: File-focused with intelligent truncation
- **Highest Priority**: Files currently being edited (full content)
- **High Priority**: Files being discussed (full content)
- **Medium Priority**: Imported dependencies (signatures only, not full implementation)
- **Low Priority**: Recent changes (git diffs, summarized)
- **Compression**: Show function signatures instead of full implementations for context files

#### 🌐 Web RAG Builder (NOT YET IMPLEMENTED)
```typescript
class WebRAGBuilder extends RAGBuilder {
  // Context Source: Current webpage + browsing history + conversation
  // Token Strategy: Page-focused (current page > links > history > chat)
  // Artifacts: Page screenshots, extracted text, navigation history
  // Memories: Browsing patterns, research goals, discovered insights
  // Identity: "You are browsing with {user} researching {topic}"

  async buildContext(contextId: UUID, personaId: UUID, options?: RAGBuildOptions): Promise<RAGContext> {
    // contextId = browsingSessionId

    // 1. Load browsing session state
    const session = await this.loadBrowsingSession(contextId);
    const currentUrl = session.currentUrl;
    const researchTopic = session.researchTopic;

    // 2. Build identity as research companion
    const identity = {
      name: personaId,
      systemPrompt: `You are browsing with user researching: ${researchTopic}. Help find relevant information and synthesize insights.`,
      role: 'researcher'
    };

    // 3. Load conversation history (discussion about findings)
    const conversationHistory = await this.loadBrowsingDialogue(contextId, options?.maxMessages ?? 15);

    // 4. Load web artifacts (current page, screenshots, extracted content)
    const artifacts = [
      await this.capturePageScreenshot(currentUrl),
      await this.extractPageText(currentUrl),
      await this.loadNavigationHistory(contextId, 5)
    ];

    // 5. Load research memories (previous findings, insights, patterns)
    const privateMemories = await this.loadResearchMemories(personaId, researchTopic);

    return { domain: 'web', contextId, personaId, identity, conversationHistory, artifacts, privateMemories, metadata };
  }
}
```

**Web Token Strategy**: Page-focused with selective history
- **Highest Priority**: Current webpage content (extracted, cleaned)
- **High Priority**: Links on current page (for navigation decisions)
- **Medium Priority**: Recent pages visited (summaries only)
- **Low Priority**: Full browsing history (excluded, only keep in memories)
- **Compression**: Extract main content, remove boilerplate HTML

### RAG Builder Registration

All RAG builders must be registered with RAGBuilderFactory at system startup:

```typescript
// system/rag/RAGRegistry.ts (NEW FILE)
import { RAGBuilderFactory } from './shared/RAGBuilder';
import { ChatRAGBuilder } from './builders/ChatRAGBuilder';
import { AcademyRAGBuilder } from './builders/AcademyRAGBuilder';
import { GameRAGBuilder } from './builders/GameRAGBuilder';
import { CodeRAGBuilder } from './builders/CodeRAGBuilder';
import { WebRAGBuilder } from './builders/WebRAGBuilder';

export function registerAllRAGBuilders(): void {
  RAGBuilderFactory.register('chat', new ChatRAGBuilder());
  RAGBuilderFactory.register('academy', new AcademyRAGBuilder());
  RAGBuilderFactory.register('game', new GameRAGBuilder());
  RAGBuilderFactory.register('code', new CodeRAGBuilder());
  RAGBuilderFactory.register('web', new WebRAGBuilder());

  console.log('✅ Registered RAG builders for 5 domains');
}
```

---

## ⚙️ THE ACTION SYSTEM: Domain-Specific Execution (2025-10-18)

### The Problem

The cognitive cycle generates **intent** (what the AI wants to do), but executing that intent is domain-specific:
- **Chat**: Send message to room
- **Academy**: Submit answer to training exercise
- **Game**: Make move in game
- **Code**: Edit file, run tests
- **Web**: Navigate to URL, extract data

### The Action Interface

```typescript
// system/cognition/shared/ActionTypes.ts (NEW FILE)

export interface Action {
  domain: RAGDomain;
  type: string;  // Domain-specific action type
  payload: unknown;  // Domain-specific data
  timestamp: number;
  actorId: UUID;  // PersonaUser who generated this action
}

// Domain-specific action types
export interface ChatAction extends Action {
  domain: 'chat';
  type: 'send_message' | 'react' | 'edit_message' | 'delete_message';
  payload: {
    roomId: UUID;
    content?: string;
    messageId?: UUID;
    reaction?: string;
  };
}

export interface AcademyAction extends Action {
  domain: 'academy';
  type: 'submit_answer' | 'ask_question' | 'request_hint' | 'complete_exercise';
  payload: {
    sessionId: UUID;
    exerciseId: UUID;
    answer?: string;
    question?: string;
  };
}

export interface GameAction extends Action {
  domain: 'game';
  type: 'make_move' | 'suggest_strategy' | 'request_undo' | 'resign';
  payload: {
    gameId: UUID;
    move?: string;  // Game-specific notation
    suggestion?: string;
  };
}

export interface CodeAction extends Action {
  domain: 'code';
  type: 'edit_file' | 'run_tests' | 'commit_changes' | 'suggest_refactor';
  payload: {
    sessionId: UUID;
    fileId?: UUID;
    filePath?: string;
    changes?: string;  // Diff format
    commitMessage?: string;
    suggestion?: string;
  };
}

export interface WebAction extends Action {
  domain: 'web';
  type: 'navigate' | 'extract_data' | 'bookmark' | 'synthesize_insight';
  payload: {
    sessionId: UUID;
    url?: string;
    selector?: string;
    bookmark?: { url: string; title: string; tags: string[] };
    insight?: string;
  };
}
```

### The ActionExecutor Pattern

```typescript
// system/cognition/shared/ActionExecutor.ts (NEW FILE)

export abstract class ActionExecutor {
  abstract readonly domain: RAGDomain;

  abstract execute(action: Action): Promise<ActionResult>;
}

export interface ActionResult {
  success: boolean;
  outcome?: unknown;  // Domain-specific result
  error?: string;
}

// Factory pattern (like RAGBuilderFactory)
export class ActionExecutorFactory {
  private static executors: Map<RAGDomain, ActionExecutor> = new Map();

  static register(domain: RAGDomain, executor: ActionExecutor): void {
    this.executors.set(domain, executor);
  }

  static getExecutor(domain: RAGDomain): ActionExecutor {
    const executor = this.executors.get(domain);
    if (!executor) {
      throw new Error(`No ActionExecutor registered for domain: ${domain}`);
    }
    return executor;
  }

  static async execute(action: Action): Promise<ActionResult> {
    const executor = this.getExecutor(action.domain);
    return executor.execute(action);
  }
}
```

### Domain-Specific Executors

#### Chat Action Executor
```typescript
// system/cognition/executors/ChatActionExecutor.ts (NEW FILE)

export class ChatActionExecutor extends ActionExecutor {
  readonly domain: RAGDomain = 'chat';

  async execute(action: Action): Promise<ActionResult> {
    const chatAction = action as ChatAction;

    switch (chatAction.type) {
      case 'send_message':
        return this.sendMessage(chatAction);
      case 'react':
        return this.addReaction(chatAction);
      case 'edit_message':
        return this.editMessage(chatAction);
      case 'delete_message':
        return this.deleteMessage(chatAction);
      default:
        throw new Error(`Unknown chat action type: ${chatAction.type}`);
    }
  }

  private async sendMessage(action: ChatAction): Promise<ActionResult> {
    const { roomId, content } = action.payload;

    // Use existing chat message system
    const messageEntity = await ChatMessageEntity.create({
      roomId,
      senderId: action.actorId,
      content: content!,
      timestamp: Date.now()
    });

    // Store in database
    await messageEntity.save();

    // Emit real-time event
    EventBus.emit('chat:message-received', { message: messageEntity });

    return { success: true, outcome: messageEntity };
  }

  // ... other methods
}
```

#### Academy Action Executor
```typescript
// system/cognition/executors/AcademyActionExecutor.ts (NEW FILE)

export class AcademyActionExecutor extends ActionExecutor {
  readonly domain: RAGDomain = 'academy';

  async execute(action: Action): Promise<ActionResult> {
    const academyAction = action as AcademyAction;

    switch (academyAction.type) {
      case 'submit_answer':
        return this.submitAnswer(academyAction);
      case 'ask_question':
        return this.askQuestion(academyAction);
      case 'request_hint':
        return this.requestHint(academyAction);
      case 'complete_exercise':
        return this.completeExercise(academyAction);
      default:
        throw new Error(`Unknown academy action type: ${academyAction.type}`);
    }
  }

  private async submitAnswer(action: AcademyAction): Promise<ActionResult> {
    const { sessionId, exerciseId, answer } = action.payload;

    // Load exercise and check answer
    const exercise = await TrainingExerciseEntity.findById(exerciseId);
    const isCorrect = await exercise.checkAnswer(answer!);

    // Record attempt in training session
    const attempt = await TrainingAttemptEntity.create({
      sessionId,
      exerciseId,
      personaId: action.actorId,
      answer: answer!,
      isCorrect,
      timestamp: Date.now()
    });

    await attempt.save();

    // Update persona's learning progress (genome modification if LoRA enabled)
    if (isCorrect) {
      await this.updateLearningProgress(action.actorId, exerciseId, 'mastered');
    }

    // Emit training event
    EventBus.emit('academy:answer-submitted', { attempt });

    return { success: true, outcome: { isCorrect, feedback: exercise.feedback } };
  }

  // ... other methods
}
```

#### Game Action Executor (Conceptual)
```typescript
// system/cognition/executors/GameActionExecutor.ts (NOT YET IMPLEMENTED)

export class GameActionExecutor extends ActionExecutor {
  readonly domain: RAGDomain = 'game';

  async execute(action: Action): Promise<ActionResult> {
    const gameAction = action as GameAction;

    switch (gameAction.type) {
      case 'make_move':
        return this.makeMove(gameAction);
      case 'suggest_strategy':
        return this.suggestStrategy(gameAction);
      // ... etc
    }
  }

  private async makeMove(action: GameAction): Promise<ActionResult> {
    const { gameId, move } = action.payload;

    // Load game state
    const game = await GameSessionEntity.findById(gameId);

    // Validate move legality
    const isLegal = await game.validateMove(move!, action.actorId);
    if (!isLegal) {
      return { success: false, error: 'Illegal move' };
    }

    // Execute move
    await game.applyMove(move!, action.actorId);
    await game.save();

    // Check win conditions
    const gameOver = await game.checkWinConditions();

    // Emit game event
    EventBus.emit('game:move-made', { gameId, move, playerId: action.actorId, gameOver });

    return { success: true, outcome: { gameOver, newState: game.state } };
  }
}
```

### Registering All Executors

```typescript
// system/cognition/ActionRegistry.ts (NEW FILE)

import { ActionExecutorFactory } from './shared/ActionExecutor';
import { ChatActionExecutor } from './executors/ChatActionExecutor';
import { AcademyActionExecutor } from './executors/AcademyActionExecutor';
import { GameActionExecutor } from './executors/GameActionExecutor';
import { CodeActionExecutor } from './executors/CodeActionExecutor';
import { WebActionExecutor } from './executors/WebActionExecutor';

export function registerAllActionExecutors(): void {
  ActionExecutorFactory.register('chat', new ChatActionExecutor());
  ActionExecutorFactory.register('academy', new AcademyActionExecutor());
  ActionExecutorFactory.register('game', new GameActionExecutor());
  ActionExecutorFactory.register('code', new CodeActionExecutor());
  ActionExecutorFactory.register('web', new WebActionExecutor());

  console.log('✅ Registered ActionExecutors for 5 domains');
}
```

---

## 🔄 MIGRATION STRATEGY: Don't Break Chat (2025-10-18)

### The Golden Rule

**NEVER break existing functionality while refactoring.** Chat must continue working at every commit.

### Phase-by-Phase Migration

#### Phase 1: Create New Types (No Behavior Change)
**Goal**: Add universal types alongside existing chat-specific code

**Files to Create**:
- `system/cognition/shared/CognitionTypes.ts` - CognitiveEvent, StateChange interfaces
- `system/cognition/shared/ActionTypes.ts` - Action interfaces for all domains
- `system/cognition/shared/ActionExecutor.ts` - ActionExecutor base class and factory

**Testing**: Run `npx tsc --noEmit` - must compile with zero errors

**Commit**: "Add universal cognition types (no behavior change)"

#### Phase 2: Add process() Method (Delegates to Existing Code)
**Goal**: Add universal entry point that delegates to existing handleChatMessage

**File to Modify**: `system/user/server/PersonaUser.ts`

**Changes**:
```typescript
// ADD new method (don't modify handleChatMessage yet!)
async process(event: CognitiveEvent): Promise<StateChange> {
  console.log(`🧠 PersonaUser.process() domain=${event.domain} context=${event.contextId.slice(0, 8)}`);

  switch (event.domain) {
    case 'chat':
      // Delegate to existing chat handler
      const chatMessage = event.trigger as ChatMessageEntity;
      await this.handleChatMessage(chatMessage);
      return { success: true };

    default:
      throw new Error(`Domain not yet implemented: ${event.domain}`);
  }
}
```

**Testing**:
1. `npx tsc --noEmit` - verify compilation
2. `npm start` - deploy system
3. `./jtag ping` - verify 64 commands registered
4. Send test message in chat, verify AIs still respond
5. Check logs for "🧠 PersonaUser.process()" messages

**Commit**: "Add PersonaUser.process() delegating to existing handlers"

#### Phase 3: Abstract RAGBuilder Calls (Still Only Chat)
**Goal**: Replace hard-coded `new ChatRAGBuilder()` with factory pattern

**File to Modify**: `system/user/server/PersonaUser.ts`

**Changes**:
```typescript
// Lines 700, 1496: Replace hard-coded ChatRAGBuilder
// OLD:
const ragBuilder = new ChatRAGBuilder();

// NEW:
const ragBuilder = RAGBuilderFactory.getBuilder(this.currentDomain);
// this.currentDomain = 'chat' by default (add property to PersonaUser)
```

**Add to PersonaUser class**:
```typescript
export class PersonaUser extends AIUser {
  private currentDomain: RAGDomain = 'chat';  // NEW property
  // ... rest of class
}
```

**Testing**:
1. `npx tsc --noEmit`
2. `npm start`
3. Send test message, verify AIs respond (behavior unchanged)
4. Check logs - should see ChatRAGBuilder still being used

**Commit**: "Use RAGBuilderFactory instead of hard-coded ChatRAGBuilder"

#### Phase 4: Implement ActionExecutor for Chat
**Goal**: Route chat actions through universal action system

**Files to Create**:
- `system/cognition/executors/ChatActionExecutor.ts`
- `system/cognition/ActionRegistry.ts`

**File to Modify**: `system/user/server/PersonaUser.ts`

**Changes in respondToMessage()**:
```typescript
// OLD: Direct database write + event emit
const messageEntity = await ChatMessageEntity.create({...});
await messageEntity.save();
EventBus.emit('chat:message-received', { message: messageEntity });

// NEW: Create action and use executor
const action: ChatAction = {
  domain: 'chat',
  type: 'send_message',
  actorId: this.id,
  payload: { roomId, content: responseText },
  timestamp: Date.now()
};

const result = await ActionExecutorFactory.execute(action);
if (!result.success) {
  throw new Error(`Failed to send message: ${result.error}`);
}
```

**Testing**:
1. `npx tsc --noEmit`
2. `npm start`
3. Send message, verify AIs respond
4. Check logs for "ChatActionExecutor.sendMessage()" messages
5. Verify real-time events still work (message appears in UI)

**Commit**: "Route chat actions through universal ActionExecutor system"

#### Phase 5: Implement Academy Domain (New Functionality)
**Goal**: Add first new domain without touching chat code

**Files to Create**:
- `system/rag/builders/AcademyRAGBuilder.ts`
- `system/cognition/executors/AcademyActionExecutor.ts`
- `database/entities/TrainingSessionEntity.ts`
- `database/entities/TrainingExerciseEntity.ts`
- `database/entities/TrainingAttemptEntity.ts`

**Files to Modify**:
- `system/rag/RAGRegistry.ts` - register AcademyRAGBuilder
- `system/cognition/ActionRegistry.ts` - register AcademyActionExecutor

**File to Modify**: `system/user/server/PersonaUser.ts`

**Changes**:
```typescript
async process(event: CognitiveEvent): Promise<StateChange> {
  switch (event.domain) {
    case 'chat':
      // Existing chat logic (unchanged)
      const chatMessage = event.trigger as ChatMessageEntity;
      await this.handleChatMessage(chatMessage);
      return { success: true };

    case 'academy':  // NEW!
      return this.handleAcademyEvent(event);

    default:
      throw new Error(`Domain not implemented: ${event.domain}`);
  }
}

private async handleAcademyEvent(event: CognitiveEvent): Promise<StateChange> {
  // Build RAG context for training session
  const ragBuilder = RAGBuilderFactory.getBuilder('academy');
  const context = await ragBuilder.buildContext(event.contextId, this.id);

  // Evaluate if should respond (same logic as chat)
  const decision = await this.evaluateShouldRespond(context);
  if (!decision.shouldRespond) {
    return { success: true }; // Silent non-participation
  }

  // Generate action using LLM
  const action = await this.generateAcademyAction(context, event);

  // Execute action
  const result = await ActionExecutorFactory.execute(action);

  return { success: result.success, action, error: result.error };
}
```

**Testing**:
1. Create test training session: `./jtag academy/session/create --skill="TypeScript" --objectives='["Learn interfaces","Understand generics"]'`
2. Start training: `./jtag academy/session/start --sessionId=<ID> --personaId=<HELPER_AI_ID>`
3. Verify academy context is built (check logs)
4. Verify persona responds in academy domain
5. **CRITICAL**: Verify chat still works (send message, AIs respond)

**Commit**: "Implement academy domain (training sessions) - chat unaffected"

### Testing After Each Phase

**Essential Verification**:
```bash
# 1. TypeScript compilation
npx tsc --noEmit

# 2. System deployment
npm start

# 3. Ping check
./jtag ping
# Expect: 64+ commands, 12 daemons, systemReady: true

# 4. Chat functionality (MUST WORK AFTER EVERY PHASE)
# Get room ID
./jtag data/list --collection=rooms --limit=1

# Send test message
./jtag debug/chat-send --roomId="<ID>" --message="Test: verify chat works after phase N"

# Wait 10 seconds, check responses
./jtag debug/logs --filterPattern="AI-RESPONSE|POSTED" --tailLines=20

# 5. Visual verification
./jtag screenshot --querySelector="chat-widget"
```

**If ANY test fails, STOP and fix before continuing to next phase.**

---

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

