/**
 * SettingsNavWidget - Navigation for settings sections
 *
 * Shows settings navigation in the left sidebar when Settings is open.
 * Emits events to coordinate with SettingsWidget in center panel.
 *
 * Structure:
 * - public/settings-nav-widget.html - Template container
 * - public/settings-nav-widget.scss - Styles (compiled to .css)
 * - SettingsNavWidget.ts - Logic (this file)
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
      template: 'settings-nav-widget.html',
      styles: 'settings-nav-widget.css',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  /**
   * Override path resolution - directory is 'settings-nav' (kebab-case)
   */
  protected resolveResourcePath(filename: string): string {
    return `widgets/settings-nav/public/${filename}`;
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
    // Inject loaded template and styles into shadow DOM
    if (this.shadowRoot && (this.templateHTML || this.templateCSS)) {
      const styleTag = this.templateCSS ? `<style>${this.templateCSS}</style>` : '';
      this.shadowRoot.innerHTML = styleTag + (this.templateHTML || '');
    }

    // Generate nav items dynamically
    this.renderNavItems();
    this.setupEventListeners();
  }

  private renderNavItems(): void {
    const sections: { id: SettingsSection; icon: string; label: string }[] = [
      { id: 'providers', icon: '🤖', label: 'AI Providers' },
      { id: 'appearance', icon: '🎨', label: 'Appearance' },
      { id: 'account', icon: '👤', label: 'Account' },
      { id: 'about', icon: 'ℹ️', label: 'About' }
    ];

    const navItemsContainer = this.shadowRoot?.querySelector('.nav-items');
    if (!navItemsContainer) return;

    navItemsContainer.innerHTML = sections.map(s => `
      <div class="nav-item ${this.currentSection === s.id ? 'active' : ''}" data-section="${s.id}">
        <span class="nav-icon">${s.icon}</span>
        <span class="nav-label">${s.label}</span>
      </div>
    `).join('');
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
