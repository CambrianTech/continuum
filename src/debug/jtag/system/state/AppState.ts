/**
 * AppState - Centralized application state using signals
 *
 * This is the SINGLE SOURCE OF TRUTH for UI state.
 * All widgets subscribe to this store, not to events.
 *
 * Pattern: useState/Redux-like centralized state
 * - signal() for observable values
 * - computed() for derived values
 * - batch() for atomic multi-value updates
 *
 * @example
 * ```typescript
 * import { AppState } from '@system/state/AppState';
 *
 * // Subscribe to content changes
 * effect(() => {
 *   const room = AppState.currentEntityId.value;
 *   console.log('Room changed to:', room);
 * });
 *
 * // Change room (automatically notifies all subscribers)
 * AppState.openContent('chat', 'general');
 * ```
 */

import { signal, computed, batch } from '@preact/signals-core';

/**
 * Content item in the tab bar (VS Code-style tabs)
 */
export interface ContentItem {
  id: string;
  type: string;  // 'chat', 'settings', 'theme', etc.
  entityId?: string;  // Room ID, settings page, etc.
  displayName?: string;  // Tab label
  closeable?: boolean;  // Can user close this tab?
}

/**
 * Resolved entity info (from database lookup)
 */
export interface ResolvedEntity {
  id: string;
  displayName: string;
  uniqueId: string;
  [key: string]: unknown;
}

/**
 * PageState for widget communication
 */
export interface PageState {
  contentType: string;
  entityId: string | null;
  resolved: ResolvedEntity | null;
}

// ============================================================================
// CORE STATE SIGNALS
// ============================================================================

/** Current content type being displayed (chat, settings, theme, etc.) */
const currentContentType = signal<string>('chat');

/** Current entity ID (room UUID/uniqueId, settings page name, etc.) */
const currentEntityId = signal<string | null>('general');

/** Resolved entity info (after database lookup) */
const resolvedEntity = signal<ResolvedEntity | null>(null);

/** Open tabs in the tab bar */
const openTabs = signal<ContentItem[]>([
  { id: 'general', type: 'chat', entityId: 'general', displayName: 'General', closeable: false }
]);

/** Currently active tab ID */
const activeTabId = signal<string | null>('general');

/** Is a navigation in progress? */
const isNavigating = signal<boolean>(false);

// ============================================================================
// COMPUTED VALUES
// ============================================================================

/** Currently active tab (derived from openTabs + activeTabId) */
const currentTab = computed<ContentItem | null>(() => {
  const tabs = openTabs.value;
  const id = activeTabId.value;
  return tabs.find(t => t.id === id) ?? null;
});

/** Current pageState for widget compatibility */
const pageState = computed<PageState>(() => ({
  contentType: currentContentType.value,
  entityId: currentEntityId.value,
  resolved: resolvedEntity.value
}));

// ============================================================================
// ACTIONS (Atomic state updates)
// ============================================================================

/**
 * Switch to an existing tab
 */
function switchTab(tabId: string): void {
  const tab = openTabs.value.find(t => t.id === tabId);
  if (!tab) return;

  batch(() => {
    activeTabId.value = tabId;
    currentContentType.value = tab.type;
    currentEntityId.value = tab.entityId ?? null;
    // Note: resolvedEntity will be updated by the widget after lookup
  });
}

/**
 * Open content (creates tab if needed, switches to it)
 */
function openContent(type: string, entityId?: string, displayName?: string): void {
  batch(() => {
    // Find existing tab for this content
    const existingTab = openTabs.value.find(t =>
      t.type === type && t.entityId === entityId
    );

    if (existingTab) {
      // Switch to existing tab
      activeTabId.value = existingTab.id;
    } else {
      // Create new tab
      const newTab: ContentItem = {
        id: entityId ?? `${type}-${Date.now()}`,
        type,
        entityId,
        displayName: displayName ?? entityId ?? type,
        closeable: true
      };
      openTabs.value = [...openTabs.value, newTab];
      activeTabId.value = newTab.id;
    }

    currentContentType.value = type;
    currentEntityId.value = entityId ?? null;
    resolvedEntity.value = null; // Will be resolved by widget
  });
}

/**
 * Close a tab
 */
function closeTab(tabId: string): void {
  const tabs = openTabs.value;
  const tabIndex = tabs.findIndex(t => t.id === tabId);
  if (tabIndex === -1) return;

  const tab = tabs[tabIndex];
  if (tab.closeable === false) return; // Can't close uncloseable tabs

  batch(() => {
    // Remove the tab
    openTabs.value = tabs.filter(t => t.id !== tabId);

    // If we closed the active tab, switch to adjacent
    if (activeTabId.value === tabId) {
      const newTabs = openTabs.value;
      const newIndex = Math.min(tabIndex, newTabs.length - 1);
      const newTab = newTabs[newIndex];
      if (newTab) {
        activeTabId.value = newTab.id;
        currentContentType.value = newTab.type;
        currentEntityId.value = newTab.entityId ?? null;
      }
    }
  });
}

/**
 * Set resolved entity (called by widgets after DB lookup)
 */
function setResolvedEntity(entity: ResolvedEntity | null): void {
  resolvedEntity.value = entity;

  // Update tab display name if we have a resolution
  if (entity && activeTabId.value) {
    const tabs = openTabs.value;
    const tabIndex = tabs.findIndex(t => t.id === activeTabId.value);
    if (tabIndex !== -1 && !tabs[tabIndex].displayName) {
      const updatedTabs = [...tabs];
      updatedTabs[tabIndex] = {
        ...updatedTabs[tabIndex],
        displayName: entity.displayName
      };
      openTabs.value = updatedTabs;
    }
  }
}

/**
 * Update tab display name
 */
function updateTabName(tabId: string, displayName: string): void {
  const tabs = openTabs.value;
  const tabIndex = tabs.findIndex(t => t.id === tabId);
  if (tabIndex === -1) return;

  const updatedTabs = [...tabs];
  updatedTabs[tabIndex] = { ...updatedTabs[tabIndex], displayName };
  openTabs.value = updatedTabs;
}

/**
 * Initialize state from user state (on app load)
 */
function initFromUserState(contentState: {
  openItems?: ContentItem[];
  currentItemId?: string;
}): void {
  batch(() => {
    if (contentState.openItems?.length) {
      openTabs.value = contentState.openItems;
    }
    if (contentState.currentItemId) {
      const tab = openTabs.value.find(t => t.id === contentState.currentItemId);
      if (tab) {
        activeTabId.value = tab.id;
        currentContentType.value = tab.type;
        currentEntityId.value = tab.entityId ?? null;
      }
    }
  });
}

/**
 * Set navigation state
 */
function setNavigating(value: boolean): void {
  isNavigating.value = value;
}

// ============================================================================
// EXPORT
// ============================================================================

export const AppState = {
  // Signals (read with .value, or use in effect())
  currentContentType,
  currentEntityId,
  resolvedEntity,
  openTabs,
  activeTabId,
  isNavigating,

  // Computed
  currentTab,
  pageState,

  // Actions
  switchTab,
  openContent,
  closeTab,
  setResolvedEntity,
  updateTabName,
  initFromUserState,
  setNavigating,

  // For debugging
  debug() {
    console.group('AppState Debug');
    console.log('currentContentType:', currentContentType.value);
    console.log('currentEntityId:', currentEntityId.value);
    console.log('resolvedEntity:', resolvedEntity.value);
    console.log('openTabs:', openTabs.value);
    console.log('activeTabId:', activeTabId.value);
    console.log('currentTab:', currentTab.value);
    console.groupEnd();
  }
} as const;

// Also export for convenient destructuring
export {
  currentContentType,
  currentEntityId,
  resolvedEntity,
  openTabs,
  activeTabId,
  isNavigating,
  currentTab,
  pageState,
  switchTab,
  openContent,
  closeTab,
  setResolvedEntity,
  updateTabName,
  initFromUserState,
  setNavigating
};
