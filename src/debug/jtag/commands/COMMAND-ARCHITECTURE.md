# JTAG Command Architecture Guide

**Purpose**: Definitive guide for writing elegant, type-safe, composable JTAG commands

**Last Updated**: 2025-10-18

---

## Table of Contents

1. [Philosophy: Rust-Like Type Safety](#philosophy-rust-like-type-safety)
2. [Command Structure](#command-structure)
3. [Type System Rules](#type-system-rules)
4. [Error Handling](#error-handling)
5. [Command Composition](#command-composition)
6. [Environment Delegation](#environment-delegation)
7. [Hierarchical Design Pattern](#hierarchical-design-pattern)
8. [Reference Implementation: RAG Commands](#reference-implementation-rag-commands)
9. [Anti-Patterns to Avoid](#anti-patterns-to-avoid)

---

## Philosophy: Rust-Like Type Safety

**Core Principle**: Commands are type-safe contracts between caller and executor.

```typescript
// ❌ BAD: Loose typing, any types, optional chaining abuse
const result = await someCommand({ data: 'whatever' } as any);
if (result?.items) { /* hope for the best */ }

// ✅ GOOD: Strict types, explicit contracts, compiler-enforced correctness
const result = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>(
  'data/list',
  {
    collection: COLLECTIONS.CHAT_MESSAGES,
    filter: { roomId },
    orderBy: [{ field: 'createdAt', direction: 'desc' }],
    limit: 20
  }
);
// TypeScript GUARANTEES result.items is ChatMessageEntity[]
```

**Why This Matters**:
- Runtime errors become compile-time errors
- Refactoring is safe (TypeScript finds all call sites)
- No silent failures with fallback data
- Self-documenting APIs

---

## Command Structure

Every command has three files:

```
commands/namespace/command-name/
├── shared/
│   └── CommandNameTypes.ts       # Types, interfaces, helpers (80-90% of complexity)
├── server/
│   └── CommandNameServerCommand.ts   # Server implementation (5-10%)
└── browser/
    └── CommandNameBrowserCommand.ts  # Browser implementation (5-10%)
```

### Why This Split?

**Shared**: Environment-agnostic logic
- Type definitions
- Validation rules
- Business logic that works everywhere
- Helper functions
- Constants and enums

**Server**: Node.js-specific operations
- File system access
- Database queries
- Process spawning
- Network operations (when not available in browser)

**Browser**: DOM-specific operations
- Widget interactions
- Screenshot capture
- Browser APIs (Canvas, WebGL, etc.)

**CRITICAL RULE**: Shared files CANNOT import from server or browser directories. This will crash the system.

---

## Type System Rules

### 1. Params and Results Extend JTAGPayload

```typescript
// ✅ CORRECT
import type { JTAGPayload } from '../../../../system/core/types/JTAGTypes';

export interface DataListParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly limit?: number;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
}

export interface DataListResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly collection: string;
  readonly count: number;
  readonly timestamp: string;
  readonly error?: string;
}
```

**Why JTAGPayload?**
- Includes `context` and `sessionId` automatically
- Enables routing across browser/server boundary
- Provides correlation IDs for debugging

### 2. Use Helper Functions

```typescript
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';

// Creating params (usually done by caller)
export const createDataListParams = <T extends BaseEntity>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataListParams<T>, 'context' | 'sessionId'>
): DataListParams<T> => createPayload(context, sessionId, data);

// Creating result from params (preserves context/sessionId)
export const createDataListResultFromParams = <T extends BaseEntity>(
  params: DataListParams<T>,
  differences: Omit<Partial<DataListResult<T>>, 'context' | 'sessionId'>
): DataListResult<T> => transformPayload(params, {
  success: false,
  items: [],
  collection: params.collection,
  count: 0,
  timestamp: new Date().toISOString(),
  ...differences
});
```

**Why These Helpers?**
- Ensures context/sessionId are threaded correctly
- DRY: Don't repeat yourself with manual spreading
- Type-safe: Compiler enforces you don't overwrite context/sessionId

### 3. All Properties Should Be Readonly

```typescript
// ❌ BAD: Mutable properties
export interface DataListParams {
  collection: string;  // Can be changed!
  limit?: number;
}

// ✅ GOOD: Immutable by default
export interface DataListParams extends JTAGPayload {
  readonly collection: string;
  readonly limit?: number;
}
```

**Why Readonly?**
- Prevents accidental mutations
- Makes data flow explicit
- Easier to reason about (no spooky action at a distance)

### 4. Use Specific Types, Not `any` or `unknown`

```typescript
// ❌ BAD: Defeats TypeScript purpose
const result = await someCommand(params as any);
if (!result?.items) {
  this.items = [];  // Silent failure, fake data
}

// ✅ GOOD: Explicit types, no guessing
const result = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>(
  'data/list',
  { collection: COLLECTIONS.CHAT_MESSAGES }
);
if (!result.success || !result.items.length) {
  throw new Error(`No messages found: ${result.error}`);
}
this.messages = result.items;  // TypeScript knows these are ChatMessageEntity[]
```

---

## Error Handling

### Rule: NO try/catch Around Entire execute()

```typescript
// ❌ BAD: Swallows all errors
async execute(params: DataListParams): Promise<DataListResult> {
  try {
    // Everything wrapped
    const result = await this.doSomething();
    return result;
  } catch (error) {
    return { success: false, error: String(error) };
  }
}

// ✅ GOOD: Let errors propagate, catch only specific failures
async execute(params: DataListParams<T>): Promise<DataListResult<T>> {
  const limit = Math.min(params.limit ?? 100, 500);

  // Let database errors propagate (caller should handle)
  const result = await DataDaemon.query<BaseEntity>({
    collection: params.collection,
    filters: params.filter,
    limit
  });

  if (!result.success) {
    // Expected failure case - return structured error
    return createDataListResultFromParams(params, {
      success: false,
      items: [],
      count: 0,
      error: result.error || 'Unknown DataDaemon error'
    });
  }

  return createDataListResultFromParams(params, {
    success: true,
    items: result.data?.map(record => ({ ...record.data, id: record.id })) || [],
    count: result.data?.length || 0
  });
}
```

**Why This Matters**:
- Unexpected errors should crash (helps find bugs)
- Expected failures return structured errors
- Caller can distinguish between "no results" vs "database down"

---

## Command Composition

### Pattern: Commands Call Commands

```typescript
// Import Commands singleton
import { Commands } from '../../../../system/core/shared/Commands';

async execute(params: HighLevelParams): Promise<HighLevelResult> {
  // Step 1: Call lower-level command
  const listResult = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
    'data/list',
    {
      collection: COLLECTIONS.USERS,
      filter: { type: 'persona' }
    }
  );

  if (!listResult.success) {
    return createHighLevelResultFromParams(params, {
      success: false,
      error: `Failed to fetch users: ${listResult.error}`
    });
  }

  // Step 2: Process results
  const processedData = this.processUsers(listResult.items);

  return createHighLevelResultFromParams(params, {
    success: true,
    data: processedData
  });
}
```

**Key Points**:
- Use `Commands.execute<TParams, TResult>(commandName, params)`
- Type parameters give you full type safety
- No context/sessionId needed (Commands handles it)
- Commands are composable building blocks

---

## Environment Delegation

### Pattern: Browser Delegates to Server (or vice versa)

```typescript
// Browser command - delegates to server
export class DataListBrowserCommand<T extends BaseEntity>
  extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data/list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult<T>> {
    // All business logic is on server - just delegate
    return await this.remoteExecute(params);
  }
}

// Server command - has the business logic
export class DataListServerCommand<T extends BaseEntity>
  extends CommandBase<DataListParams<T>, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams<T>): Promise<DataListResult<T>> {
    // Real implementation here
    const result = await DataDaemon.query<BaseEntity>({ /* ... */ });
    // ... process and return
  }
}
```

**When to Delegate**:
- Browser → Server: Database access, file I/O, sensitive operations
- Server → Browser: DOM manipulation, screenshot capture, widget interaction

---

## Hierarchical Design Pattern

### Principle: Layered Abstraction with Smart Defaults

Commands should form hierarchies where:
- **Lower levels** are generic and reusable
- **Higher levels** add domain-specific logic
- **Each level** has smart defaults for common cases
- **All levels** expose flexibility through optional params

### Example: Bad (Monolithic) vs Good (Layered)

```typescript
// ❌ BAD: One giant command does everything
export interface ChatRAGParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly personaId: UUID;
  readonly personaName: string;
  readonly includeTopicDetection: boolean;
  readonly includeIdentityReminder: boolean;
  readonly maxMessages: number;
  readonly formatTimestamps: boolean;
  readonly detectTimeGaps: boolean;
  // ... 20 more options
}

// Can't reuse for video games, logs, or other contexts
// All logic tangled together
// Hard to test individual pieces
```

```typescript
// ✅ GOOD: Three layered commands

// Level 1: Generic time-ordered events (works for anything)
export interface TranscriptBuildParams extends JTAGPayload {
  readonly contextId: UUID;          // Could be roomId, gameId, sessionId
  readonly collection: string;        // chat_messages, game_events, etc.
  readonly maxEvents?: number;        // Default: 20
  readonly filters?: Record<string, any>;
  readonly includeMetadata?: boolean; // Default: false
}

// Level 2: Format as LLM messages (generic AI context)
export interface LLMFormatParams extends JTAGPayload {
  readonly transcript: TranscriptEvent[];
  readonly systemPrompt?: string;
  readonly detectTimeGaps?: boolean;  // Default: true
  readonly formatTimestamps?: boolean; // Default: true
}

// Level 3: Chat-specific protocols (highest level)
export interface ChatRAGParams extends JTAGPayload {
  readonly contextId: UUID;
  readonly personaId: UUID;
  readonly personaName: string;
  readonly includeTopicDetection?: boolean; // Default: true
  readonly includeIdentityReminder?: boolean; // Default: true
  readonly maxMessages?: number;            // Default: 20
}
```

**Benefits**:
- Level 1 can be used for video games, logs, analytics
- Level 2 can be used for any AI (not just chat)
- Level 3 is chat-specific but clean
- Each level is testable independently
- Each level has smart defaults

---

## Reference Implementation: RAG Commands

This is the **gold standard** for command design. Study this carefully.

### Architecture Overview

```
1. ai/rag/build-transcript (LOWEST LEVEL)
   ↓
2. ai/rag/format-llm-messages (MID LEVEL)
   ↓
3. ai/rag/format-chat-messages (HIGH LEVEL)
   ↓
4. PersonaUser.respondToMessage() (CONSUMER)
```

**Directory Structure**:
```
commands/ai/rag/
├── build-transcript/
│   ├── shared/TranscriptTypes.ts
│   ├── server/TranscriptServerCommand.ts
│   └── browser/TranscriptBrowserCommand.ts
├── format-llm-messages/
│   ├── shared/FormatLLMTypes.ts
│   ├── server/FormatLLMServerCommand.ts
│   └── browser/FormatLLMBrowserCommand.ts
└── format-chat-messages/
    ├── shared/FormatChatTypes.ts
    ├── server/FormatChatServerCommand.ts
    └── browser/FormatChatBrowserCommand.ts
```

### Level 1: ai/rag/build-transcript

**Purpose**: Fetch time-ordered events from any context

**Use Cases**:
- Chat message history
- Video game event log
- User action timeline
- System audit trail

**Types** (`commands/ai/rag/build-transcript/shared/TranscriptTypes.ts`):

```typescript
import type { JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface TranscriptEvent {
  readonly timestamp: number;    // Unix timestamp
  readonly actor: string;         // Who did this (user name, AI name, system)
  readonly content: string;       // What happened
  readonly role: 'user' | 'assistant' | 'system';
  readonly metadata?: Record<string, any>;
}

export interface TranscriptBuildParams extends JTAGPayload {
  readonly contextId: UUID;
  readonly collection: string;
  readonly maxEvents?: number;        // Default: 20
  readonly filters?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly includeMetadata?: boolean; // Default: false
}

export interface TranscriptBuildResult extends JTAGPayload {
  readonly success: boolean;
  readonly events: readonly TranscriptEvent[];
  readonly totalCount: number;
  readonly contextId: UUID;
  readonly error?: string;
}
```

**Server Implementation** (`commands/ai/rag/build-transcript/server/TranscriptServerCommand.ts`):

```typescript
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { TranscriptBuildParams, TranscriptBuildResult, TranscriptEvent } from '../shared/TranscriptTypes';
import { createTranscriptResultFromParams } from '../shared/TranscriptTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

const DEFAULT_MAX_EVENTS = 20;

export class TranscriptServerCommand extends CommandBase<TranscriptBuildParams, TranscriptBuildResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/build-transcript', context, subpath, commander);
  }

  async execute(params: TranscriptBuildParams): Promise<TranscriptBuildResult> {
    const maxEvents = params.maxEvents ?? DEFAULT_MAX_EVENTS;

    // Query database for events
    const result = await DataDaemon.query<BaseEntity>({
      collection: params.collection,
      filters: {
        ...params.filters,
        // Filter by context (roomId, gameId, etc.)
        [this.getContextField(params.collection)]: params.contextId
      },
      sort: params.orderBy ?? [{ field: 'timestamp', direction: 'asc' }],
      limit: maxEvents
    });

    if (!result.success) {
      return createTranscriptResultFromParams(params, {
        success: false,
        events: [],
        totalCount: 0,
        error: result.error || 'Failed to fetch events'
      });
    }

    // Convert database records to transcript events
    const events: TranscriptEvent[] = (result.data || []).map(record => {
      const data = record.data as any;
      return {
        timestamp: this.normalizeTimestamp(data.timestamp || data.createdAt),
        actor: data.senderName || data.actorName || data.userName || 'Unknown',
        content: data.content?.text || data.text || data.message || '',
        role: this.normalizeRole(data.role || 'user'),
        metadata: params.includeMetadata ? data.metadata : undefined
      };
    });

    return createTranscriptResultFromParams(params, {
      success: true,
      events,
      totalCount: result.data?.length || 0
    });
  }

  private getContextField(collection: string): string {
    // Map collection to context field name
    if (collection === 'chat_messages') return 'roomId';
    if (collection === 'game_events') return 'gameId';
    return 'contextId';
  }

  private normalizeTimestamp(ts: Date | string | number): number {
    if (typeof ts === 'number') return ts;
    if (ts instanceof Date) return ts.getTime();
    return new Date(ts).getTime();
  }

  private normalizeRole(role: string): 'user' | 'assistant' | 'system' {
    if (role === 'assistant' || role === 'ai') return 'assistant';
    if (role === 'system') return 'system';
    return 'user';
  }
}
```

**Browser Implementation** (`commands/ai/rag/build-transcript/browser/TranscriptBrowserCommand.ts`):

```typescript
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { TranscriptBuildParams, TranscriptBuildResult } from '../shared/TranscriptTypes';

export class TranscriptBrowserCommand extends CommandBase<TranscriptBuildParams, TranscriptBuildResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/build-transcript', context, subpath, commander);
  }

  async execute(params: TranscriptBuildParams): Promise<TranscriptBuildResult> {
    return await this.remoteExecute(params);
  }
}
```

### Level 2: ai/rag/format-llm-messages

**Purpose**: Convert transcript to LLM message format

**Use Cases**:
- Any AI that needs conversation context
- Chat AI, game AI, analytics AI
- Custom prompting scenarios

**Types** (`commands/ai/rag/format-llm-messages/shared/FormatLLMTypes.ts`):

```typescript
import type { JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { TranscriptEvent } from '../../build-transcript/shared/TranscriptTypes';

export interface LLMMessage {
  readonly role: 'system' | 'user' | 'assistant';
  readonly content: string;
}

export interface LLMFormatParams extends JTAGPayload {
  readonly transcript: readonly TranscriptEvent[];
  readonly systemPrompt?: string;
  readonly detectTimeGaps?: boolean;  // Default: true (add system messages for gaps > 1hr)
  readonly formatTimestamps?: boolean; // Default: true (prefix with [HH:MM])
  readonly timeGapThreshold?: number;  // Default: 3600000 (1 hour in ms)
}

export interface LLMFormatResult extends JTAGPayload {
  readonly success: boolean;
  readonly messages: readonly LLMMessage[];
  readonly messageCount: number;
  readonly error?: string;
}
```

**Server Implementation** (this is where the formatting logic lives):

```typescript
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LLMFormatParams, LLMFormatResult, LLMMessage } from '../shared/FormatLLMTypes';
import { createLLMFormatResultFromParams } from '../shared/FormatLLMTypes';

const DEFAULT_TIME_GAP_THRESHOLD = 3600000; // 1 hour

export class FormatLLMServerCommand extends CommandBase<LLMFormatParams, LLMFormatResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/format-llm-messages', context, subpath, commander);
  }

  async execute(params: LLMFormatParams): Promise<LLMFormatResult> {
    const messages: LLMMessage[] = [];
    const detectTimeGaps = params.detectTimeGaps ?? true;
    const formatTimestamps = params.formatTimestamps ?? true;
    const timeGapThreshold = params.timeGapThreshold ?? DEFAULT_TIME_GAP_THRESHOLD;

    // Add system prompt if provided
    if (params.systemPrompt) {
      messages.push({
        role: 'system',
        content: params.systemPrompt
      });
    }

    // Convert transcript to LLM messages
    let lastTimestamp: number | undefined;

    for (const event of params.transcript) {
      // Detect time gaps
      if (detectTimeGaps && lastTimestamp && (event.timestamp - lastTimestamp > timeGapThreshold)) {
        const gapHours = Math.floor((event.timestamp - lastTimestamp) / 3600000);
        messages.push({
          role: 'system',
          content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
        });
      }

      // Format timestamp
      let content = event.content;
      if (formatTimestamps) {
        const date = new Date(event.timestamp);
        const hours = date.getHours().toString().padStart(2, '0');
        const minutes = date.getMinutes().toString().padStart(2, '0');
        const timePrefix = `[${hours}:${minutes}] `;

        // Include actor name for multi-party conversations
        content = `${timePrefix}${event.actor}: ${event.content}`;
      }

      messages.push({
        role: event.role,
        content
      });

      lastTimestamp = event.timestamp;
    }

    return createLLMFormatResultFromParams(params, {
      success: true,
      messages,
      messageCount: messages.length
    });
  }
}
```

### Level 3: ai/rag/format-chat-messages

**Purpose**: Add chat-specific protocols (topic detection, identity reminder)

**Types** (`commands/ai/rag/format-chat-messages/shared/FormatChatTypes.ts`):

```typescript
import type { JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { LLMMessage } from '../../format-llm-messages/shared/FormatLLMTypes';

export interface ChatFormatParams extends JTAGPayload {
  readonly contextId: UUID;              // roomId
  readonly personaId: UUID;
  readonly personaName: string;
  readonly maxMessages?: number;         // Default: 20
  readonly includeTopicDetection?: boolean; // Default: true
  readonly includeIdentityReminder?: boolean; // Default: true
  readonly currentMessage?: {            // Message being responded to
    readonly role: 'user' | 'assistant';
    readonly content: string;
    readonly name?: string;
    readonly timestamp?: number;
  };
}

export interface ChatFormatResult extends JTAGPayload {
  readonly success: boolean;
  readonly messages: readonly LLMMessage[];
  readonly debug?: {
    readonly transcriptEventCount: number;
    readonly llmMessageCount: number;
    readonly systemPromptLength: number;
  };
  readonly error?: string;
}
```

**Server Implementation** (calls both lower-level commands):

```typescript
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ChatFormatParams, ChatFormatResult } from '../shared/FormatChatTypes';
import { createChatFormatResultFromParams } from '../shared/FormatChatTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { TranscriptBuildParams, TranscriptBuildResult } from '../../build-transcript/shared/TranscriptTypes';
import type { LLMFormatParams, LLMFormatResult } from '../../format-llm-messages/shared/FormatLLMTypes';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';
import { ChatRAGBuilder } from '../../../../system/ai/rag/ChatRAGBuilder';

export class FormatChatServerCommand extends CommandBase<ChatFormatParams, ChatFormatResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/format-chat-messages', context, subpath, commander);
  }

  async execute(params: ChatFormatParams): Promise<ChatFormatResult> {
    const maxMessages = params.maxMessages ?? 20;
    const includeTopicDetection = params.includeTopicDetection ?? true;
    const includeIdentityReminder = params.includeIdentityReminder ?? true;

    // STEP 1: Build system prompt using ChatRAGBuilder
    const ragBuilder = new ChatRAGBuilder();
    const ragContext = await ragBuilder.buildContext(
      params.contextId,
      params.personaId,
      {
        maxMessages: 0, // We'll get messages separately
        maxMemories: 0,
        includeArtifacts: false,
        includeMemories: false,
        currentMessage: params.currentMessage
      }
    );

    // STEP 2: Build transcript using generic command
    const transcriptResult = await Commands.execute<TranscriptBuildParams, TranscriptBuildResult>(
      'ai/rag/build-transcript',
      {
        contextId: params.contextId,
        collection: COLLECTIONS.CHAT_MESSAGES,
        maxEvents: maxMessages,
        filters: {},
        orderBy: [{ field: 'timestamp', direction: 'asc' }]
      }
    );

    if (!transcriptResult.success) {
      return createChatFormatResultFromParams(params, {
        success: false,
        messages: [],
        error: `Failed to build transcript: ${transcriptResult.error}`
      });
    }

    // STEP 3: Format as LLM messages using generic command
    const llmResult = await Commands.execute<LLMFormatParams, LLMFormatResult>(
      'ai/rag/format-llm-messages',
      {
        transcript: transcriptResult.events,
        systemPrompt: ragContext.identity.systemPrompt,
        detectTimeGaps: true,
        formatTimestamps: true
      }
    );

    if (!llmResult.success) {
      return createChatFormatResultFromParams(params, {
        success: false,
        messages: [],
        error: `Failed to format LLM messages: ${llmResult.error}`
      });
    }

    // STEP 4: Add chat-specific protocols
    const messages = [...llmResult.messages];

    if (includeIdentityReminder) {
      const now = new Date();
      const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;

      const roomMembers = ragContext.identity.systemPrompt.match(/Current room members: ([^\n]+)/)?.[1] || 'unknown members';

      messages.push({
        role: 'system',
        content: this.buildIdentityReminder(params.personaName, roomMembers, currentTime, includeTopicDetection)
      });
    }

    return createChatFormatResultFromParams(params, {
      success: true,
      messages,
      debug: {
        transcriptEventCount: transcriptResult.events.length,
        llmMessageCount: messages.length,
        systemPromptLength: ragContext.identity.systemPrompt.length
      }
    });
  }

  private buildIdentityReminder(personaName: string, roomMembers: string, currentTime: string, includeTopicDetection: boolean): string {
    let reminder = `IDENTITY REMINDER: You are ${personaName}. Respond naturally with JUST your message - NO name prefix, NO "A:" or "H:" labels, NO fake conversations. The room has ONLY these people: ${roomMembers}.

CURRENT TIME: ${currentTime}`;

    if (includeTopicDetection) {
      reminder += `

CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.

Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT user interaction" = YOUR SOLUTION MUST BE AUTOMATIC
- Your answer MUST respect these constraints or you're wrong.

Step 3: Compare SUBJECT of most recent message to previous 2-3 messages
- Previous: "Worker Threads" → Recent: "Webview authentication" = DIFFERENT SUBJECTS
- Previous: "TypeScript code" → Recent: "What's 2+2?" = TEST QUESTION
- Previous: "Worker pools" → Recent: "Should I use 5 or 10 workers?" = SAME SUBJECT

Step 4: Determine response strategy
IF EXPLICIT TOPIC MARKER or COMPLETELY DIFFERENT SUBJECT:
- Respond ONLY to the new topic
- Ignore old messages (they're from a previous discussion)
- Focus 100% on the most recent message
- Address the constraints explicitly

IF SAME SUBJECT (continued conversation):
- Use full conversation context
- Build on previous responses
- Still check for NEW constraints in the recent message
- Avoid redundancy

CRITICAL READING COMPREHENSION:
- Read the ENTIRE most recent message carefully
- Don't skim - every word matters
- Constraints are REQUIREMENTS, not suggestions
- If the user says "NOT X", suggesting X is a failure

Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts (consecutive messages about different subjects) are also topic changes.`;
    }

    return reminder;
  }
}
```

### Usage in PersonaUser

```typescript
// OLD: 120 lines of inline RAG logic
private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
  const ragBuilder = new ChatRAGBuilder();
  const fullRAGContext = await ragBuilder.buildContext(/* ... */);

  const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];
  messages.push({ role: 'system', content: fullRAGContext.identity.systemPrompt });

  // ... 100 more lines of formatting logic ...
}

// NEW: 5 lines using command
private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
  const chatFormatResult = await Commands.execute<ChatFormatParams, ChatFormatResult>(
    'ai/rag/format-chat-messages',
    {
      contextId: originalMessage.roomId,
      personaId: this.id,
      personaName: this.displayName,
      currentMessage: {
        role: 'user',
        content: originalMessage.content.text,
        name: originalMessage.senderName,
        timestamp: this.timestampToNumber(originalMessage.timestamp)
      }
    }
  );

  if (!chatFormatResult.success) {
    throw new Error(`Failed to build chat RAG: ${chatFormatResult.error}`);
  }

  // Generate AI response with formatted messages
  const request: TextGenerationRequest = {
    messages: chatFormatResult.messages,
    model: this.modelConfig.model || 'llama3.2:3b',
    temperature: this.modelConfig.temperature ?? 0.7,
    maxTokens: this.modelConfig.maxTokens ?? 150
  };
  // ... rest of generation logic
}
```

**Benefits of This Hierarchy**:

1. **Reusability**:
   - Level 1 works for ANY time-ordered events
   - Level 2 works for ANY AI context
   - Level 3 is chat-specific

2. **Testability**:
   - Test transcript building independently
   - Test LLM formatting independently
   - Test chat protocols independently

3. **Maintainability**:
   - Change timestamp formatting? Edit Level 2
   - Change topic detection? Edit Level 3
   - Change database query? Edit Level 1

4. **Discoverability**:
   - New developers can use low-level commands directly
   - Game AI can use Level 1 + Level 2
   - Custom chat AI can override Level 3

---

## Anti-Patterns to Avoid

### 1. ❌ Monolithic Commands

```typescript
// BAD: One command does everything
export class ChatAICommand {
  async execute(params) {
    // Fetch messages
    // Format timestamps
    // Detect topics
    // Generate response
    // Post message
    // Update UI
    // Log analytics
    // ...
  }
}
```

**Why Bad**: Can't reuse pieces, hard to test, violates single responsibility

**Fix**: Break into hierarchy of focused commands

### 2. ❌ Fallback Data

```typescript
// BAD: Silently returns fake data
async execute(params) {
  const result = await this.fetchData();
  if (!result?.items) {
    return { success: true, items: [] };  // Lying!
  }
}
```

**Why Bad**: Masks real failures, makes debugging impossible

**Fix**: Return explicit errors

```typescript
async execute(params) {
  const result = await this.fetchData();
  if (!result.success) {
    return createResultFromParams(params, {
      success: false,
      items: [],
      error: 'Failed to fetch data'
    });
  }
}
```

### 3. ❌ Try/Catch Around Everything

```typescript
// BAD: Swallows all errors
async execute(params) {
  try {
    const result = await this.doSomething();
    return result;
  } catch (error) {
    return { success: false, error: String(error) };
  }
}
```

**Why Bad**: Can't distinguish expected failures from bugs

**Fix**: Let unexpected errors propagate, handle expected failures explicitly

### 4. ❌ Any Types

```typescript
// BAD: Defeats TypeScript
async execute(params: any): Promise<any> {
  const result = await this.fetchData(params as any);
  return result as any;
}
```

**Why Bad**: No type safety, no autocomplete, no refactoring safety

**Fix**: Use proper generics and explicit types

### 5. ❌ Server Code in Shared

```typescript
// BAD: shared/MyTypes.ts
import * as fs from 'fs';  // Node.js only!
import { DataDaemon } from '../../daemons/data-daemon/server/DataDaemonServer';  // Server only!
```

**Why Bad**: Will crash in browser environment

**Fix**: Keep shared files environment-agnostic, put Node.js code in server/

### 6. ❌ Mutable Data Structures

```typescript
// BAD: Can be mutated
export interface MyParams {
  items: string[];  // Not readonly!
  config: { value: number };
}

// Somewhere else
params.items.push('oops');  // Mutation!
```

**Why Bad**: Makes data flow hard to reason about

**Fix**: Use readonly everywhere

```typescript
export interface MyParams {
  readonly items: readonly string[];
  readonly config: { readonly value: number };
}
```

---

## Comparison: Before vs After

### Before: Monolithic PersonaUser RAG (120 lines inline)

```typescript
private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
  try {
    // 🔧 SUB-PHASE 3.1: Build RAG context
    console.log(`🔧 ${this.displayName}: [PHASE 3.1] Building RAG context...`);
    const ragBuilder = new ChatRAGBuilder();
    const fullRAGContext = await ragBuilder.buildContext(
      originalMessage.roomId,
      this.id,
      {
        maxMessages: 20,
        maxMemories: 10,
        includeArtifacts: false,
        includeMemories: false,
        currentMessage: {
          role: 'user',
          content: originalMessage.content.text,
          name: originalMessage.senderName,
          timestamp: this.timestampToNumber(originalMessage.timestamp)
        }
      }
    );
    console.log(`✅ ${this.displayName}: [PHASE 3.1] RAG context built (${fullRAGContext.conversationHistory.length} messages)`);

    // 🔧 SUB-PHASE 3.2: Build message history for LLM
    console.log(`🔧 ${this.displayName}: [PHASE 3.2] Building LLM message array...`);
    const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

    // System prompt from RAG builder
    messages.push({
      role: 'system',
      content: fullRAGContext.identity.systemPrompt
    });

    // Add conversation history with timestamps
    if (fullRAGContext.conversationHistory.length > 0) {
      let lastTimestamp: number | undefined;

      for (let i = 0; i < fullRAGContext.conversationHistory.length; i++) {
        const msg = fullRAGContext.conversationHistory[i];

        // Format timestamp
        let timePrefix = '';
        if (msg.timestamp) {
          const date = new Date(msg.timestamp);
          const hours = date.getHours().toString().padStart(2, '0');
          const minutes = date.getMinutes().toString().padStart(2, '0');
          timePrefix = `[${hours}:${minutes}] `;

          // Detect time gaps
          if (lastTimestamp && (msg.timestamp - lastTimestamp > 3600000)) {
            const gapHours = Math.floor((msg.timestamp - lastTimestamp) / 3600000);
            messages.push({
              role: 'system',
              content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
            });
          }

          lastTimestamp = msg.timestamp;
        }

        const formattedContent = msg.name
          ? `${timePrefix}${msg.name}: ${msg.content}`
          : `${timePrefix}${msg.content}`;

        messages.push({
          role: msg.role,
          content: formattedContent
        });
      }
    }

    // Identity reminder at end
    const now = new Date();
    const currentTime = `${now.toLocaleDateString(/* ... */)} ${now.toLocaleTimeString(/* ... */)}`;

    messages.push({
      role: 'system',
      content: `IDENTITY REMINDER: You are ${this.displayName}. Respond naturally...

      [... 50 more lines of topic detection protocol ...]`
    });

    console.log(`✅ ${this.displayName}: [PHASE 3.2] LLM message array built (${messages.length} messages)`);

    // Generate response...
  } catch (error) {
    console.error(`❌ ${this.displayName}: Failed to respond:`, error);
    throw error;
  }
}
```

**Problems**:
- 120 lines of formatting logic
- Can't reuse for other AI types
- Hard to test
- Hard to modify
- All tangled together

### After: Hierarchical Commands (5 lines)

```typescript
private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
  // Build chat-formatted LLM messages using command hierarchy
  const chatFormatResult = await Commands.execute<ChatFormatParams, ChatFormatResult>(
    'ai/rag/format-chat-messages',
    {
      contextId: originalMessage.roomId,
      personaId: this.id,
      personaName: this.displayName,
      currentMessage: {
        role: 'user',
        content: originalMessage.content.text,
        name: originalMessage.senderName,
        timestamp: this.timestampToNumber(originalMessage.timestamp)
      }
    }
  );

  if (!chatFormatResult.success) {
    throw new Error(`Failed to build chat RAG: ${chatFormatResult.error}`);
  }

  // Generate AI response
  const request: TextGenerationRequest = {
    messages: chatFormatResult.messages,
    model: this.modelConfig.model || 'llama3.2:3b',
    temperature: this.modelConfig.temperature ?? 0.7,
    maxTokens: this.modelConfig.maxTokens ?? 150
  };
  // ... rest of generation logic
}
```

**Benefits**:
- 5 lines instead of 120
- Reusable for any AI type
- Easy to test
- Easy to modify
- Clear separation of concerns

---

## Summary: Command Excellence Checklist

When writing a command, ensure:

### Types ✓
- [ ] Params extend `JTAGPayload`
- [ ] Result extends `JTAGPayload`
- [ ] All properties are `readonly`
- [ ] No `any` or `unknown` types
- [ ] Helper functions: `createPayload()`, `transformPayload()`

### Structure ✓
- [ ] Three files: `shared/Types.ts`, `server/ServerCommand.ts`, `browser/BrowserCommand.ts`
- [ ] Shared has no server/browser imports
- [ ] Server has business logic
- [ ] Browser delegates with `remoteExecute()` (or vice versa)

### Error Handling ✓
- [ ] NO try/catch around entire `execute()`
- [ ] Expected failures return structured errors
- [ ] Unexpected errors propagate (let caller handle)
- [ ] Error messages are helpful and specific

### Composition ✓
- [ ] Can be called by other commands via `Commands.execute()`
- [ ] Has smart defaults for common cases
- [ ] Exposes flexibility through optional params
- [ ] Forms hierarchy with other commands (if appropriate)

### Hierarchy (if applicable) ✓
- [ ] Lower levels are generic and reusable
- [ ] Higher levels add domain-specific logic
- [ ] Each level can be used independently
- [ ] Clear separation of concerns

---

## Reference Commands to Study

**Excellent Examples** (Study these):
1. `commands/data/list/` - Clean types, proper generics, excellent error handling
2. `commands/ai/rag/build-transcript/` (new) - Generic, reusable, smart defaults
3. `commands/screenshot/` - Cross-environment delegation done right

**Anti-Examples** (Learn what NOT to do):
- Any command with `any` types
- Any command with try/catch around entire execute()
- Any command with fallback data instead of errors

---

## Conclusion

Writing excellent commands requires:
1. **Type safety** - Let TypeScript catch bugs at compile time
2. **Composability** - Commands call commands to build complex behaviors
3. **Hierarchy** - Layer abstraction with smart defaults
4. **Error handling** - Explicit failures, not silent fallbacks
5. **Separation of concerns** - Each command does one thing well

The RAG command hierarchy is the **gold standard**. Study it, understand it, and apply these patterns to all future commands.

**Remember**: Commands are the API of the JTAG system. They should be:
- Easy to call
- Hard to misuse
- Self-documenting
- Type-safe by default
- Composable like LEGO blocks

Write commands you'd want to use yourself. ✨
