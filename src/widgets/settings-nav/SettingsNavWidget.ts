/**
 * SettingsNavWidget - Navigation for settings sections
 *
 * MIGRATED TO ReactiveWidget:
 * - Lit's reactive properties (@reactive)
 * - Declarative templates (html``)
 * - Automatic DOM diffing
 * - Declarative event handlers (@click)
 */

import { ReactiveWidget, html, css, reactive, type TemplateResult } from '../shared/ReactiveWidget';
import { Events } from '../../system/core/shared/Events';

export type SettingsSection = 'providers' | 'appearance' | 'account' | 'about';

export const SETTINGS_NAV_EVENTS = {
  SECTION_CHANGED: 'settings:section:changed'
} as const;

export interface SettingsSectionChangedPayload {
  section: SettingsSection;
}

interface NavItem {
  id: SettingsSection;
  icon: string;
  label: string;
}

const NAV_SECTIONS: NavItem[] = [
  { id: 'providers', icon: '🤖', label: 'AI Providers' },
  { id: 'appearance', icon: '🎨', label: 'Appearance' },
  { id: 'account', icon: '👤', label: 'Account' },
  { id: 'about', icon: 'ℹ️', label: 'About' }
];

export class SettingsNavWidget extends ReactiveWidget {
  // Reactive state - changes trigger re-render automatically
  @reactive() private currentSection: SettingsSection = 'providers';

  private _eventCleanup?: () => void;

  static override styles = css`
    :host {
      display: block;
    }

    .settings-nav {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-xs, 4px);
      padding: var(--spacing-sm, 8px);
    }

    .nav-item {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm, 8px);
      padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
      border-radius: var(--border-radius-sm, 4px);
      cursor: pointer;
      transition: background-color 0.15s ease;
      color: var(--text-secondary, #aaa);
    }

    .nav-item:hover {
      background-color: var(--hover-background, rgba(255, 255, 255, 0.05));
    }

    .nav-item.active {
      background-color: var(--active-background, rgba(0, 200, 255, 0.1));
      color: var(--text-primary, #fff);
      border-left: 2px solid var(--accent-color, #00c8ff);
    }

    .nav-icon {
      font-size: 1.1em;
    }

    .nav-label {
      font-size: 0.9em;
    }
  `;

  override connectedCallback(): void {
    super.connectedCallback();
    console.log('📋 SettingsNavWidget: Initializing...');

    // Listen for section changes from SettingsWidget (bidirectional sync)
    this._eventCleanup = Events.subscribe(
      SETTINGS_NAV_EVENTS.SECTION_CHANGED,
      (payload: SettingsSectionChangedPayload) => {
        if (payload.section !== this.currentSection) {
          this.currentSection = payload.section; // Triggers re-render
        }
      }
    );
  }

  override disconnectedCallback(): void {
    this._eventCleanup?.();
    console.log('🧹 SettingsNavWidget: Cleanup complete');
    super.disconnectedCallback();
  }

  private handleSectionClick(section: SettingsSection): void {
    if (section !== this.currentSection) {
      this.currentSection = section; // Triggers re-render
      Events.emit(SETTINGS_NAV_EVENTS.SECTION_CHANGED, { section } as SettingsSectionChangedPayload);
    }
  }

  private renderNavItem(item: NavItem): TemplateResult {
    const isActive = this.currentSection === item.id;
    return html`
      <div
        class="nav-item ${isActive ? 'active' : ''}"
        @click=${() => this.handleSectionClick(item.id)}
      >
        <span class="nav-icon">${item.icon}</span>
        <span class="nav-label">${item.label}</span>
      </div>
    `;
  }

  override render(): TemplateResult {
    return html`
      <div class="settings-nav">
        ${NAV_SECTIONS.map(item => this.renderNavItem(item))}
      </div>
    `;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
