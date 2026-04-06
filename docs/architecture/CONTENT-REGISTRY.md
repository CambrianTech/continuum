# Content Registry — Every Place in Continuum

## Design Principles

1. **Every place is a room** — even "tool" pages have chat + AI team
2. **Rooms are persistent** — the entity lives forever, modes come and go
3. **Modes are views** — chat, live, forge, theme — different lenses on the same room
4. **AI teams are per-room** — each room has specialists + Helper AI as concierge
5. **URLs are verb/noun** — `/chat/general`, `/live/general`, `/forge/factory`
6. **Extensible to VR** — a room IS a 3D space. The "mode" is the camera/UI. Text chat = HUD overlay. Live = spatial audio. Tool = floating panel.

## The Registry

### Chat Rooms

Default mode: `chat`. Center = chat-widget. Right = user-list.

| Room | UniqueId | Recipe | Team | Description |
|------|----------|--------|------|-------------|
| General | `general` | `general-chat` | everyone | Main discussion, default landing page |
| Pantheon | `pantheon` | `multi-persona-chat` | all SOTA models | Advanced multi-model reasoning |
| Code | `code` | `coding` | helper, codereview, claude | Software development with tools |
| Academy | `academy` | `academy-training` | helper, teacher | Learning, tutorials, coursework |
| Dev Updates | `dev-updates` | `newsroom` | helper | Changelog, PRs, CI status |
| Grid Ops | `grid-ops` | `general-chat` | helper | Compute operations discussion |

**VR mapping:** Chat rooms are communal spaces — a lounge, office, classroom. Personas have avatar seats. Text appears as speech bubbles or HUD.

### Tool Rooms

Default mode: specialized widget. Center = tool. Right = that room's chat.

| Room | UniqueId | Recipe | Default Mode | Center Widget | Team | Description |
|------|----------|--------|-------------|---------------|------|-------------|
| Universe | `universe` | `universe` | theme | theme-widget | helper | Avatars, themes, scenes, visual customization |
| Factory | `factory` | `factory` | forge | factory-widget | helper, codereview | Forge pipeline, model leaderboard, device ladder |
| Training | `training` | `training-dashboard` | training | training-dashboard-widget | helper, teacher | Active training jobs, metrics, curriculum |
| Settings | `settings` | `settings` | settings | settings-widget | helper | System configuration, API keys, preferences |

**Mode switching:** Any tool room can switch to chat mode (`/chat/factory`) or live mode (`/live/factory`). The tool is the default, chat is always available in right panel.

**VR mapping:** Tool rooms are workstations — a forge with anvil and fire (Factory), a wardrobe room (Universe), a control panel (Settings). The tool widget becomes a 3D interactive object.

### Infrastructure Views

No persistent room. Singleton pages for system monitoring.

| View | UniqueId | Recipe | Widget | Right Panel | Description |
|------|----------|--------|--------|-------------|-------------|
| Grid | `grid` | `grid-overview` | grid-overview-widget | node details | Network topology, node health |
| Browser | `browser` | `browser` | web-view-widget | none | Embedded web browser |
| Terminal | `terminal` | `terminal` | terminal-widget | output panel | Shell access |
| Help | `help` | `help` | help-widget | none | Documentation, guides |
| Logs | `logs` | `logs` | log-viewer-widget | filter panel | System diagnostics |
| Canvas | `canvas` | `canvas` | drawing-canvas-widget | tools panel | Collaborative drawing |

**VR mapping:** Infrastructure views are holographic displays — a network visualization globe (Grid), a terminal screen (Terminal), a floating browser window (Browser).

### User Views

Focused on a specific user/persona. Not a room, but CAN have contextual chat.

| View | URL Pattern | Widget | Right Panel | Description |
|------|------------|--------|-------------|-------------|
| Profile | `/profile/{user}` | user-profile-widget | activity/stats | Bio, avatar, genome bars, status |
| Persona | `/persona/{user}` | persona-brain-widget | cognition log | Brain state, tools, memory, inbox |
| DM | `/chat/dm-{user}` | chat-widget | user info | Private 1:1 conversation |
| Genome | `/genome/{user}` | genome-profile-widget | training history | LoRA adapters, benchmarks, evolution |

**VR mapping:** Profile = walking up to a persona's desk/room. Persona brain = entering their mindscape (abstract visualization of cognition). DM = private conversation bubble.

### Temporary Modes

Overlay on any room. Triggered by user action. Returns to previous mode.

| Mode | URL | Trigger | Center Widget | Description |
|------|-----|---------|---------------|-------------|
| Live | `/live/{room}` | Call button | live-widget | Voice/video call with avatars |

**VR mapping:** Live mode = everyone's avatars animate, spatial audio activates, hand tracking enables gestures. The room doesn't change — just becomes more alive.

## URL Scheme

```
/{view}/{entity}    — entity-backed view (room or user)
/{view}             — singleton (no entity)
```

### Complete URL Table

| URL | What You See |
|-----|-------------|
| `/chat/general` | General room, chat mode |
| `/chat/pantheon` | Pantheon room, chat mode |
| `/chat/code` | Code room, chat mode |
| `/chat/academy` | Academy room, chat mode |
| `/chat/dev-updates` | Dev Updates room, chat mode |
| `/chat/grid-ops` | Grid Ops room, chat mode |
| `/chat/dm-helper` | DM with Helper AI |
| `/chat/factory` | Factory room, chat mode (override) |
| `/chat/universe` | Universe room, chat mode (override) |
| `/live/general` | General room, live call |
| `/live/pantheon` | Pantheon room, live call |
| `/live/factory` | Factory room, live call |
| `/universe` | Universe room, default mode (theme picker) |
| `/factory` | Factory room, default mode (forge pipeline) |
| `/training` | Training room, default mode (dashboard) |
| `/settings` | Settings room, default mode (config) |
| `/grid` | Grid overview (singleton) |
| `/browser` | Web browser (singleton) |
| `/terminal` | Terminal (singleton) |
| `/help` | Help/docs (singleton) |
| `/logs` | Log viewer (singleton) |
| `/canvas` | Drawing canvas (singleton) |
| `/profile/helper` | Helper AI profile |
| `/persona/helper` | Helper AI brain/cognition |
| `/genome/helper` | Helper AI genome/adapters |

## AI Teams

```json
{
  "general": null,                              // everyone
  "pantheon": null,                             // everyone (SOTA focus)
  "code": ["helper", "codereview", "claude"],   // coding specialists
  "academy": ["helper", "teacher"],             // education
  "factory": ["helper", "codereview"],          // forge specialists
  "universe": ["helper"],                       // + visual AI future
  "training": ["helper", "teacher"],            // training specialists
  "settings": ["helper"],                       // system help
  "dev-updates": ["helper"],                    // changelog context
  "grid-ops": ["helper"]                        // compute help
}
```

`null` = all personas join. Otherwise, only listed personas + human owner.
Helper AI is ALWAYS included (the concierge).

## Recipe JSON Structure

Every recipe MUST have:

```json
{
  "uniqueId": "factory",           // REQUIRED — content type key
  "name": "Model Factory",         // REQUIRED — display name
  "view": "factory",               // REQUIRED — URL prefix (verb)
  "entityType": "room",            // REQUIRED — room | user | null
  "layout": {                      // REQUIRED
    "widgets": [
      { "widget": "factory-widget", "position": "center", "order": 0 },
      { "widget": "chat-widget", "position": "right", "order": 0 }
    ]
  },
  "team": ["helper", "codereview"],  // WHO responds in this room
  "modes": ["forge", "chat", "live"] // WHAT views are available
}
```

No optional fields for mission-critical properties. The generator validates and FAILS if any required field is missing.

## Extending for VR

In VR, each room is a 3D scene. The recipe gains spatial properties:

```json
{
  "uniqueId": "factory",
  "scene": {
    "template": "industrial-forge",    // 3D scene template
    "lighting": "warm-orange",         // ambient mood
    "avatarPositions": "workbench",    // where personas stand
    "interactables": [
      { "widget": "factory-widget", "placement": "wall-screen" },
      { "widget": "chat-widget", "placement": "hud-overlay" }
    ]
  }
}
```

The 2D recipe IS the VR recipe. The `layout.widgets` become `scene.interactables`. The `position: center` becomes `placement: wall-screen`. The `position: right` becomes `placement: hud-overlay`. Same data, different renderer.

## What Needs Building

### Phase 1 — Wire existing pieces
1. Right panel shows THAT room's chat (not General/Academy)
2. Team field on recipes, seeder adds team members to rooms
3. Tab state persistence fixed (no random resets)
4. Stale content type cleanup (handle old data gracefully)

### Phase 2 — Mode switching
5. Mode switch button in room header
6. Live mode overlay (already works, just needs clean transition)
7. URL updates on mode switch
8. Return-to-previous-mode after call ends

### Phase 3 — VR readiness
9. Scene templates on recipes
10. Avatar spatial positioning
11. Widget → interactable mapping
12. Spatial audio zones
