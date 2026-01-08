# Widget Reactive Conversion Plan

## Problem Statement

Widgets using `.innerHTML =` for updates cause:
1. **Child widget destruction** - Container widgets wipe nested custom elements
2. **Lost state** - Form inputs lose focus, scroll positions reset
3. **Animation breaks** - CSS animations restart on every update
4. **Memory leaks** - Event listeners on destroyed elements become orphaned
5. **Performance issues** - Full DOM rebuilds instead of targeted updates

## Solution Pattern

Replace `.innerHTML =` with **reactive patterns**:

### For Containers (slot pattern)
```typescript
// ❌ BAD - destroys children
container.innerHTML = `<child-widget></child-widget>`;

// ✅ GOOD - swap only the dynamic slot
if (this._dynamicSlot) {
  this._dynamicSlot.innerHTML = '';
  this._dynamicSlot.appendChild(this.createWidgetElement(widget));
}
```

### For Content Updates (targeted DOM manipulation)
```typescript
// ❌ BAD - destroys everything
element.innerHTML = `<div class="status">${status}</div>`;

// ✅ GOOD - update only what changed
const statusEl = element.querySelector('.status');
if (statusEl) statusEl.textContent = status;
```

### For Lists (reconciliation)
```typescript
// ❌ BAD - rebuilds entire list
list.innerHTML = items.map(i => `<item>${i}</item>`).join('');

// ✅ GOOD - reconcile existing items
this.reconcileList(list, items, (item) => this.createItemElement(item));
```

---

## Widgets Requiring Conversion

### Priority 0: Critical - Container Widgets
These wipe child widgets on update. **Must fix first.**

| Widget | File | Line(s) | Status | Notes |
|--------|------|---------|--------|-------|
| SidebarWidget | `widgets/sidebar/SidebarWidget.ts` | 149 | ✅ DONE | Fixed with slot pattern |
| MainWidget | `widgets/main/MainWidget.ts` | 205 | ✅ DONE | Already had widget cache; removed dead legacy code |
| ContinuumWidget | `widgets/continuum/ContinuumWidget.ts` | 97 | ✅ OK | Initial render only, no dynamic updates |
| PanelLayoutWidget | `widgets/shared/PanelLayoutWidget.ts` | 59 | ✅ OK | Uses native `<slot>`, children survive |
| BasePanelWidget | `widgets/shared/BasePanelWidget.ts` | 113 | ✅ OK | Subclasses render plain HTML only, no custom elements |
| BaseSidePanelWidget | `widgets/shared/BaseSidePanelWidget.ts` | 83, 98 | ✅ OK | Initial render only; RightPanelWidget uses setAttribute for updates |
| BaseContentWidget | `widgets/shared/BaseContentWidget.ts` | 444 | ✅ OK | No subclasses exist; renderEmptyState is plain HTML |

### Priority 1: Medium - Dynamic Update Widgets (Performance Only)
These use innerHTML for plain HTML content - no custom elements destroyed.
**Status: All audited, safe from widget destruction. Performance optimization only.**

| Widget | File | Line(s) | Status | Notes |
|--------|------|---------|--------|-------|
| ChatWidget | `widgets/chat/chat-widget/ChatWidget.ts` | 322, 1074 | ✅ OK | Line 322 creates NEW elements; Line 1074 updates plain HTML header |
| AIStatusIndicator | `widgets/chat/chat-widget/AIStatusIndicator.ts` | 325 | ✅ OK | Plain HTML status (icon, text, button) |
| ContinuumEmoterWidget | `widgets/continuum-emoter/ContinuumEmoterWidget.ts` | 290, 310 | ✅ OK | Plain HTML status text |
| ChartRenderer | `widgets/cognition-histogram/ChartRenderer.ts` | 93, 109 | ✅ OK | Legend text + SVG clearing |
| ThemeWidget | `widgets/shared/ThemeWidget.ts` | 100, 125 | ✅ OK | Initial render + theme card grid |
| UserProfileWidget | `widgets/user-profile/UserProfileWidget.ts` | 219-254 | ✅ OK | Initial render + profile sections |

### Priority 2: Low - Initial Render Only (No Action Needed)
These only use innerHTML for initial shadowRoot setup. **All verified safe.**

| Widget | File | Line(s) | Status | Notes |
|--------|------|---------|--------|-------|
| BaseWidget | `widgets/shared/BaseWidget.ts` | 633 | ✅ OK | Initial shadowRoot setup only |
| CognitionHistogramWidget | `widgets/cognition-histogram/CognitionHistogramWidget.ts` | 253 | ✅ OK | Initial only |
| SettingsWidget | `widgets/settings/SettingsWidget.ts` | 185 | ✅ OK | Initial only |
| ContentTabsWidget | `widgets/content-tabs/ContentTabsWidget.ts` | 168 | ✅ OK | Initial only |
| DrawingCanvasWidget | `widgets/drawing-canvas/DrawingCanvasWidget.ts` | 304 | ✅ OK | Initial only |
| HeaderControlsWidget | `widgets/header-controls/HeaderControlsWidget.ts` | 130 | ✅ OK | Initial only |
| EntityListWidget | `widgets/shared/EntityListWidget.ts` | 66 | ✅ OK | Initial only |
| SettingsAssistantWidget | `widgets/settings/SettingsAssistantWidget.ts` | 163 | ✅ OK | Initial only |
| ChatWidgetBase | `widgets/chat/shared/ChatWidgetBase.ts` | 42 | ✅ OK | Initial only |
| PositronCursorWidget | `widgets/positron-cursor/PositronCursorWidget.ts` | 292 | ✅ OK | Initial only |

---

## Implementation Strategy

### Phase 1: Container Widgets (P0)
1. **MainWidget** - Most impactful, content area switching
2. **Base classes** (BasePanelWidget, BaseSidePanelWidget, BaseContentWidget)
3. **ContinuumWidget** - App shell

**Pattern**: Render structure once, use slots for dynamic content.

### Phase 2: Dynamic Widgets (P1)
1. **ChatWidget** - High-traffic, frequent updates
2. **ChartRenderer** - SVG manipulation
3. **Status widgets** (AIStatusIndicator, ContinuumEmoterWidget)

**Pattern**: Targeted DOM updates, element caching.

### Phase 3: Audit Medium Priority (P2)
Review each widget to determine if conversion needed:
- If only initial render → May be acceptable
- If has `requestUpdate()` or re-render logic → Needs conversion

---

## Conversion Checklist

For each widget conversion:

- [ ] Identify all `.innerHTML =` usages
- [ ] Categorize: initial render vs dynamic update
- [ ] For containers: implement slot pattern
- [ ] For updates: implement targeted DOM manipulation
- [ ] Add element caching for frequently accessed elements
- [ ] Remove redundant `requestUpdate()` calls
- [ ] Test: verify child widgets survive updates
- [ ] Test: verify animations don't restart
- [ ] Test: verify form state preserved

---

## Testing Protocol

After each conversion:

```bash
# 1. Build
npm run build:ts

# 2. Deploy
npm start

# 3. Visual verification
./jtag interface/screenshot

# 4. Navigation test (for containers)
# Switch between tabs rapidly, verify no crashes

# 5. Animation test (for status widgets)
# Trigger AI activity, verify comet animations don't restart
```

---

## Success Metrics

- [x] No `.innerHTML =` in container widgets (P0 complete) ✅ 2026-01-08
- [x] No animation restarts on tab switch ✅ 2026-01-08 (GLOBAL_LAYOUT widgets persist, setAttribute for updates)
- [x] No form focus loss on updates ✅ 2026-01-08 (Lit diffing + GLOBAL_LAYOUT persistence)
- [x] Heap memory stable during rapid navigation ✅ 2026-01-08 (async state + batching)
- [x] All 7 P0 widgets audited ✅ 2026-01-08 (2 fixed, 5 already safe)
- [x] All 6 P1 widgets audited ✅ 2026-01-08 (all safe - plain HTML only)
- [x] All 10 P2 widgets audited ✅ 2026-01-08 (all initial render only)

## Async State System (From Plan)

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | RoomListWidget - use pageState, remove self-subscription | ✅ Done |
| 2 | ReactiveEntityScrollerWidget - batchedUpdate() | ✅ Done |
| 3 | Event transaction guards | ✅ Done (EventGuard.ts) |
| 4 | ReactiveState - async notifications | ✅ Done |
| 5 | AsyncStorage - non-blocking localStorage | ✅ Done |
| 6 | pageState single source of truth | ✅ Done |
| 7 | Event serializer for concurrent safety | ✅ Done (EventGuard.ts) |

---

## Related Documents

- [Plan: Infinite Recursion & Async State](~/.claude/plans/cozy-knitting-blossom.md)
- [Architecture Rules](docs/ARCHITECTURE-RULES.md)

---

## Progress Log

| Date | Widget | Status | Notes |
|------|--------|--------|-------|
| 2026-01-08 | SidebarWidget | ✅ DONE | Slot pattern for dynamic widget |
| 2026-01-08 | MainWidget | ✅ DONE | Already used widget cache; removed dead legacy code |
| 2026-01-08 | ContinuumWidget | ✅ OK | Initial render only |
| 2026-01-08 | PanelLayoutWidget | ✅ OK | Native `<slot>` pattern |
| 2026-01-08 | BasePanelWidget | ✅ OK | Subclasses render plain HTML |
| 2026-01-08 | BaseSidePanelWidget | ✅ OK | Initial render; RightPanelWidget uses setAttribute |
| 2026-01-08 | BaseContentWidget | ✅ OK | No subclasses; plain HTML fallback |
| 2026-01-08 | ChatWidget | ✅ OK | Creates new elements; header is plain HTML |
| 2026-01-08 | AIStatusIndicator | ✅ OK | Plain HTML status content |
| 2026-01-08 | ContinuumEmoterWidget | ✅ OK | Plain HTML status text |
| 2026-01-08 | ChartRenderer | ✅ OK | Legend + SVG plain HTML |
| 2026-01-08 | ThemeWidget | ✅ OK | Initial + theme cards plain HTML |
| 2026-01-08 | UserProfileWidget | ✅ OK | Initial + profile sections plain HTML |
| 2026-01-08 | **P2 Audit Complete** | ✅ OK | All 10 widgets - initial render only |
| 2026-01-08 | ContentStateService | ✅ DONE | Async notifications via scheduleNotify() |
| 2026-01-08 | AsyncStorage.ts | ✅ DONE | Non-blocking localStorage wrapper |
| 2026-01-08 | ChatMessageCache | ✅ DONE | Migrated to asyncStorage |
| 2026-01-08 | LocalStorageStateManager | ✅ DONE | Migrated to asyncStorage |
| 2026-01-08 | EventGuard.ts | ✅ DONE | Event guards + serializer utility |
| 2026-01-08 | **All Phases Complete** | ✅ DONE | All 7 async state phases implemented |

