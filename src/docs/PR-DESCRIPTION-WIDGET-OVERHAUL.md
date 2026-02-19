# PR: Widget Overhaul - Vite Migration & Reactive State

## Summary

Major overhaul of widget system to use Lit's reactive state properly, eliminate innerHTML usage, and fix performance issues causing 30+ second load times.

## Changes Completed

### Performance Fixes
- Remove LocalStorageDataBackend logging (was causing 42k+ console messages)
- Optimize ThemeWidget CSS loading with parallel Promise.all requests
- Fix sidebar gap by conditionally rendering empty dynamic slot
- Remove user-list-widget height constraints for proper flex

### Reactive State Improvements
- Add `shouldAddEntity` filtering pattern to ReactiveListWidget base class
- RoomListWidget filters out system rooms and DMs via shouldAddEntity
- Add reactive scroll-to-selected via Lit's `updated()` lifecycle
- UserListWidget filter chips with reactive state

### Layout Fixes
- Sidebar flex priority (rooms 0.3, users 1.0)
- User list fills available space
- Room list scroll-to-selected on content open

## Known Technical Debt (See WIDGET-TECHNICAL-DEBT.md)

### CRITICAL - Daemon Architecture (Root Cause of 30+ Second Delays)

The daemon system uses synchronous initialization that blocks everything:

```typescript
// DaemonBase.ts - THE PROBLEM
async initializeDaemon(): Promise<void> {
  await this.initialize();  // BLOCKS - 18 daemons × avg 500ms = 9+ seconds
}
```

**Required Fix: OS Kernel-Style Architecture**

| Current (Web Dev) | Fix (OS/RTOS) |
|-------------------|---------------|
| `await this.initialize()` | Non-blocking `start()` with state machine |
| Boolean `isReady` | Process states: `created→starting→ready→failed→stopped` |
| `setTimeout(check, 100)` | Event-driven signals via EventEmitter |
| Sequential DB queries | Return immediately, resolve async in background |

**Files to Refactor:**
- `daemons/command-daemon/shared/DaemonBase.ts` - Add lifecycle states, message queuing
- `system/core/system/shared/JTAGSystem.ts` - Non-blocking daemon orchestration
- `daemons/session-daemon/server/SessionDaemonServer.ts` - Fast session creation
- `commands/state/get/server/StateGetServerCommand.ts` - State caching

**Rust Migration Path:** Daemons can be migrated to Rust with TypeScript wrappers maintaining the same interface.

---

### Widget Technical Debt

#### CRITICAL - Causes 30+ Second Load Times
- [ ] MainWidget setTimeout hacks
- [ ] BaseWidget polling loop (`setTimeout(checkReady, 100)`)

### HIGH - innerHTML Breaking State
- [ ] ChatWidget.ts (2 places)
- [ ] AIStatusIndicator.ts
- [ ] ChatWidgetBase.ts
- [ ] BaseWidget.ts

### MEDIUM - Needs ReactiveWidget Migration
- [ ] ContinuumEmoterWidget
- [ ] DrawingCanvasWidget
- [ ] ThemeWidget
- [ ] UserProfileWidget
- [ ] BaseContentWidget
- [ ] BasePanelWidget
- [ ] EntityListWidget

## Test Plan

- [ ] Page loads in < 2 seconds
- [ ] Tab switching is instant (not 10-30 seconds)
- [ ] AI status indicators update when AIs respond
- [ ] Filter chips filter the user list
- [ ] Room list scrolls to selected room
- [ ] No console errors on load
- [ ] Refresh doesn't show 30+ second white screen

## Breaking Changes

None - all changes are internal refactoring.

## Related Issues

- Performance regression on page load
- Tab switching delays
- AI status not updating in real-time

---
Generated with [Claude Code](https://claude.com/claude-code)
