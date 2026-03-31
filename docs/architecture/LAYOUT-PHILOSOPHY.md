# Layout Philosophy: Left / Center / Right

The three-panel layout follows IDE conventions but with recipe-scoped tooling.

## The Three Panels

| Panel | Scope | What It Shows | Analogy |
|-------|-------|---------------|---------|
| **Left** | **Global** | Navigation, rooms, agents, conversations. Same across all recipes. | File browser |
| **Center** | **Activity** | The primary content — forge controls, video call, chat, canvas. The user's focus. | Editor / document |
| **Right** | **Recipe-scoped** | Context tools for THIS activity. Changes per recipe. Inspects and augments the center. | Inspector / properties panel |

## The Key Insight

The **right bar is recipe-scoped**. This is the distinction that makes the layout useful instead of cluttered.

The recipe IS the content type — like a MIME type for the UI. It declares what the center shows, and what tools belong in the right panel for that context. A chat recipe has different right-panel tools than a factory recipe or a live call recipe.

```
recipe = "factory"
  center: factory-widget (forge controls, published models, active forges)
  right:  factory-stats-widget (leaderboard, filters, device coverage, alloy status)

recipe = "live"
  center: live-call-widget (video grid, 3D avatars)
  right:  participant-list, audio-controls, caption-settings

recipe = "chat"
  center: chat-widget (conversation)
  right:  thread-details, shared-files, member-list

recipe = "academy"
  center: academy-widget (training session)
  right:  progress-tracker, exam-scores, adapter-status
```

## Rules

1. **Left panel widgets are persistent** — they survive recipe changes. Room list, agent list, navigation.

2. **Center is one primary widget** — the focus of the activity. Never split the center into multiple competing widgets.

3. **Right panel is optional** — `right: null` is valid. Not every recipe needs inspector tools. But when the center gets complex (factory, live calls, training), the right panel prevents the center from becoming bloated.

4. **Right panel widgets load data independently** — they're not children of the center widget. They query the same commands/events but render their own view. This keeps widgets decoupled.

5. **Right panel responds to center state** — if the user selects a model in the center, the right panel shows that model's details. Communication happens through events, not direct coupling.

6. **Mobile collapses right into a drawer** — on narrow viewports, the right panel becomes a slide-out drawer or a tab. The information is still accessible but doesn't compete for space.

## Anti-Patterns

- **Empty right bar** — if a recipe doesn't use the right panel, that's fine. But if the center widget is overloaded with stats, filters, and secondary controls, those belong in the right panel.

- **Global tools in the right bar** — settings, help, user profile — these are global (left panel or header). The right bar is strictly recipe-scoped.

- **Center doing double duty** — the center should be the activity, not the activity + the toolbox. If the center widget has collapsible sidebars or tabs for secondary info, that info probably belongs in the right panel.

## Implementation

Recipes declare the layout:

```json
{
  "layout": {
    "main": ["factory-widget"],
    "right": {
      "widgets": ["factory-stats-widget"]
    }
  }
}
```

The layout engine (`RecipeLayoutService`) reads this and places widgets in the correct panels. Widgets self-register via `customElements.define`. The recipe connects them.

## Future: Moveable Widgets

Users should eventually be able to drag widgets between panels — move the stats from right to a floating window, or collapse the right panel and expand the center. The recipe defines the default, the user customizes. State persists per-user per-recipe.
