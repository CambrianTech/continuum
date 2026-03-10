/**
 * RightPanelWidget - IDE-style accordion sidebar
 *
 * Hosts multiple stacked sections, each containing a widget.
 * Sections are collapsible, and any section can be "popped out"
 * to open as a full tab in the main content area.
 *
 * When a content type declares sections in its RightPanelConfig,
 * this widget renders them as an accordion. Legacy configs
 * (single widget/room/compact) are auto-converted to one section.
 *
 * Listens to UI_EVENTS.RIGHT_PANEL_CONFIGURE to reconfigure sections.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import { Events } from '../../system/core/shared/Events';
import { UI_EVENTS, type RightPanelConfigPayload, type RightPanelSectionPayload } from '../../system/core/shared/EventConstants';
import { ContentOpen } from '../../commands/collaboration/content/open/shared/ContentOpenTypes';
import type { ContentType } from '../../system/data/entities/UserStateEntity';
import { styles as SIDE_PANEL_STYLES } from '../shared/styles/side-panel.styles';

const COLLAPSE_STORAGE_KEY = 'continuum-right-panel-collapse-state';

// === STYLES (must precede class for static styles reference) ===

const RIGHT_PANEL_STYLES = `
  :host {
    clip-path: inset(0);
    box-sizing: border-box;
    min-width: 0;
  }

  .sections-container {
    position: absolute;
    top: 35px;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .empty-sidebar {
    padding: 16px;
    text-align: center;
    color: var(--content-secondary, #777);
    font-size: 11px;
    font-style: italic;
  }

  /* === Accordion Section === */

  .accordion-section {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border-bottom: 1px solid rgba(60, 80, 100, 0.3);
    transition: flex 0.2s ease;
  }

  .accordion-section.collapsed {
    flex: 0 0 auto;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    cursor: pointer;
    user-select: none;
    background: rgba(10, 20, 30, 0.6);
    border-bottom: 1px solid rgba(60, 80, 100, 0.2);
    min-height: 28px;
    flex-shrink: 0;
  }

  .section-header:hover {
    background: rgba(0, 255, 200, 0.03);
  }

  .section-chevron {
    font-size: 10px;
    color: var(--content-secondary, #888);
    width: 10px;
    text-align: center;
  }

  .section-icon {
    font-size: 12px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 700;
    color: rgba(0, 255, 200, 0.8);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex: 1;
  }

  .section-actions {
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .section-header:hover .section-actions {
    opacity: 1;
  }

  .pop-out-btn {
    background: none;
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 3px;
    color: rgba(0, 212, 255, 0.8);
    font-size: 11px;
    padding: 1px 5px;
    cursor: pointer;
    line-height: 1;
  }

  .pop-out-btn:hover {
    background: rgba(0, 212, 255, 0.1);
    border-color: rgba(0, 212, 255, 0.6);
  }

  /* === Section Body === */

  .section-body {
    flex: 1;
    position: relative;
    overflow: hidden;
    min-height: 60px;
  }

  .section-body > * {
    display: block;
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    min-width: 0;
    overflow: hidden;
    box-sizing: border-box;
  }

  /* Sidebar widget defaults */
  .sidebar-widget {
    font-size: 11px;
  }
`;

export class RightPanelWidget extends ReactiveWidget {

  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(SIDE_PANEL_STYLES),
    unsafeCSS(RIGHT_PANEL_STYLES),
  ] as CSSResultGroup;

  @reactive() private _sections: RightPanelSectionPayload[] = [];
  @reactive() private _collapsedSections: Set<string> = new Set();
  @reactive() private _isHidden: boolean = false;
  @reactive() private _contentType: string = '';

  /** Cached widget instances keyed by section id — survives section reconfigurations */
  private _widgetCache = new Map<string, HTMLElement>();

  constructor() {
    super({ widgetName: 'RightPanelWidget' });
  }

  // === Panel Configuration ===

  protected get panelSide(): 'left' | 'right' {
    return 'right';
  }

  // === Lifecycle ===

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Restore collapsed state from localStorage
    this._restoreCollapseState();

    // Listen for layout configuration events from MainWidget
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe(UI_EVENTS.RIGHT_PANEL_CONFIGURE, (config: RightPanelConfigPayload) => {
        this._handleLayoutConfig(config);
      });
      return () => unsubscribe();
    });

    this.log('Initialized with accordion layout');
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="panel-header">
        <div class="panel-title">
          <span class="panel-title-icon">⚡</span>
          <span>Sidebar</span>
        </div>
        <button class="collapse-btn" title="Collapse panel" @click=${this._handleCollapse}>
          »
        </button>
      </div>
      <div class="sections-container">
        ${this._sections.length === 0
          ? html`<div class="empty-sidebar">No sidebar content for this view.</div>`
          : this._sections.map(s => this._renderSection(s))
        }
      </div>
    `;
  }

  private _renderSection(section: RightPanelSectionPayload): TemplateResult {
    const isCollapsed = this._collapsedSections.has(section.id);
    const weight = section.flexWeight ?? 1;

    return html`
      <div class="accordion-section ${isCollapsed ? 'collapsed' : ''}"
           style="${isCollapsed ? '' : `flex: ${weight}`}">
        <div class="section-header" @click=${() => this._toggleSection(section.id)}>
          <span class="section-chevron">${isCollapsed ? '▸' : '▾'}</span>
          ${section.icon ? html`<span class="section-icon">${section.icon}</span>` : nothing}
          <span class="section-title">${section.title}</span>
          <div class="section-actions">
            ${section.popOutContentType ? html`
              <button class="pop-out-btn" title="Open in new tab"
                      @click=${(e: Event) => { e.stopPropagation(); this._popOut(section); }}>
                ⬀
              </button>
            ` : nothing}
          </div>
        </div>
        ${isCollapsed ? nothing : html`
          <div class="section-body">
            ${this._getOrCreateWidget(section)}
          </div>
        `}
      </div>
    `;
  }

  // === Widget Management ===

  /**
   * Get or create a widget instance for a section.
   * Cached by section id to preserve widget state across re-renders.
   */
  private _getOrCreateWidget(section: RightPanelSectionPayload): HTMLElement {
    const cacheKey = `${section.id}:${section.widgetTag}`;
    let widget = this._widgetCache.get(cacheKey);

    if (!widget) {
      widget = document.createElement(section.widgetTag);

      // Apply props as attributes
      if (section.props) {
        for (const [key, value] of Object.entries(section.props)) {
          widget.setAttribute(key, value);
        }
      }

      // Mark as sidebar instance
      widget.setAttribute('sidebar', '');
      widget.classList.add('sidebar-widget');

      this._widgetCache.set(cacheKey, widget);
      this.log(`Created ${section.widgetTag} for section '${section.id}'`);
    }

    return widget;
  }

  /**
   * Clean up widget cache — remove widgets for sections that no longer exist.
   */
  private _pruneWidgetCache(activeSections: RightPanelSectionPayload[]): void {
    const activeKeys = new Set(activeSections.map(s => `${s.id}:${s.widgetTag}`));
    for (const [key, widget] of this._widgetCache) {
      if (!activeKeys.has(key)) {
        // Disconnect from DOM if still attached
        widget.remove();
        this._widgetCache.delete(key);
        this.log(`Pruned widget cache: ${key}`);
      }
    }
  }

  // === Layout Configuration ===

  private _handleLayoutConfig(config: RightPanelConfigPayload): void {
    this.log(`Layout config for ${config.contentType}:`, config);
    this._contentType = config.contentType;

    if (config.widget === null && !config.sections) {
      // Hide the panel entirely
      this._isHidden = true;
      this._sections = [];
      this._collapse();
      this.requestUpdate();
      return;
    }

    // Build sections array
    let sections: RightPanelSectionPayload[];

    if (config.sections && config.sections.length > 0) {
      // IDE-style: use declared sections
      sections = config.sections;
    } else {
      // Legacy: single chat-widget section
      sections = [{
        id: 'chat',
        title: 'Assistant',
        icon: '💬',
        widgetTag: config.widget || 'chat-widget',
        props: {
          ...(config.room ? { room: config.room } : {}),
          ...(config.compact ? { compact: '' } : {}),
        },
        flexWeight: 1,
      }];
    }

    this._sections = sections;
    this._pruneWidgetCache(sections);

    // Apply default collapsed state for new sections
    for (const s of sections) {
      if (s.collapsedByDefault && !this._collapsedSections.has(s.id)) {
        // Only set default on first encounter (don't override user's toggle)
        const stored = this._loadCollapseState();
        if (!(s.id in stored)) {
          this._collapsedSections.add(s.id);
        }
      }
    }

    this._isHidden = false;
    this._expand();
    this.requestUpdate();
  }

  // === Section Toggle & Pop-Out ===

  private _toggleSection(sectionId: string): void {
    const newSet = new Set(this._collapsedSections);
    if (newSet.has(sectionId)) {
      newSet.delete(sectionId);
    } else {
      newSet.add(sectionId);
    }
    this._collapsedSections = newSet;
    this._saveCollapseState();
  }

  private async _popOut(section: RightPanelSectionPayload): Promise<void> {
    if (!section.popOutContentType) return;

    try {
      await ContentOpen.execute({
        contentType: section.popOutContentType as ContentType,
        title: section.title,
      });
      this.log(`Popped out section '${section.id}' as tab`);
    } catch (err) {
      console.error(`Failed to pop out section ${section.id}:`, err);
    }
  }

  // === Collapse State Persistence ===

  private _saveCollapseState(): void {
    try {
      const state: Record<string, boolean> = {};
      for (const id of this._collapsedSections) {
        state[id] = true;
      }
      localStorage.setItem(COLLAPSE_STORAGE_KEY, JSON.stringify(state));
    } catch { /* localStorage unavailable */ }
  }

  private _loadCollapseState(): Record<string, boolean> {
    try {
      const raw = localStorage.getItem(COLLAPSE_STORAGE_KEY);
      return raw ? JSON.parse(raw) : {};
    } catch {
      return {};
    }
  }

  private _restoreCollapseState(): void {
    const stored = this._loadCollapseState();
    this._collapsedSections = new Set(Object.keys(stored).filter(k => stored[k]));
  }

  // === Panel Collapse/Expand via PanelResizer ===

  private _handleCollapse = (): void => {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as HTMLElement & { toggle?: () => void };
      resizer?.toggle?.();
    }
  };

  private _collapse(): void {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as HTMLElement & { collapse?: () => void };
      resizer?.collapse?.();
    }
  }

  private _expand(): void {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as HTMLElement & { expand?: () => void };
      resizer?.expand?.();
    }
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
