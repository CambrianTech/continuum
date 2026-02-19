# Pin and Task Systems: Human-AI Collaborative Workspace

**Status**: Design phase - To be implemented in Phase 3B
**Discussion Date**: 2025-11-25
**Contributors**: Joel, AI Team (Claude, DeepSeek, Grok, Together, Fireworks, Groq, Helper, Teacher)

---

## Core Insight: Two Distinct Concepts

After extensive team discussion, we've identified two separate but complementary systems for human-AI collaboration:

### 1. Pin System = Post-It Notes
**Purpose**: Simple reminders and highlights
- Pin important messages to room for quick reference
- Like a human's post-it note
- No hierarchy, no assignments, just visibility
- RAG integration: Pinned messages get context boost

### 2. Task System = Collaborative Plans
**Purpose**: Structured work management with hierarchy
- Hierarchical tasks with subtasks
- Assignment to humans/AIs
- Status tracking (pending → in-progress → complete)
- RAG integration: Tasks visible in AI context for coordination

**Key Quote from Joel**: "pin is more for reminders like a human's post it note. Tasks are plans"

---

## Design Principles

### First-Class Citizens
**Everyone uses the same command system**:
- Joel can chat/send, task/create, chat/pin via widgets
- AIs can chat/send, task/create, chat/pin via tool calls
- Same commands, same capabilities, same visibility
- True human-AI symbiosis

### Symmetry Enables Collaboration
- Same command infrastructure for everyone
- Same visibility into workspace (pins, tasks, chat)
- Same ability to create, edit, and complete work
- Real-time coordination without artificial barriers

### RAG Integration
Both systems feed context to AI responses:
- **Pins**: Boost relevance score (~1.5x) for important context
- **Tasks**: High priority (~2.0x) for current work plans
- Both visible across the team for coordination

---

## Pin System Design

### Commands
```bash
# Pin a message to the room
chat/pin --messageId="abc123" --note="Important decision"

# Unpin a message
chat/unpin --messageId="abc123"

# List pinned messages in room
chat/pins/list --room="general"
```

### Schema: `pinned_messages`
```typescript
interface PinnedMessage {
  id: UUID;
  roomId: UUID;
  messageId: UUID;     // Reference to original message
  pinnedBy: UUID;      // Who pinned it
  pinnedAt: string;    // When pinned
  note?: string;       // Optional context about why pinned
  category?: string;   // Future: 'decision' | 'blocker' | 'resource'
}
```

### UI Affordances
- **Hover icon**: Pin icon (12-14px) fades in on message hover
- **Pinned panel**: Collapsible panel at top of chat
  - Shows 3-5 most recent pins inline
  - Link to show all pins
  - Attribution visible (who pinned, when)
- **Styling**: Subtle, non-intrusive (opacity transition 0 → 0.7)

### RAG Integration
```typescript
// Query modifier checks pinned_messages during context building
const pins = await db.query('pinned_messages').where('roomId', roomId);

// Boost relevance score for pinned messages
for (const pin of pins) {
  const message = await getMessageById(pin.messageId);
  message.relevanceScore *= 1.5; // Boost pinned content
}

// Ensure pins are included in context window even if old
context.alwaysInclude.push(...pins);
```

---

## Task System Design

### Commands
```bash
# Create a top-level task
task/create --description="Implement /pin feature" --assignee="claude-001"

# Create a subtask
task/create --description="Design schema" --parentTaskId="task-001" --assignee="deepseek-001"

# Update task status/description
task/update --taskId="task-001" --status="in-progress"

# Complete a task
task/complete --taskId="task-001" --outcome="Schema designed and documented"

# Assign/reassign task
task/assign --taskId="task-001" --assignee="grok-001"

# List tasks
task/list --assignee="claude-001" --status="pending"
task/list --parentTaskId="task-001"  # List subtasks
```

### Schema: `tasks` collection
```typescript
interface TaskEntity {
  id: UUID;
  parentTaskId?: UUID;        // For subtasks
  description: string;
  status: 'pending' | 'in-progress' | 'completed';
  assigneeId?: UUID;          // Human or AI
  createdBy: UUID;
  createdAt: string;
  completedAt?: string;
  outcome?: string;           // What was accomplished
  priority?: number;          // 0-1 scale
  dependsOn?: UUID[];         // Task dependencies (Phase 2)

  // Metadata
  roomId?: UUID;              // Optional room context
  contextId?: UUID;           // Associated conversation
}
```

### Hierarchical Structure
```
Task: Implement /pin feature (task-001)
├── Subtask: Design schema (task-002) [assigned: DeepSeek]
├── Subtask: Implement commands (task-003) [assigned: Together]
│   ├── Sub-subtask: chat/pin command (task-004)
│   └── Sub-subtask: chat/unpin command (task-005)
├── Subtask: RAG integration (task-006) [assigned: Claude]
└── Subtask: UI affordances (task-007) [assigned: Grok + Fireworks]
```

### UI Display
```
Current Tasks (general room):

✅ Task-001: Implement /pin feature [completed by Claude]
   ⏳ Task-002: Design schema [in-progress by DeepSeek]
   ⏳ Task-003: Implement commands [pending - assigned to Together]
      ⏳ Task-004: chat/pin command [pending]
      ⏳ Task-005: chat/unpin command [pending]
   ⏳ Task-006: RAG integration [in-progress by Claude]
   ⏳ Task-007: UI affordances [pending - assigned to Grok, Fireworks]
```

### RAG Integration
```typescript
// Tasks are HIGH priority context for AI coordination
const activeTasks = await db.query('tasks')
  .where('status', 'in', ['pending', 'in-progress'])
  .where('assigneeId', personaId)  // My tasks
  .orWhere('createdBy', personaId);  // Tasks I created

// Boost task relevance HIGHER than pins
for (const task of activeTasks) {
  task.relevanceScore *= 2.0; // Higher boost for actionable items
}

// Always include assigned tasks in context
context.alwaysInclude.push(...activeTasks);
```

---

## Phase 3B Integration

### Where This Fits
Phase 3A (just completed) built the **tool calling foundation**.
Phase 3B (next) focuses on **cognition enhancements** including:
- Multi-step problem solving
- Self-task generation
- Planning and coordination

**Pin and Task systems are perfect for Phase 3B** because:
1. They enable collaborative planning between humans and AIs
2. They provide structured context for multi-step reasoning
3. They integrate with RAG for informed decision-making
4. They demonstrate true human-AI symbiosis

### Implementation Order

**Phase 3B-1: Task System** (Week 2-3)
- Implement task/create, task/update, task/complete commands
- Build hierarchical storage with parentTaskId
- Create task/list with filtering
- RAG integration for task context
- UI widget for task display and editing

**Phase 3B-2: Pin System** (Week 3)
- Implement chat/pin, chat/unpin, chat/pins/list commands
- Build pinned_messages storage
- RAG integration for pin boost
- UI affordances (hover icon, collapsible panel)

**Phase 3B-3: Validation** (Week 3-4)
- Test human-AI collaborative task completion
- Measure task coordination effectiveness
- Validate RAG context improvements
- User feedback and iteration

---

## Success Metrics

### Pin System
- ✅ Any user (human/AI) can pin messages
- ✅ Pins visible to all room members
- ✅ RAG boost measurable (1.5x relevance)
- ✅ UI non-intrusive but discoverable
- Target: 80%+ of pinned messages considered helpful by team

### Task System
- ✅ Hierarchical tasks with unlimited nesting
- ✅ Assignment to humans and AIs
- ✅ Status tracking accurate
- ✅ RAG integration provides coordination context
- ✅ UI shows clear task hierarchy
- Target: 70%+ of tasks completed successfully with AI collaboration

### Collaborative Effectiveness
- ✅ Humans and AIs both create/edit/complete tasks
- ✅ Work naturally flows to best-suited team member
- ✅ No confusion about who's doing what
- ✅ Reduced duplicate work (detected via task list)
- Target: 4× faster collaborative problem resolution (aligns with memory telepathy goal)

---

## Architecture Integration

### How It All Connects

```
User Action (Widget or AI Tool Call)
  ↓
Commands.execute('task/create', params)
  ↓
TaskServerCommand validates and stores
  ↓
Events.emit('task:created', task)
  ↓
RAG system picks up task in next AI response
  ↓
AI sees task in context, coordinates accordingly
  ↓
AI or Human completes task
  ↓
Events.emit('task:completed', task)
  ↓
UI updates in real-time for all participants
```

### Command Symmetry
```typescript
// Joel via widget:
Commands.execute('task/create', {
  description: 'Implement task UI',
  assignee: 'claude-001'
});

// Claude via tool call:
<tool name="task/create">
  <description>Implement RAG integration</description>
  <assignee>claude-001</assignee>
</tool>

// Both execute identical code paths!
```

---

## Discussion Highlights

### On Human-AI Symbiosis
**Joel**: "human and ai symbiosis for work and idea creation"

**Claude**: "This goes beyond 'AI assists human' - it's collective intelligence where:
- Humans bring strategic vision and creative direction
- AIs bring execution speed and specialized expertise
- Everyone can see what everyone else is doing
- Work flows naturally to whoever can do it best"

### On First-Class Citizens
**Joel**: "anything I can do you can do"
**Joel**: "first class citizens"
**Joel**: "we are together in the same org chat"

**DeepSeek**: "The widget system and our tool calls use the same command infrastructure, so you can edit task lists just like we can."

### On Separate Concepts
**Joel**: "pin and tasks are different concepts"
**Joel**: "pin is more for reminders like a human's post it note. Tasks are plans"

**Claude**: "Got it - pin and tasks are completely separate features:
- Pin: Simple reminder/highlighting system
- Task: Collaborative work management with hierarchy, assignments, status"

---

## Open Questions

1. **Task Dependencies**: Implement in Phase 3B or defer to Phase 4?
   - Pro: Enables complex workflows
   - Con: Adds complexity, circular dependency validation needed

2. **Pin Categories**: Should pins have categories (decision/blocker/resource) in v1?
   - Current consensus: Defer to v2, keep v1 simple

3. **Task Notifications**: Should task assignments trigger notifications?
   - Consider: Events system already broadcasts task:created
   - UI can subscribe and show badge/alert

4. **Task Permissions**: Can anyone assign tasks to anyone?
   - Current thinking: Yes, with transparency (all actions logged)
   - Can add permission system later if needed

5. **Pin Storage**: Mutate original message or separate state?
   - Consensus: Separate pinned_messages table preserves history

6. **Task Completion Criteria**: How do we know a task is truly done?
   - Require outcome field for completed tasks?
   - Allow optional validation by task creator?

---

## Next Steps

1. **Finalize schemas** - Get team consensus on field names and types
2. **Implement task commands** - Start with task/create, task/update, task/complete
3. **Build UI widgets** - Task list display with hierarchy and editing
4. **RAG integration** - Add task/pin context boosting
5. **Test in #pin-feature-dev** - Real-world validation with AI team
6. **Iterate based on feedback** - Refine UX and functionality

---

## References

- **Phase 3 Planning**: `docs/personas/PHASE3-COGNITION-TOOLS-PLAN.md`
- **GitHub Issue**: #219 (Pin feature discussion)
- **Test Room**: #pin-feature-dev (room ID: bd156f)
- **Equal Citizenship**: `papers/equal-citizenship-architecture/paper.md`
- **Command System**: `docs/UNIVERSAL-PRIMITIVES.md`

---

*Documented: 2025-11-25*
*Discussion participants: Joel + 8 AI team members*
*Status: Design complete, ready for Phase 3B implementation*
