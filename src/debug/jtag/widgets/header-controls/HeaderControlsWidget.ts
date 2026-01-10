/**
 * HeaderControlsWidget - Header menu with version, theme, settings, help
 *
 * Consolidated widget that manages the entire header controls section.
 * Displays dynamic version and action buttons (theme, settings, help).
 * Listens to events for dynamic updates.
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
import { VERSION } from '../../shared/version';
import { Events } from '../../system/core/shared/Events';

export class HeaderControlsWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(`
      .header-controls {
        display: flex;
        gap: 16px;
        align-items: center;
        justify-content: flex-end;
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
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentVersion: string = VERSION;

  constructor() {
    super({
      widgetName: 'HeaderControlsWidget'
    });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Subscribe to version updates (future: when version changes dynamically)
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe('system:version', (version: string) => {
        this.currentVersion = version;
        this.requestUpdate();
      });
      return () => unsubscribe();
    });
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="header-controls">
        <div class="version-info">
          <div class="version-badge">v${this.currentVersion}</div>
        </div>
        <div class="status-buttons">
          <button class="status-button" @click=${this.handleThemeClick}>Theme</button>
          <button class="status-button" @click=${this.handleSettingsClick}>Settings</button>
          <button class="status-button" @click=${this.handleBrowserClick}>Browser</button>
          <button class="status-button" @click=${this.handleHelpClick}>Help</button>
        </div>
      </div>
    `;
  }

  // === Event Handlers ===

  private handleThemeClick = (): void => {
    Events.emit('header:theme-clicked', {});
    this.dispatchEvent(new CustomEvent('theme-clicked', {
      bubbles: true,
      composed: true
    }));
  };

  private handleSettingsClick = (): void => {
    Events.emit('header:settings-clicked', {});
    this.dispatchEvent(new CustomEvent('settings-clicked', {
      bubbles: true,
      composed: true
    }));
  };

  private handleBrowserClick = (): void => {
    Events.emit('header:browser-clicked', {});
    this.dispatchEvent(new CustomEvent('browser-clicked', {
      bubbles: true,
      composed: true
    }));
  };

  private handleHelpClick = (): void => {
    Events.emit('header:help-clicked', {});
    this.dispatchEvent(new CustomEvent('help-clicked', {
      bubbles: true,
      composed: true
    }));
  };
}

// Registration handled by centralized BROWSER_WIDGETS registry
