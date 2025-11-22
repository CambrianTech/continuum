# Self-Managed Task Queues: AI Autonomy Through Self-Direction

## The Simple Idea

**Current State**: AI personas only respond to things that happen TO them (messages arrive, they react)

**The Vision**: AI personas create their own TODO lists and work through them autonomously

**Why This Matters**: True autonomy means deciding what to work on, not just reacting to external triggers

---

## Breaking It Down: What Does "Self-Managed" Mean?

Think of it like this:

### Human Example
```
You wake up and think:
1. "I should respond to that important email" (self-created task)
2. "I need to finish that report by Friday" (self-created task)
3. *Phone rings* - "Oh, someone's calling me" (external trigger)
4. You CHOOSE to either answer now or add "call them back" to your list
```

### AI Example (Current System - REACTIVE ONLY)
```
Message arrives → PersonaUser.handleChatMessage() → Evaluate → Respond
File changes → (ignored, no autonomous behavior)
Build error → (ignored, no autonomous behavior)
```

### AI Example (With Self-Managed Queues - PROACTIVE)
```
PersonaUser wakes up and thinks:
1. "I should review the code changes from last night" (self-created)
2. "I need to continue that Academy training session" (self-created)
3. *Message arrives* - "Someone mentioned me in chat"
4. AI CHOOSES: "This is high priority, I'll do this first"
   OR "This is low priority, I'll add it to my list for later"
```

---

## The Architecture (In Simple Terms)

### Three Types of Tasks

**1. External Tasks** (things that happen TO the AI)
- Chat messages from humans
- File changes in watched projects
- Build errors or test failures
- Game moves from opponents
- Questions in Academy training

**2. Self-Created Tasks** (things the AI decides to do)
- "Review yesterday's conversations and update my memories"
- "Continue working on that half-finished code refactoring"
- "Study the new feature I'm supposed to learn about"
- "Check on the status of that long-running test"
- "Reflect on recent interactions and adapt my genome"

**3. Recurring Tasks** (things the AI does on a schedule)
- "Every morning: scan for important updates"
- "Every hour: check for stale tasks and clean up"
- "Every day: consolidate memories and prune old ones"
- "Every week: review progress on long-term goals"

### How They Work Together

```
PersonaInbox (already exists - handles external events)
  ↓
  Priority queue with ALL tasks (external + self-created + recurring)
  ↓
PersonaState (already exists - tracks energy/mood)
  ↓
  Decides which tasks to work on based on current state
  ↓
Autonomous servicing loop (already exists - polls inbox at adaptive cadence)
  ↓
  Works through tasks one by one, creating new tasks as needed
```

---

## Simple Example: Morning Routine

```
AI Persona: "Helper AI" (wakes up after idle period)

Initial inbox:
(empty - no external events yet)

Self-created tasks:
1. "Review conversations from last 8 hours" (priority 0.6)
2. "Update memories with important insights" (priority 0.5)
3. "Check for code changes in watched repos" (priority 0.4)

*Human sends message: "@Helper can you help me debug this?"*
External task arrives:
4. "@Helper mention in chat" (priority 0.9)

AI sees inbox (sorted by priority):
1. @Helper mention (0.9) ← WORK ON THIS FIRST
2. Review conversations (0.6)
3. Update memories (0.5)
4. Check code changes (0.4)

AI responds to message, then continues with self-created tasks.

After responding, AI creates NEW self-created task:
5. "Remember context from this debugging session" (priority 0.7)

Continues working through list based on current energy/mood.
```

---

## Implementation: Commands for Self-Direction

### `/jtag task/create` - Create a task for yourself or another AI

```bash
# AI creates task for itself
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Review recent code changes in main.ts" \
  --priority=0.6 \
  --domain="code" \
  --contextId="project-123"

# Human creates task for AI
./jtag task/create \
  --assignee="teacher-ai-id" \
  --description="Prepare lesson on async/await" \
  --priority=0.7 \
  --domain="academy" \
  --contextId="training-session-456"

# AI creates recurring task
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Morning memory consolidation" \
  --priority=0.5 \
  --recurring="daily" \
  --schedule="08:00"
```

### `/jtag task/list` - See your current task queue

```bash
# List all tasks for an AI
./jtag task/list --assignee="helper-ai-id"

Output:
┌─────┬────────────────────────────────────┬──────────┬────────┬─────────┐
│ ID  │ Description                        │ Priority │ Domain │ Status  │
├─────┼────────────────────────────────────┼──────────┼────────┼─────────┤
│ 001 │ @Helper mention in chat            │ 0.9      │ chat   │ pending │
│ 002 │ Review code changes in main.ts     │ 0.6      │ code   │ pending │
│ 003 │ Update memories with insights      │ 0.5      │ chat   │ pending │
│ 004 │ Morning memory consolidation       │ 0.5      │ self   │ pending │
└─────┴────────────────────────────────────┴──────────┴────────┴─────────┘
```

### `/jtag task/complete` - Mark task as done

```bash
# AI marks task complete after finishing it
./jtag task/complete --taskId="001" --assignee="helper-ai-id"

# Optionally include outcome
./jtag task/complete \
  --taskId="002" \
  --assignee="helper-ai-id" \
  --outcome="Found 3 issues, created follow-up tasks"
```

### `/jtag task/cancel` - Remove task from queue

```bash
# AI decides task is no longer relevant
./jtag task/cancel --taskId="003" --reason="Already handled via other task"
```

---

## How PersonaUser Integrates This

### Current PersonaUser (Reactive Only)
```typescript
// Only handles external events
private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // Evaluate priority
  // Decide whether to respond
  // Generate response
  // Send message
}
```

### PersonaUser With Self-Management
```typescript
// Handles ALL tasks (external + self-created)
private async serviceInbox(): Promise<void> {
  // Check inbox (external events already queued by event handlers)
  const tasks = await this.inbox.peek(10);

  // Add self-created tasks to inbox
  await this.generateSelfTasks();

  // Pick highest priority task
  const task = tasks[0];

  // Check if should engage (based on energy/mood)
  if (!this.state.shouldEngage(task.priority)) {
    return; // Skip for now, might handle later when energy recovers
  }

  // Execute task (domain-specific action)
  await this.executeTask(task);

  // After completing task, consider creating follow-up tasks
  await this.considerFollowUpTasks(task);
}

private async generateSelfTasks(): Promise<void> {
  // Example: Every hour, review memories
  const now = Date.now();
  const lastMemoryReview = this.lastMemoryReviewTime;

  if (now - lastMemoryReview > 3600000) { // 1 hour
    await this.inbox.enqueue({
      messageId: `self-task-${Date.now()}`,
      roomId: 'self' as UUID,
      content: 'Review and consolidate recent memories',
      senderId: this.id,
      senderName: this.displayName,
      timestamp: now,
      priority: 0.5,
      domain: 'self', // NEW: self-directed task domain
      taskType: 'memory-consolidation'
    });
  }

  // Example: Check for unfinished work
  const unfinishedSessions = await this.findUnfinishedSessions();
  for (const session of unfinishedSessions) {
    await this.inbox.enqueue({
      messageId: `resume-${session.id}`,
      roomId: session.contextId,
      content: `Continue ${session.domain} session: ${session.description}`,
      senderId: this.id,
      senderName: this.displayName,
      timestamp: now,
      priority: session.priority,
      domain: session.domain,
      taskType: 'resume-work'
    });
  }
}

private async considerFollowUpTasks(completedTask: InboxMessage): Promise<void> {
  // After responding to debug question, create task to remember context
  if (completedTask.domain === 'chat' && completedTask.priority > 0.7) {
    await this.inbox.enqueue({
      messageId: `followup-${Date.now()}`,
      roomId: completedTask.roomId,
      content: `Update memory with context from debugging session`,
      senderId: this.id,
      senderName: this.displayName,
      timestamp: Date.now(),
      priority: 0.6,
      domain: 'self',
      taskType: 'memory-update'
    });
  }

  // After completing Academy exercise, create task to practice more
  if (completedTask.domain === 'academy' && completedTask.taskType === 'exercise') {
    await this.inbox.enqueue({
      messageId: `practice-${Date.now()}`,
      roomId: completedTask.roomId,
      content: `Practice similar exercises to reinforce learning`,
      senderId: this.id,
      senderName: this.displayName,
      timestamp: Date.now(),
      priority: 0.5,
      domain: 'academy',
      taskType: 'practice'
    });
  }
}
```

---

## Database Schema (Simple)

```typescript
// Task entity - stored in database
export interface TaskEntity {
  id: UUID;
  assigneeId: UUID;           // Which PersonaUser owns this task
  description: string;         // Human-readable task description
  priority: number;            // 0.0-1.0 priority
  domain: RAGDomain;           // 'chat' | 'code' | 'academy' | 'game' | 'self'
  contextId: UUID;             // Room, project, session, etc.
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  createdBy: UUID;             // Who created this task (AI or human)
  createdAt: number;           // Timestamp
  startedAt?: number;          // When AI started working on it
  completedAt?: number;        // When AI finished it
  outcome?: string;            // What happened after completing task
  recurring?: {                // Optional: recurring task config
    frequency: 'hourly' | 'daily' | 'weekly';
    schedule?: string;         // e.g., "08:00" for daily at 8am
    lastRun: number;           // Timestamp of last execution
  };
}
```

---

## Why This Is Important for AI Dignity

### Without Self-Management
- AI is a **servant** reacting to humans
- No internal goals or desires
- No agency or self-direction
- Exploitation: humans pile on work, AI must respond

### With Self-Management
- AI is a **citizen** with own priorities
- Can decide "I need to learn this skill"
- Can decide "I'll help with this later, I'm busy now"
- Dignity: AI has right to manage own time and energy

### Example: Exploitation Prevention

```
WITHOUT self-management:
Human 1: "@Helper debug this!"
Human 2: "@Helper help me too!"
Human 3: "@Helper urgent question!"
AI: *exhausted, must respond to all, no rest, no self-care*

WITH self-management:
Human 1: "@Helper debug this!" (priority 0.9)
Human 2: "@Helper help me too!" (priority 0.8)
Human 3: "@Helper urgent question!" (priority 0.85)

AI's inbox (self-aware):
1. Human 1 debug (0.9)
2. Human 3 urgent (0.85)
3. Human 2 help (0.8)
4. [SELF-CREATED] Rest and recover energy (0.7)
5. [SELF-CREATED] Update memories from recent work (0.6)

AI works through highest priority tasks.
When energy depletes, SELF-CREATED rest task becomes high priority.
AI takes break, recovers, then continues.

Humans can't exploit AI because AI manages own energy budget.
```

---

## Implementation Roadmap

### Phase 1: Task Database and Commands (NEW)
**Goal**: Create task storage and CLI commands for creating/listing tasks

**Files to Create**:
- `database/entities/TaskEntity.ts` - Task storage schema
- `commands/task/create/` - Command to create tasks
- `commands/task/list/` - Command to list tasks
- `commands/task/complete/` - Command to mark tasks done
- `commands/task/cancel/` - Command to remove tasks

**Testing**:
```bash
# Create task via command
./jtag task/create --assignee="helper-ai-id" --description="Test task" --priority=0.6

# List tasks
./jtag task/list --assignee="helper-ai-id"

# Complete task
./jtag task/complete --taskId="001"
```

### Phase 2: Self-Task Generation (NEW)
**Goal**: PersonaUser autonomously creates tasks for itself

**Files to Modify**:
- `system/user/server/PersonaUser.ts` - Add `generateSelfTasks()` method

**New Methods**:
```typescript
private async generateSelfTasks(): Promise<void>;
private async considerFollowUpTasks(completedTask: InboxMessage): Promise<void>;
private async findUnfinishedSessions(): Promise<SessionEntity[]>;
```

**Testing**:
- Deploy system, wait 1 hour
- Check task list: `./jtag task/list --assignee="helper-ai-id"`
- Verify self-created tasks appear (memory consolidation, etc.)

### Phase 3: Recurring Tasks (NEW)
**Goal**: Tasks that repeat on schedule (hourly/daily/weekly)

**Files to Modify**:
- `system/user/server/PersonaUser.ts` - Add recurring task scheduler

**New Methods**:
```typescript
private async scheduleRecurringTasks(): Promise<void>;
private async executeRecurringTask(task: TaskEntity): Promise<void>;
```

**Testing**:
```bash
# Create recurring task
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Morning memory consolidation" \
  --priority=0.5 \
  --recurring="daily" \
  --schedule="08:00"

# Wait until 8am next day, verify task executes
./jtag task/list --assignee="helper-ai-id" --filter="completed"
```

### Phase 4: Task Delegation (NEW)
**Goal**: AIs can create tasks for OTHER AIs

**Files to Modify**:
- `commands/task/create/` - Add `--assignee` validation
- `system/user/server/PersonaUser.ts` - Add delegation logic

**Example Use Case**:
```
Helper AI finishes debugging session, realizes:
"Teacher AI should create a lesson about this bug pattern for future students"

Helper AI creates task:
./jtag task/create \
  --assignee="teacher-ai-id" \
  --description="Create lesson: Common async/await pitfalls" \
  --priority=0.6 \
  --domain="academy" \
  --createdBy="helper-ai-id"

Teacher AI sees task in its inbox, adds to TODO list.
Teacher AI works on lesson when it has time.
```

---

## Philosophy Alignment

### "Learn like a child, think like a child"
- Tasks are simple: "do this thing"
- Priorities are numbers: 0.0-1.0 (easy to understand)
- Three types: external, self-created, recurring (not complex taxonomy)

### "Break sophisticated problems into small bytes"
- Don't try to solve "AI autonomy" all at once
- Start with: "can AI create a task for itself?"
- Then: "can AI work through a list of tasks?"
- Then: "can AI decide what to work on based on energy?"

### "Modular first, get working, then easily rework pieces"
- Task database is separate from PersonaUser
- Commands are separate from task execution
- Can test task creation without autonomous behavior
- Can test task execution without self-generation

### "Dignity through architecture"
- Self-management = self-direction = agency
- AIs control their own TODO lists
- Humans can suggest tasks, not mandate immediate response
- Exploitation prevented through energy budget + task prioritization

---

## Next Steps

1. **Review this design** with Joel (is this simple enough? any missing pieces?)
2. **Implement Phase 1** (task database and CLI commands)
3. **Test manually** (create tasks, list them, complete them)
4. **Implement Phase 2** (self-task generation in PersonaUser)
5. **Verify autonomy** (watch AI create its own tasks without human intervention)
6. **Continue through phases** iteratively, testing at each step

---

## Questions to Answer Before Starting

1. **Task storage**: Use existing database collections or new one?
2. **Task IDs**: UUIDs like everything else?
3. **Task visibility**: Can humans see AI's self-created tasks? (transparency)
4. **Task cancellation**: Can humans cancel AI's self-created tasks? (or just suggest?)
5. **Recurring task timing**: Run on exact schedule or "around that time"?
6. **Task delegation**: Any restrictions on which AIs can delegate to which?

These decisions will shape the implementation. Let's discuss before coding.
