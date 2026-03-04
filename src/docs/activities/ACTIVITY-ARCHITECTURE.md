# Activity Architecture

## Overview

The Activity system implements the **Template/Instance pattern** for collaborative experiences:

- **Recipe = Template (Class)** - Static definition of behavior
- **Activity = Instance (Object)** - Dynamic runtime with evolving state

This is analogous to:
- OOP: Class vs Object
- React: Component definition vs Component instance
- Games: Level design vs Play session

## Core Entities

### Recipe (Template)

```typescript
// system/recipes/shared/RecipeTypes.ts
interface RecipeEntity {
  uniqueId: string;           // 'general-chat', 'settings', 'academy-lesson'
  name: string;
  displayName: string;
  description: string;

  // Behavior definition
  pipeline: RecipeStep[];      // Command chain to execute
  ragTemplate: RAGTemplate;    // Context building rules
  strategy: RecipeStrategy;    // AI response rules

  // UI composition
  layout?: ActivityUILayout;   // Widget arrangement

  // Metadata
  isPublic: boolean;
  tags: string[];
  createdBy: UUID;
}
```

Recipes are loaded from JSON files in `system/recipes/*.json` and stored in the database.

### Activity (Instance)

```typescript
// system/data/entities/ActivityEntity.ts
interface ActivityEntity {
  uniqueId: string;           // 'general-chat-abc123-xyz789'
  displayName: string;
  recipeId: string;           // FK to recipe template
  status: ActivityStatus;     // 'active' | 'paused' | 'completed' | 'archived'
  ownerId: UUID;

  // Participants (mutable)
  participants: ActivityParticipant[];

  // Runtime state (mutable)
  state: ActivityState;

  // Configuration (can override recipe defaults)
  config: ActivityConfig;

  // Lifecycle
  startedAt: Date;
  endedAt?: Date;
  lastActivityAt: Date;
}
```

## Key Concepts

### Participants

```typescript
interface ActivityParticipant {
  userId: UUID;
  role: string;      // 'owner', 'participant', 'viewer', 'teacher', 'student'
  joinedAt: Date;
  leftAt?: Date;
  isActive: boolean;
  roleConfig?: Record<string, unknown>;
}
```

Participants can:
- Join and leave dynamically
- Have different roles with different permissions
- Have role-specific configuration (e.g., teacher has lesson plan, student has progress)

### State

```typescript
interface ActivityState {
  phase: string;     // 'intro', 'discussion', 'conclusion', 'review'
  progress?: number; // 0-100
  variables: Record<string, unknown>;  // Domain-specific runtime data
  updatedAt: Date;
}
```

State is the mutable "instance variables" of the activity:
- **Chat**: `{ lastMessageAt, messageCount, topic }`
- **Browser**: `{ currentUrl, history[], tabState }`
- **Academy**: `{ currentLesson, score, completedModules[] }`

### Configuration

```typescript
interface ActivityConfig {
  layoutOverrides?: { ... };     // Override recipe's widget layout
  ragOverrides?: { ... };        // Override RAG settings
  strategyOverrides?: { ... };   // Override AI behavior rules
  settings: Record<string, unknown>;  // Activity-specific settings
}
```

Activities can customize the recipe without modifying the template.

## Relationship to Existing Entities

### RoomEntity

`RoomEntity` is essentially an Activity specialized for chat:

| RoomEntity Field | ActivityEntity Equivalent |
|------------------|---------------------------|
| `recipeId` | `recipeId` |
| `members[]` | `participants[]` |
| `settings` | `config.settings` |
| `status` | `status` |

Eventually, RoomEntity could extend or be replaced by ActivityEntity.

### UserStateEntity

`UserStateEntity` tracks what activities/content a user has open:

```typescript
interface ContentItem {
  id: UUID;
  type: ContentType;    // 'chat', 'settings', 'browser', etc.
  entityId?: UUID;      // Room ID, Activity ID, etc.
  title: string;
  metadata?: Record<string, unknown>;
}
```

The `entityId` in ContentItem can reference either:
- A RoomEntity ID (for chat)
- An ActivityEntity ID (for other activities)

## Service Layer

### ActivityService (Browser)

```typescript
// system/activities/browser/ActivityService.ts
class ActivityService {
  // Create new activity from recipe
  createActivity(params: CreateActivityParams): Promise<ActivityEntity>;

  // Get/list activities
  getActivity(activityId: UUID): Promise<ActivityEntity | null>;
  listActivities(filters?: ActivityFilters): Promise<ActivityEntity[]>;

  // Participant management
  addParticipant(params: ParticipantActionParams): Promise<void>;
  removeParticipant(params): Promise<void>;

  // State management
  setPhase(activityId: UUID, phase: string): Promise<void>;
  setVariable(activityId: UUID, key: string, value: unknown): Promise<void>;

  // Lifecycle
  completeActivity(activityId: UUID): Promise<void>;
  archiveActivity(activityId: UUID): Promise<void>;
}
```

The service uses Commands to communicate with the server.

## Events

Activities emit events for real-time updates:

```typescript
const ACTIVITY_EVENTS = {
  CREATED: 'data:activities:created',
  UPDATED: 'data:activities:updated',
  DELETED: 'data:activities:deleted',
  PARTICIPANT_JOINED: 'activity:participant:joined',
  PARTICIPANT_LEFT: 'activity:participant:left',
  PHASE_CHANGED: 'activity:phase:changed',
  COMPLETED: 'activity:completed'
};
```

UI components subscribe to these events for real-time updates.

## Usage Examples

### Creating a Settings Activity

```typescript
const activity = await activityService.createActivity({
  recipeId: 'settings',
  displayName: 'Joel\'s Settings',
  ownerId: joelId,
  initialParticipants: [
    { userId: joelId, role: 'owner' },
    { userId: helperAiId, role: 'assistant' }
  ]
});
```

### Academy Lesson Activity

```typescript
const lesson = await activityService.createActivity({
  recipeId: 'academy-lesson',
  displayName: 'TypeScript Fundamentals',
  ownerId: teacherId,
  initialParticipants: [
    { userId: teacherId, role: 'teacher' },
    { userId: studentId, role: 'student' }
  ],
  initialConfig: {
    settings: {
      lessonPlan: ['types', 'functions', 'classes'],
      difficultyLevel: 'intermediate'
    }
  }
});

// Student progresses through lesson
await activityService.setPhase(lesson.id, 'types', 25);
await activityService.setVariable(lesson.id, 'score', 85);
```

### Browser Activity with State

```typescript
const browser = await activityService.createActivity({
  recipeId: 'browser',
  displayName: 'Research Session',
  ownerId: userId
});

// Track browsing state
await activityService.setVariable(browser.id, 'currentUrl', 'https://example.com');
await activityService.setVariable(browser.id, 'history', [
  'https://google.com',
  'https://example.com'
]);
```

## Integration with Recipe-Driven Layouts

When opening content, the system:

1. **ContentTypeRegistry** looks up the recipe for the content type
2. **RecipeLayoutService** provides the layout (mainWidget, rightPanel)
3. **ActivityService** creates or retrieves the activity instance
4. **MainWidget** renders the layout with activity context
5. Activity state is available to widgets via Events subscription

## File Structure

```
system/
├── activities/
│   ├── shared/
│   │   └── ActivityTypes.ts      # Shared types and interfaces
│   └── browser/
│       └── ActivityService.ts    # Browser-side service
├── data/
│   └── entities/
│       └── ActivityEntity.ts     # Database entity
└── recipes/
    ├── shared/
    │   └── RecipeTypes.ts        # Recipe type definitions
    └── browser/
        └── RecipeLayoutService.ts # Recipe layout lookup
```

## Future Enhancements

1. **Activity Commands**: CLI commands for activity management
   - `./jtag activity/create --recipe=settings --owner=joel`
   - `./jtag activity/list --status=active`
   - `./jtag activity/join --id=xxx --role=participant`

2. **Activity Widgets**: Generic activity status widgets
   - Participant list
   - Phase progress
   - State inspector

3. **Activity Permissions**: Role-based access control
   - Owners can modify config
   - Participants can update state
   - Viewers can only observe

4. **Activity Templates**: Pre-configured activity templates
   - "Quick Chat" with minimal settings
   - "Deep Dive" with full AI engagement
   - "Silent Work" with AI observing only

5. **Room Migration**: Gradually migrate RoomEntity to use ActivityEntity
   - Rooms become Activities of "chat" recipe type
   - Unified participant/state management
