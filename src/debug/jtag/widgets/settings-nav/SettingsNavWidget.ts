/**
 * SettingsNavWidget - Navigation for settings sections
 *
 * Shows settings navigation in the left sidebar when Settings is open.
 * Emits events to coordinate with SettingsWidget in center panel.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';

export type SettingsSection = 'providers' | 'appearance' | 'account' | 'about';

export const SETTINGS_NAV_EVENTS = {
  SECTION_CHANGED: 'settings:section:changed'
} as const;

export interface SettingsSectionChangedPayload {
  section: SettingsSection;
}

export class SettingsNavWidget extends BaseWidget {
  private currentSection: SettingsSection = 'providers';

  constructor() {
    super({
      widgetName: 'SettingsNavWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('📋 SettingsNavWidget: Initializing...');

    // Listen for section changes from SettingsWidget (bidirectional sync)
    Events.subscribe(SETTINGS_NAV_EVENTS.SECTION_CHANGED, (payload: SettingsSectionChangedPayload) => {
      if (payload.section !== this.currentSection) {
        this.currentSection = payload.section;
        this.updateActiveState();
      }
    });
  }

  protected async renderWidget(): Promise<void> {
    const sections: { id: SettingsSection; icon: string; label: string }[] = [
      { id: 'providers', icon: '🤖', label: 'AI Providers' },
      { id: 'appearance', icon: '🎨', label: 'Appearance' },
      { id: 'account', icon: '👤', label: 'Account' },
      { id: 'about', icon: 'ℹ️', label: 'About' }
    ];

    const styles = `
      :host {
        display: block;
      }

      .settings-nav-container {
        padding: var(--spacing-md, 12px);
      }

      .nav-title {
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--content-secondary, rgba(255, 255, 255, 0.5));
        margin-bottom: var(--spacing-sm, 8px);
        padding: 0 var(--spacing-sm, 8px);
      }

      .nav-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm, 8px);
        padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
        border-radius: var(--border-radius-sm, 6px);
        cursor: pointer;
        transition: all 0.15s ease;
        color: var(--content-secondary, rgba(255, 255, 255, 0.7));
        font-size: 14px;
      }

      .nav-item:hover {
        background: var(--sidebar-hover, rgba(0, 212, 255, 0.1));
        color: var(--content-primary, white);
      }

      .nav-item.active {
        background: var(--sidebar-active, rgba(0, 212, 255, 0.15));
        color: var(--content-accent, #00d4ff);
        border-left: 3px solid var(--content-accent, #00d4ff);
        margin-left: -3px;
      }

      .nav-icon {
        font-size: 16px;
        width: 20px;
        text-align: center;
      }

      .nav-label {
        flex: 1;
      }
    `;

    const navItems = sections.map(s => `
      <div class="nav-item ${this.currentSection === s.id ? 'active' : ''}" data-section="${s.id}">
        <span class="nav-icon">${s.icon}</span>
        <span class="nav-label">${s.label}</span>
      </div>
    `).join('');

    const template = `
      <div class="settings-nav-container">
        <div class="nav-title">Settings</div>
        ${navItems}
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${template}`;
    this.setupEventListeners();
  }

  private setupEventListeners(): void {
    this.shadowRoot?.querySelectorAll('.nav-item').forEach(item => {
      item.addEventListener('click', (e) => {
        const section = (e.currentTarget as HTMLElement).dataset.section as SettingsSection;
        if (section && section !== this.currentSection) {
          this.currentSection = section;
          this.updateActiveState();
          Events.emit(SETTINGS_NAV_EVENTS.SECTION_CHANGED, { section } as SettingsSectionChangedPayload);
        }
      });
    });
  }

  private updateActiveState(): void {
    this.shadowRoot?.querySelectorAll('.nav-item').forEach(item => {
      const section = (item as HTMLElement).dataset.section;
      item.classList.toggle('active', section === this.currentSection);
    });
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 SettingsNavWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
