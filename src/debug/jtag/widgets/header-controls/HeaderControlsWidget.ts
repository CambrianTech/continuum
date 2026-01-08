/**
 * HeaderControlsWidget - Header menu with version, theme, settings, help
 *
 * Consolidated widget that manages the entire header controls section.
 * Displays dynamic version and action buttons (theme, settings, help).
 * Listens to events for dynamic updates.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { VERSION } from '../../shared/version';
import { Events } from '../../system/core/shared/Events';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export class HeaderControlsWidget extends BaseWidget {
  private currentVersion: string = VERSION;
  private _eventUnsubscribe?: () => void;

  constructor() {
    super({
      widgetName: 'HeaderControlsWidget',
      template: undefined,  // Inline template
      styles: undefined,     // Inline styles
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    verbose() && console.log('🎮 HeaderControlsWidget: Initializing header controls...');

    // Subscribe to version updates (future: when version changes dynamically)
    this._eventUnsubscribe = Events.subscribe('system:version', (version: string) => {
      this.currentVersion = version;
      this.updateVersionDisplay();
    });

    verbose() && console.log(`✅ HeaderControlsWidget: Initialized (version: ${this.currentVersion})`);
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
      .header-controls {
        display: flex;
        gap: 16px;
        align-items: center;
      }

      .version-info {
        display: flex;
        align-items: center;
      }

      .version-badge {
        background: var(--button-primary-background, linear-gradient(135deg, #00d4ff, #0080ff));
        color: var(--button-primary-text, #000a0f);
        padding: 6px 12px;
        border-radius: 4px;
        font-weight: 600;
        font-size: 12px;
        font-family: 'Courier New', monospace;
        letter-spacing: 0.5px;
        box-shadow:
          0 2px 8px var(--button-primary-shadow, rgba(0, 212, 255, 0.3)),
          inset 0 1px 0 rgba(255, 255, 255, 0.2);
        border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.4));
        cursor: default;
        user-select: none;
      }

      .version-badge:hover {
        background: linear-gradient(135deg, #00e4ff, #0090ff);
        box-shadow:
          0 4px 12px rgba(0, 212, 255, 0.5),
          inset 0 1px 0 rgba(255, 255, 255, 0.3);
      }

      .status-buttons {
        display: flex;
        gap: 8px;
      }

      .status-button {
        background: var(--button-secondary-background, rgba(0, 212, 255, 0.1));
        border: 1px solid var(--widget-border, rgba(0, 212, 255, 0.3));
        color: var(--content-accent, #00d4ff);
        padding: 6px 16px;
        border-radius: 4px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s ease;
        user-select: none;
      }

      .status-button:hover {
        background: rgba(0, 212, 255, 0.2);
        border-color: rgba(0, 212, 255, 0.6);
        box-shadow: 0 2px 8px rgba(0, 212, 255, 0.2);
      }

      .status-button:active {
        transform: translateY(1px);
        box-shadow: 0 1px 4px rgba(0, 212, 255, 0.3);
      }

      .status-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
    `;

    const template = `
      <div class="header-controls">
        <div class="version-info">
          <div class="version-badge" id="version-badge">${this.getVersionText()}</div>
        </div>
        <div class="status-buttons">
          <button class="status-button" id="theme-button">Theme</button>
          <button class="status-button" id="settings-button">Settings</button>
          <button class="status-button" id="browser-button">Browser</button>
          <button class="status-button" id="help-button">Help</button>
        </div>
      </div>
    `;

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${template}
    `;

    // Add event listeners after DOM is created
    this.setupEventListeners();

    verbose() && console.log('✅ HeaderControlsWidget: Rendered');
  }

  /**
   * Get formatted version text
   */
  private getVersionText(): string {
    // Extract package name if needed
    return `v${this.currentVersion}`;
  }

  /**
   * Update version display dynamically
   */
  private updateVersionDisplay(): void {
    const versionBadge = this.shadowRoot?.getElementById('version-badge');
    if (versionBadge) {
      versionBadge.textContent = this.getVersionText();
      verbose() && console.log(`🔄 HeaderControlsWidget: Updated version to ${this.currentVersion}`);
    }
  }

  /**
   * Setup event listeners for buttons
   */
  private setupEventListeners(): void {
    // Theme button
    this.shadowRoot?.getElementById('theme-button')?.addEventListener('click', () => {
      this.handleThemeClick();
    });

    // Settings button
    this.shadowRoot?.getElementById('settings-button')?.addEventListener('click', () => {
      this.handleSettingsClick();
    });

    // Browser button
    this.shadowRoot?.getElementById('browser-button')?.addEventListener('click', () => {
      this.handleBrowserClick();
    });

    // Help button
    this.shadowRoot?.getElementById('help-button')?.addEventListener('click', () => {
      this.handleHelpClick();
    });
  }

  /**
   * Handle theme button click
   */
  private handleThemeClick(): void {
    verbose() && console.log('🎨 HeaderControlsWidget: Theme button clicked');

    // Emit event for parent widget to handle theme modal
    Events.emit('header:theme-clicked', {});

    // Also dispatch DOM event for MainWidget to catch
    this.dispatchEvent(new CustomEvent('theme-clicked', {
      bubbles: true,
      composed: true
    }));
  }

  /**
   * Handle settings button click
   */
  private handleSettingsClick(): void {
    verbose() && console.log('⚙️ HeaderControlsWidget: Settings button clicked');

    // Emit event for future settings implementation
    Events.emit('header:settings-clicked', {});

    this.dispatchEvent(new CustomEvent('settings-clicked', {
      bubbles: true,
      composed: true
    }));
  }

  /**
   * Handle browser button click
   */
  private handleBrowserClick(): void {
    verbose() && console.log('🌐 HeaderControlsWidget: Browser button clicked');

    Events.emit('header:browser-clicked', {});

    this.dispatchEvent(new CustomEvent('browser-clicked', {
      bubbles: true,
      composed: true
    }));
  }

  /**
   * Handle help button click
   */
  private handleHelpClick(): void {
    verbose() && console.log('❓ HeaderControlsWidget: Help button clicked');

    // Emit event for future help implementation
    Events.emit('header:help-clicked', {});

    this.dispatchEvent(new CustomEvent('help-clicked', {
      bubbles: true,
      composed: true
    }));
  }

  protected async onWidgetCleanup(): Promise<void> {
    this._eventUnsubscribe?.();
    verbose() && console.log('🧹 HeaderControlsWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
