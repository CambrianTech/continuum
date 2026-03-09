/**
 * ContentTabsWidget - React-style subscriber to global content state
 *
 * ARCHITECTURE:
 * 1. Subscribes to contentState (global singleton)
 * 2. Renders tabs from contentState.openItems
 * 3. On click: updates contentState → triggers re-render
 * 4. On close: updates contentState → triggers re-render
 *
 * NO events, NO parent calls, just shared state.
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { contentState, type ContentStateData } from '../../system/state/ContentStateService';
import { ContentService } from '../../system/state/ContentService';
import { LiveCallTracker, type LiveCallState } from '../live/LiveCallTracker';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { ContentType } from '../../system/data/entities/UserStateEntity';
import { styles as externalStyles } from './public/content-tabs-widget.styles';

export interface TabInfo {
  id: string;
  label: string;
  active: boolean;
  closeable?: boolean;
  entityId?: string;
  uniqueId?: string;  // Human-readable ID for URLs
  contentType?: string;
}

/** Icons for content type differentiation in tabs */
const TYPE_ICONS: Partial<Record<ContentType, string>> = {
  chat: '\u{1F4AC}',            // 💬 speech bubble
  live: '\u{1F3A5}',            // 🎥 camera
  persona: '\u{1F9E0}',         // 🧠 brain
  settings: '\u{2699}\uFE0F',   // ⚙️ gear
  theme: '\u{1F3A8}',           // 🎨 palette
  'user-profile': '\u{1F464}',  // 👤 bust
  profile: '\u{1F464}',         // 👤 bust
  diagnostics: '\u{1F50D}',     // 🔍 magnifier
  'diagnostics-log': '\u{1F4CB}', // 📋 clipboard
  canvas: '\u{1F5BC}\uFE0F',    // 🖼️ framed picture
  document: '\u{1F4C4}',        // 📄 page
  help: '\u{2753}',             // ❓ question mark
  browser: '\u{1F310}',         // 🌐 globe
  'data-explorer': '\u{1F5C4}\uFE0F', // 🗄️ file cabinet
  'system-config': '\u{1F527}', // 🔧 wrench
  'widget-debug': '\u{1F41B}',  // 🐛 bug
};

export class ContentTabsWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(externalStyles)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private tabs: TabInfo[] = [];

  /** Active live call states — for media indicator on tabs */
  @reactive() private _liveCalls: Map<string, LiveCallState> = new Map();
  private _liveTrackerUnsub?: () => void;

  constructor() {
    super({
      widgetName: 'ContentTabsWidget'
    });
  }

  override connectedCallback(): void {
    super.connectedCallback();

    // Track active live calls via shared browser-side tracker (no event bus complexity)
    this._liveTrackerUnsub = LiveCallTracker.subscribe((calls) => {
      this._liveCalls = calls;
      this.requestUpdate();
    });
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._liveTrackerUnsub?.();
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Subscribe to global contentState - React pattern
    this.createMountEffect(() => {
      const unsubscribe = contentState.subscribe((state) => {
        this.updateFromContentState(state);
      });
      return () => unsubscribe();
    });
  }

  /**
   * Update from global contentState - the ONLY source of truth
   */
  private updateFromContentState(state: ContentStateData): void {
    // Convert to TabInfo format
    this.tabs = state.openItems.map(item => ({
      id: item.id,
      label: item.title || item.type,
      active: item.id === state.currentItemId,
      closeable: true,
      entityId: item.entityId,
      uniqueId: item.uniqueId,  // For human-readable URLs
      contentType: item.type
    }));
    this.requestUpdate();
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    if (this.tabs.length === 0) {
      return html`
        <div class="content-tabs-container">
          <div class="empty-state">No content tabs</div>
        </div>
      `;
    }

    return html`
      <div class="content-tabs-container">
        ${this.tabs.map(tab => {
          const typeIcon = tab.contentType ? TYPE_ICONS[tab.contentType as ContentType] : undefined;
          const callState = tab.contentType === 'live' && tab.entityId
            ? this._liveCalls.get(tab.entityId)
            : undefined;
          const hasActiveMedia = callState && (callState.micActive || callState.cameraActive);
          return html`
            <div class="content-tab ${tab.active ? 'active' : ''} ${hasActiveMedia ? 'media-active' : ''}"
                 @click=${(e: Event) => this.handleTabClick(e, tab)}>
              ${hasActiveMedia ? html`<span class="media-indicator"></span>` : ''}
              ${typeIcon ? html`<span class="tab-type-icon">${typeIcon}</span>` : ''}
              <span class="tab-label">${tab.label}</span>
              ${tab.closeable ? html`
                <span class="tab-close" @click=${(e: Event) => this.handleTabClose(e, tab)}>×</span>
              ` : ''}
            </div>
          `;
        })}
      </div>
    `;
  }

  // === Event Handlers ===

  /**
   * Handle tab click - delegate to ContentService
   */
  private handleTabClick(event: Event, tab: TabInfo): void {
    if (this.userState?.userId) {
      ContentService.setUserId(this.userState.userId as UUID);
    }
    ContentService.switchTo(tab.id);
  }

  /**
   * Handle tab close - delegate to ContentService
   */
  private handleTabClose(event: Event, tab: TabInfo): void {
    event.stopPropagation(); // Don't trigger tab click

    if (this.userState?.userId) {
      ContentService.setUserId(this.userState.userId as UUID);
    }
    ContentService.close(tab.id);
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
