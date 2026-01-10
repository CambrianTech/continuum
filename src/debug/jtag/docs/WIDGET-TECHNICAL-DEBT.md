# Widget Technical Debt - MUST FIX

**Branch:** `feature/widget-overhaul-vite`
**Date:** 2026-01-10
**Status:** IN PROGRESS

## Critical Issues Causing Performance Problems

### 1. innerHTML Usage (20 instances) - BREAKS REACTIVE STATE

Using `innerHTML =` destroys DOM state, breaks focus, causes re-renders, and bypasses Lit's efficient diffing.

| File | Count | Priority |
|------|-------|----------|
| `ChatWidget.ts` | 2 | HIGH |
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

## Migration Priority Order

1. **CRITICAL:** Fix MainWidget/BaseWidget timeouts (white screen issue)
2. **HIGH:** ChatWidget innerHTML removal
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
