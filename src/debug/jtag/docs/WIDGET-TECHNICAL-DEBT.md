# Technical Debt - MUST FIX

**Branch:** `feature/widget-overhaul-vite`
**Date:** 2026-01-10
**Status:** IN PROGRESS

---

## CRITICAL: Daemon Architecture (Root Cause of 30+ Second Delays)

The synchronous daemon initialization pattern is the root cause of startup delays.

### The Problem

```typescript
// DaemonBase.ts:64-70
async initializeDaemon(): Promise<void> {
  this.router.registerSubscriber(this.subpath, this);
  await this.initialize();  // <-- BLOCKS EVERYTHING
}
```

Every daemon waits for the previous to fully initialize. 18 daemons × avg 500ms = 9+ seconds minimum.

### The Fix: OS Kernel-Style Architecture

Daemons should behave like systemd units, not Express middleware.

| OS Concept | Current (Bad) | Fix |
|------------|---------------|-----|
| Process States | `isReady: boolean` | `created→starting→ready→failed→stopped` |
| Blocking | `await this.initialize()` | `start()` returns immediately |
| Polling | `setTimeout(check, 100)` | Event-driven via EventEmitter |
| Queuing | None - fails if not ready | Queue messages while starting |
| Dependencies | Timing guesses | Explicit `dependencies: string[]` |

### Files to Modify

| File | Change | Priority |
|------|--------|----------|
| `daemons/command-daemon/shared/DaemonBase.ts` | Lifecycle states, message queue | CRITICAL |
| `system/core/system/shared/JTAGSystem.ts` | Non-blocking orchestration | CRITICAL |
| `daemons/session-daemon/server/SessionDaemonServer.ts` | Fast session creation | HIGH |
| `commands/state/get/server/StateGetServerCommand.ts` | State caching | HIGH |

### Rust Migration Path

Daemons can be migrated to Rust with TypeScript wrappers:
- TypeScript maintains interface compatibility
- Rust handles heavy lifting via Unix socket
- Zero-copy for performance-critical paths

---

## Widget-Specific Issues

## Critical Issues Causing Performance Problems

### 1. innerHTML Usage (20 instances) - BREAKS REACTIVE STATE

Using `innerHTML =` destroys DOM state, breaks focus, causes re-renders, and bypasses Lit's efficient diffing.

| File | Count | Priority |
|------|-------|----------|
| `ChatWidget.ts` | 1* | DONE (partial) |
| `AIStatusIndicator.ts` | 1 | HIGH |
| `ChatWidgetBase.ts` | 1 | HIGH |
| `ContinuumEmoterWidget.ts` | 2 | MEDIUM |
| `DrawingCanvasWidget.ts` | 1 | LOW |
| `BaseContentWidget.ts` | 1 | MEDIUM |
| `BasePanelWidget.ts` | 1 | MEDIUM |
| `BaseSidePanelWidget.ts` | 1 | MEDIUM |
| `BaseWidget.ts` | 1 | HIGH |
| `EntityListWidget.ts` | 1 | MEDIUM |
| `ThemeWidget.ts` | 2 | MEDIUM |
| `UserProfileWidget.ts` | 4 | MEDIUM |

**Fix:** Replace all `innerHTML =` with Lit `html\`\`` templates and `render()` method.

### 2. setTimeout/setInterval Hacks (26 instances) - CAUSES 30+ SECOND DELAYS

These timeouts block rendering and cause white screen on refresh.

| File | Issue | Priority |
|------|-------|----------|
| `MainWidget.ts` | `setTimeout(fn, 0)` and delayed init | **CRITICAL** |
| `BaseWidget.ts` | `setTimeout(checkReady, 100)` polling loop | **CRITICAL** |
| `ChatWidget.ts` | Debounce timeouts for header/positron | HIGH |
| `AIStatusIndicator.ts` | Multiple animation timeouts | MEDIUM |
| `ContinuumEmoterWidget.ts` | Animation timeouts | LOW |
| `WebViewWidget.ts` | Multiple loading timeouts | MEDIUM |
| `SettingsWidget.ts` | Artificial 500ms delay | MEDIUM |
| `EntityScroller.ts` | Idle timeout | LOW |
| `PositronCursorWidget.ts` | Hide timeout | LOW |

**Fix:** Replace with:
- Lit lifecycle methods (`firstUpdated`, `updated`)
- `@reactive()` state that triggers re-renders
- `requestAnimationFrame` for animations
- Proper async/await patterns

### 3. BaseWidget Usage (9 widgets) - NOT REACTIVE

These widgets extend `BaseWidget` instead of `ReactiveWidget`, missing Lit's reactive system.

| Widget | Status |
|--------|--------|
| `ChatWidgetBase.ts` | NEEDS MIGRATION |
| `ContinuumEmoterWidget.ts` | NEEDS MIGRATION |
| `DrawingCanvasWidget.ts` | NEEDS MIGRATION |
| `BaseContentWidget.ts` | NEEDS MIGRATION |
| `BasePanelWidget.ts` | NEEDS MIGRATION |
| `CollaborativeActivityWidget.ts` | NEEDS MIGRATION |
| `EntityListWidget.ts` | NEEDS MIGRATION |
| `ThemeWidget.ts` | NEEDS MIGRATION |
| `UserProfileWidget.ts` | NEEDS MIGRATION |

**Fix:** Migrate to `ReactiveWidget` with:
- `@reactive()` decorators for state
- `render()` method returning `html\`\``
- SCSS styles via `static styles = [unsafeCSS(externalStyles)]`

## Already Fixed (This PR)

- [x] `UserListWidget` - Filter chips, reactive state
- [x] `RoomListWidget` - shouldAddEntity filtering, scroll-to-selected
- [x] `SidebarWidget` - Flex layout, conditional rendering
- [x] `ReactiveListWidget` - Base class with shouldAddEntity pattern
- [x] `LocalStorageDataBackend` - Removed logging spam
- [x] `ThemeWidget` - Parallel file loading
- [x] `ChatWidget.ts` - Header uses targeted DOM updates, message static structure uses DOM APIs
  - *Remaining: adapter content still uses innerHTML (adapters return HTML strings - separate refactor needed)

## Migration Priority Order

1. **CRITICAL:** Fix MainWidget/BaseWidget timeouts (white screen issue)
2. ~~**HIGH:** ChatWidget innerHTML removal~~ ✅ DONE (static structure, adapter content remains)
3. **HIGH:** AIStatusIndicator innerHTML removal
4. **MEDIUM:** Migrate remaining BaseWidget widgets
5. **LOW:** Clean up animation timeouts

## The Pattern to Follow

### Before (BAD):
```typescript
class MyWidget extends BaseWidget {
  async connectedCallback() {
    await super.connectedCallback();
    this.shadowRoot.innerHTML = `<div>${this.data}</div>`;
    setTimeout(() => this.update(), 100);
  }
}
```

### After (GOOD):
```typescript
class MyWidget extends ReactiveWidget {
  @reactive() data: string = '';

  static styles = [unsafeCSS(externalStyles)];

  render() {
    return html`<div>${this.data}</div>`;
  }
}
```

## Testing Checklist

Before merging, verify:
- [ ] Page loads in < 2 seconds (not 30+)
- [ ] Tab switching is instant
- [ ] AI status indicators update live
- [ ] Filter chips work
- [ ] No console errors
- [ ] Genome panels animate on AI activity
