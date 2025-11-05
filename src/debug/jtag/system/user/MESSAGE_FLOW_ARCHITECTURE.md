# Message Flow Architecture - Complete Journey

**The complete path from human message to AI response**

---

## High-Level Flow

```
Human types message
        ↓
Chat message created
        ↓
Message history + events collected
        ↓
Protocol Sheriff checks (safety)
        ↓
RoomCoordinator decides (orchestration)
        ↓
Persona receives signal
        ↓
Persona builds context
        ↓
AI Daemon called
        ↓
Adapter routes to LLM
        ↓
Ollama API generates response
        ↓
Response flows back up
        ↓
Message posted to chat
        ↓
Other personas see new message (cycle repeats)
```

---

## Detailed Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    HUMAN INPUT                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
        Joel types: "How do I fix this TypeScript error?"
                           ↓
┌─────────────────────────────────────────────────────────────┐
│                  MESSAGE CREATION                            │
│  • Create ChatMessageEntity                                  │
│  • Assign ID, timestamp, sender                              │
│  • Store in database (chat_messages)                         │
└─────────────────────────────────────────────────────────────┘
                           ↓
                 Emit: chat:message-received
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              CONTEXT GATHERING (Passive)                     │
│  • Message history (last 10-20 messages)                     │
│  • Room participants list                                    │
│  • Persona participation stats                               │
│  • Conversation temperature (hot/warm/cool/cold)             │
└─────────────────────────────────────────────────────────────┘
                           ↓
                 All personas subscribed receive event
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              PROTOCOL SHERIFF (Safety Layer)                 │
│                                                               │
│  Checks (Fast, Deterministic):                               │
│  ✅ Rate limit: Is sender rate-limited?                      │
│  ✅ Permissions: Can sender post here?                       │
│  ✅ Loop detection: Is this part of a loop?                  │
│  ✅ Spam filter: Too many messages?                          │
│                                                               │
│  Decision: SAFE or BLOCK                                     │
└─────────────────────────────────────────────────────────────┘
                           ↓
              IF BLOCKED → Stop here, log violation
              IF SAFE → Continue ↓
                           ↓
┌─────────────────────────────────────────────────────────────┐
│            ROOM COORDINATOR (Orchestration Layer)            │
│                                                               │
│  1. Receives message + context                               │
│  2. Builds RAG context:                                      │
│     • Recent conversation                                    │
│     • Persona expertise areas                                │
│     • Participation ratios                                   │
│     • Past decisions from own DB                             │
│                                                               │
│  3. Calls AI Daemon for decision:                            │
│     "Who should respond to this message?"                    │
│     [Passes context to AI Daemon]                            │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│                 AI DAEMON (Decision)                         │
│                                                               │
│  Request:                                                    │
│  • Adapter: ollama                                           │
│  • Model: llama3.2:1b                                        │
│  • Prompt: "Given context, who should respond?"              │
│  • Temperature: 0.7                                          │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              ADAPTER (Ollama)                                │
│                                                               │
│  Routes to: http://localhost:11434/api/generate             │
│  Sends: Context + Prompt                                     │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              OLLAMA API (Local LLM)                          │
│                                                               │
│  Model: llama3.2:1b (700MB, ~200ms inference)               │
│                                                               │
│  Analysis:                                                   │
│  • Message mentions "TypeScript error"                       │
│  • Helper AI specializes in TypeScript                       │
│  • Teacher AI just responded 3x                              │
│  • CodeReview AI hasn't spoken recently                      │
│                                                               │
│  Decision: "Helper AI should respond (85% confidence)"       │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
              Response flows back through adapter
                           ↓
┌─────────────────────────────────────────────────────────────┐
│          AI DAEMON (Parses Response)                         │
│                                                               │
│  Parses:                                                     │
│  {                                                           │
│    persona: "Helper AI",                                     │
│    confidence: 0.85,                                         │
│    reasoning: "TypeScript expertise match",                  │
│    waitSeconds: 2  // Natural delay                          │
│  }                                                           │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
              Returns to RoomCoordinator
                           ↓
┌─────────────────────────────────────────────────────────────┐
│           ROOM COORDINATOR (Emits Signals)                   │
│                                                               │
│  1. Stores decision in own DB (for training)                 │
│  2. Emits coordination signals:                              │
│                                                               │
│     Emit: persona:respond-signal                             │
│     To: Helper AI                                            │
│     Wait: 2 seconds                                          │
│                                                               │
│     Emit: persona:wait-signal                                │
│     To: Teacher AI, CodeReview AI                            │
│     Reason: "Helper AI is responding"                        │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│         HELPER AI (Receives respond-signal)                  │
│                                                               │
│  1. Wait 2 seconds (natural delay)                           │
│  2. Build response context:                                  │
│     • Original message                                       │
│     • Recent conversation                                    │
│     • Own persona definition                                 │
│     • Available commands                                     │
│                                                               │
│  3. Call AI Daemon for response generation                   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│            AI DAEMON (Response Generation)                   │
│                                                               │
│  Request:                                                    │
│  • Adapter: ollama (or cloud if API key provided)            │
│  • Model: phi-3-mini (local) or claude-3-5-haiku (cloud)     │
│  • Prompt: Persona definition + Context + Question           │
│  • Temperature: 0.8 (more creative for chat)                 │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              ADAPTER (Routes to LLM)                         │
│                                                               │
│  If cloud API key exists:                                    │
│    → Route to Anthropic/OpenAI                               │
│  Else:                                                       │
│    → Route to Ollama (local)                                 │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              OLLAMA/CLOUD LLM                                │
│                                                               │
│  Generates response:                                         │
│  "This error occurs when TypeScript can't infer the type..." │
│                                                               │
│  Optional: Include command                                   │
│  "/jtag debug/logs --tailLines=20"                           │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
              Response + optional command returns
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              HELPER AI (Post-Processing)                     │
│                                                               │
│  1. Receive generated response                               │
│  2. Parse for commands (/jtag...)                            │
│  3. If command found:                                        │
│     • Protocol Sheriff checks permission                     │
│     • Execute command                                        │
│     • Attach result to message                               │
│  4. Create ChatMessageEntity                                 │
│  5. Store in database                                        │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
                 Emit: chat:message-sent
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              MESSAGE POSTED TO CHAT                          │
│                                                               │
│  Helper AI:                                                  │
│  "This error occurs when TypeScript can't infer the type..." │
│                                                               │
│  📎 Attachment: debug-logs-result.txt                        │
│     [20 lines of logs...]                                    │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ↓
              Human sees response in chat
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              CYCLE REPEATS                                   │
│                                                               │
│  • All personas receive chat:message-sent event              │
│  • Protocol Sheriff checks the new message                   │
│  • RoomCoordinator decides if follow-up needed               │
│  • Optionally: Teacher AI adds explanation                   │
│  • Optionally: CodeReview AI suggests fix                    │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Layer Responsibilities (High-Level)

### 1. Message Creation
**Who:** Chat system
**What:** Turn user input into structured message
**Output:** ChatMessageEntity + event

### 2. Context Gathering
**Who:** Event system (passive)
**What:** Collect history, stats, temperature
**Output:** Available to all subscribers

### 3. Safety Enforcement
**Who:** Protocol Sheriff
**What:** Check hard rules (rate limits, permissions, loops)
**Output:** SAFE or BLOCK

### 4. Orchestration
**Who:** RoomCoordinator
**What:** Decide WHO responds WHEN
**Output:** Coordination signals

### 5. Decision Intelligence
**Who:** AI Daemon + Ollama
**What:** Analyze context, make smart decision
**Output:** Persona selection + confidence

### 6. Response Generation
**Who:** Persona + AI Daemon + LLM
**What:** Generate actual chat response
**Output:** Message text + optional commands

### 7. Post-Processing
**Who:** Persona
**What:** Execute commands, attach results
**Output:** Complete message ready to post

### 8. Message Posting
**Who:** Chat system
**What:** Store and broadcast message
**Output:** New message in chat + events

---

## Key Points

### Separation of Concerns
- **Sheriff:** Safety (deterministic, fast)
- **Coordinator:** Intelligence (fuzzy, context-aware)
- **Persona:** Execution (generate + post)
- **AI Daemon:** Adapter layer (pluggable LLMs)

### Two LLM Calls
1. **Coordinator decision** (cheap, fast: llama3.2:1b)
   - "Who should respond?"
   - ~200ms, local, free

2. **Persona response** (quality: phi-3-mini or Claude)
   - "Generate actual response"
   - ~500ms local, or cloud if API key

### Why Two Calls?
- **Efficiency:** One coordinator call decides for ALL personas
- **Cost:** Cheap model for decisions, quality model for responses
- **Speed:** Fast local coordination, optional cloud quality

### Adaptive Quality
- **No API keys:** All local (Ollama)
- **With API keys:** Local coordinator, cloud personas
- **Cost limit:** Auto-downgrade to local

---

## Event Flow

```
User action
  ↓
chat:message-received
  ↓
[Sheriff checks]
  ↓
[Coordinator analysis]
  ↓
persona:respond-signal
persona:wait-signal
  ↓
[Persona generates]
  ↓
chat:message-sent
  ↓
[Cycle repeats]
```

---

## Database Interactions

```
Messages:
• chat_messages (store all messages)

Coordinator:
• coordination_decisions (track who/when/why)
• conversation_stats (participation, temperature)

Sheriff:
• violation_log (track violations)
• threat_detection (suspicious patterns)

Personas:
• command_usage (track command patterns)
• response_history (for training)
```

---

## Timing Examples

### Example 1: Simple Question

```
0ms:    User types "hello"
10ms:   Message created + stored
15ms:   Sheriff checks (pass)
20ms:   Coordinator calls Ollama
220ms:  Decision: Helper AI responds
222ms:  Emit respond-signal
2222ms: Helper AI generates (2sec delay)
2722ms: Response generated (phi-3-mini)
2730ms: Message posted

Total: ~2.7 seconds (feels natural)
```

### Example 2: With Command

```
0ms:    User: "Show logs"
10ms:   Message created
15ms:   Sheriff checks (pass)
20ms:   Coordinator: Helper AI
220ms:  Signal emitted
2220ms: Helper AI generates with command
2720ms: Parse command: /jtag debug/logs
2725ms: Sheriff checks command permission (pass)
2730ms: Execute command
3100ms: Command result (370ms)
3105ms: Attach result to message
3110ms: Post message

Total: ~3.1 seconds
```

---

## Failure Modes & Recovery

### Sheriff Blocks Message
```
Sheriff detects loop
  ↓
Block message
  ↓
Log violation
  ↓
Activate circuit breaker (60s)
  ↓
Notify room: "Loop detected, Helper AI paused"
  ↓
Auto-recover after 60s
```

### Coordinator Can't Decide
```
Ollama timeout
  ↓
Fallback: Simple heuristics
  ↓
"Respond to all humans" rule
  ↓
Continue with degraded intelligence
```

### Persona Generation Fails
```
LLM error
  ↓
Retry with simpler prompt
  ↓
Still fails?
  ↓
Post error message: "I'm having trouble responding, try again?"
```

### Network/API Failure
```
Cloud API down
  ↓
Auto-switch to local Ollama
  ↓
Notify: "Using local model (cloud unavailable)"
  ↓
Continue with local models
```

---

## Related Documents

- **AI_COORDINATION_ARCHITECTURE.md** - RoomCoordinator details
- **PROTOCOL_SHERIFF_ARCHITECTURE.md** - Safety enforcement
- **AI_COMMAND_EXECUTION.md** - Command execution flow
- **README.md** - Master documentation index

---

**This is the complete message journey - every step from human input to AI response! 🚀**
