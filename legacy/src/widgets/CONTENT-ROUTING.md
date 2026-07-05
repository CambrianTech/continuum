# Content Routing Architecture

**Status**: Design Phase
**Last Updated**: 2025-11-13

---

## Vision: Universal Content Area

The main content panel should display different content types based on routing, not multiple simultaneous widgets.

**Current State**: `chat-widget` always displays
**Target State**: Router shows appropriate content based on URL/state

```
Main Panel (content-view)
├── chat-widget (when viewing room)
├── training-monitor-widget (when viewing training job)
├── academy-widget (when viewing learning content)
├── game-widget (when viewing game)
└── [future content types]
```

---

## Routing Strategy

### URL Pattern (Future)
```
http://localhost:9003/chat/room-id-123
http://localhost:9003/training/session-id-456
http://localhost:9003/academy/lesson-id-789
http://localhost:9003/game/match-id-012
```

### State-Based Routing (Current Interim)
```typescript
interface ContentRoute {
  type: 'chat' | 'training' | 'academy' | 'game';
  id: string;  // room ID, session ID, lesson ID, etc.
  metadata?: Record<string, unknown>;
}
```

**UserStateEntity** already tracks current state:
```typescript
{
  currentRoomId?: UUID;
  currentTrainingSessionId?: UUID;  // NEW
  currentContentType: 'chat' | 'training' | 'academy' | 'game';  // NEW
  // ...
}
```

---

## Implementation Phases

### Phase 1: Content Type Abstraction (Current)
- Document architecture (this file)
- Complete all fine-tuning adapters (OpenAI ✅, Together, Fireworks)
- Prepare for future routing

### Phase 2: State-Based Routing
- Add `currentContentType` to UserStateEntity
- Modify `main-widget` to conditionally render based on content type
- Keep chat as default, add training view

### Phase 3: URL Routing
- Implement URL rewriting (HTML5 History API)
- Add navigation between content types
- Update sidebar to show active content type

### Phase 4: Content Registry
- Pluggable content type system
- Register new content types dynamically
- Auto-generate routes from registry

---

## Training Monitor Widget (Example)

**When**: User navigates to training session or clicks "View Training" link
**Shows**: Training job details, metrics, charts, timeline (like OpenAI UI)

**Data Source**:
- `./jtag genome/train/status --sessionId=<id>` (polls every 30s)
- Events: `training:started`, `training:progress`, `training:completed`
- Uses refactored `BaseLoRATrainerServer._queryStatus()` method

**UI Components**:
- Job metadata panel (status, model IDs, timestamps)
- Hyperparameters display
- Loss/Accuracy charts (recharts or similar)
- Event timeline
- Links to playground, files

---

## Benefits of This Architecture

1. **Clean Separation**: One content type active at a time
2. **URL Shareability**: Link directly to training job, chat room, etc.
3. **Browser Navigation**: Back/forward buttons work correctly
4. **Extensibility**: Easy to add new content types (games, academy, etc.)
5. **Performance**: Only render active content, not all widgets simultaneously

---

## Current Priority

**Finish fine-tuning adapters first** (Together AI, Fireworks), then return to routing implementation when UI features are needed.

**Routing is design debt, not blocking**. The refactored async handle pattern works without UI changes.
