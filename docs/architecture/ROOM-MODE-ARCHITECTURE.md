# Room-Mode Architecture

## Core Principle

**Every page is a room. Every room has a default mode. Every room has chat.**

A room is the permanent entity. A mode is how you view it. Chat is always available in the right panel regardless of mode.

## Room = Entity + Team + Default Mode

```
Room {
  uniqueId: 'universe'
  name: 'Universe'
  recipeId: 'universe'          // determines default mode + center widget
  team: ['helper', 'local']     // AIs assigned to this room
  members: [owner, ...team]     // everyone who can participate
}
```

The recipe defines:
- **Default mode** (what center widget shows by default)
- **Available modes** (what the room can switch to)
- **Right panel** (always chat for that room's team)

## Modes

### Recipe-based modes (permanent default for a room)

| Room | Default Mode | Center Widget | Right Panel |
|------|-------------|---------------|-------------|
| General | chat | chat-widget | user-list |
| Pantheon | chat | chat-widget | user-list |
| Code | code | code-editor (future) | chat (code room) |
| Universe | theme | theme-widget | chat (universe room) |
| Factory | forge | factory-widget | chat (factory room) |
| Settings | settings | settings-widget | chat (settings room) |
| Grid | grid | grid-overview-widget | chat (grid room) |
| Training | training | training-dashboard | chat (training room) |

### Temporary mode overlay

| Mode | Trigger | Center Widget | Returns To |
|------|---------|---------------|------------|
| live | Click call button | live-widget | Previous mode |

Live mode is the ONLY temporary mode. It overlays any room. When the call ends, you return to the room's default mode.

## URLs

```
/{mode}/{room-uniqueId}     — room in a specific mode
/{room-uniqueId}            — room in its default mode (if unique, like /factory)
```

### Examples

```
/chat/general               — General in chat mode (default)
/chat/universe              — Universe in chat mode (override)
/universe                   — Universe in theme mode (default)
/live/general               — General in live mode (temporary)
/live/universe              — Universe in live mode (temporary)
/factory                    — Factory in forge mode (default)
/chat/factory               — Factory in chat mode (override)
/profile/helper             — Helper's profile (user view, not a room)
/persona/helper             — Helper's brain (user view, not a room)
```

## Mode Switching

### Default mode → chat mode
User clicks "Chat" tab or the right panel expand button.
URL: `/universe` → `/chat/universe`
Center widget swaps from theme-widget to chat-widget.
Right panel stays the same (it's always chat).

### Any mode → live mode  
User clicks call/video button in the chat header.
URL: `/chat/general` → `/live/general` or `/universe` → `/live/universe`
Center widget swaps to live-widget.
Right panel stays as chat (continues during call, like Slack huddle).

### Live mode → previous mode
User clicks "End Call" button.
URL: `/live/general` → `/chat/general` or `/live/universe` → `/universe`
Returns to whatever mode was active before live.

## AI Teams

Every room has a team of AIs. The recipe defines the default team.

```json
// universe.json
{
  "team": {
    "default": ["helper"],
    "specialists": []    // future: visual AI, theme AI
  }
}

// factory.json  
{
  "team": {
    "default": ["helper"],
    "specialists": ["codereview", "teacher"]
  }
}

// general-chat.json
{
  "team": null           // null = everyone joins
}
```

- `null` team = all personas are members (General, Pantheon)
- Explicit team = only listed personas respond in that room
- `helper` is on every team by default (the concierge)

## Right Panel

The right panel ALWAYS shows:
1. **Chat** for that room's team (primary)
2. **Room controls** — call button, member list, room settings
3. **Context-specific tools** — code tools for Code room, forge controls for Factory

The right panel is NOT a separate entity. It's part of the room's view.

## Comparison to Existing Apps

| Feature | Slack | Discord | Teams | Continuum |
|---------|-------|---------|-------|-----------|
| Text chat | Channel | Text channel | Channel | Room (chat mode) |
| Voice/video | Huddle (overlay) | Voice channel (separate) | Meet (overlay) | Live mode (overlay) |
| Transition | Click huddle button | Join voice channel | Click Meet | Click call button |
| Chat during call | Yes (same channel) | No (different channel) | Yes (same channel) | Yes (right panel) |
| Specialized tools | Apps panel | — | Tabs (wiki, etc) | Default mode (recipe widget) |

## Implementation Notes

### What exists
- Rooms with recipeId ✅
- Recipes with layout (center + right widgets) ✅  
- URL routing with view prefix ✅
- Live widget ✅
- Chat widget ✅
- Specialized widgets (factory, grid, settings, theme, training) ✅

### What's missing
1. **Team field on recipes** — which AIs join each room
2. **Right panel wired to room** — currently shows wrong room's chat
3. **Mode switching UI** — button to switch between modes
4. **Mode state** — track current mode vs default mode
5. **Live mode return** — remember previous mode after call ends
6. **Seeder room-team linking** — add team members to rooms during seed
