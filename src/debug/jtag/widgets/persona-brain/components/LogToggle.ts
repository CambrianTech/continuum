/**
 * LogToggle Component
 *
 * A simple toggle button for enabling/disabling logging.
 * Can be used as a master toggle or per-category toggle.
 *
 * State flow:
 * - Master toggle: ON = all categories enabled, OFF = all disabled
 * - Category toggle: ON = category enabled, OFF = category disabled
 * - Category toggles are disabled when master is OFF
 */

import { Commands } from '../../../system/core/shared/Commands';

import { LogsConfig } from '../../../commands/logs/config/shared/LogsConfigTypes';
export interface LogToggleState {
  enabled: boolean;
  categories: string[];
}

export interface LogToggleConfig {
  personaId: string;
  category?: string;  // If set, this is a category toggle. If not, master toggle.
  onStateChange: (state: LogToggleState) => void;
}

/**
 * LogToggle - Handles logging toggle logic cleanly
 */
export class LogToggle {
  private personaId: string;
  private category?: string;
  private onStateChange: (state: LogToggleState) => void;

  constructor(config: LogToggleConfig) {
    this.personaId = config.personaId;
    this.category = config.category;
    this.onStateChange = config.onStateChange;
  }

  /**
   * Check if this toggle is a master toggle (no category)
   */
  get isMaster(): boolean {
    return !this.category;
  }

  /**
   * Check if a category is enabled given the current state
   */
  static isCategoryEnabled(state: LogToggleState, category: string): boolean {
    if (!state.enabled) return false;
    if (state.categories.length === 0) return true;  // Empty = all enabled
    if (state.categories.includes('*')) return true;
    return state.categories.includes(category);
  }

  /**
   * Toggle this control
   * - Master toggle: enables/disables all logging
   * - Category toggle: enables/disables specific category (only when master is ON)
   */
  async toggle(currentState: LogToggleState): Promise<void> {
    if (this.isMaster) {
      await this.toggleMaster(currentState);
    } else {
      await this.toggleCategory(currentState);
    }
  }

  /**
   * Master toggle - turn all logging ON or OFF
   */
  private async toggleMaster(currentState: LogToggleState): Promise<void> {
    const newEnabled = !currentState.enabled;

    try {
      const result = await LogsConfig.execute({
        persona: this.personaId,
        action: newEnabled ? 'enable' : 'disable'
      } as any) as any;

      if (result.success && result.personaConfig) {
        this.onStateChange({
          enabled: result.personaConfig.enabled,
          categories: result.personaConfig.categories || []
        });
      }
    } catch (error) {
      console.error('LogToggle: Error toggling master:', error);
    }
  }

  /**
   * Category toggle - turn specific category ON or OFF
   * Only works when master is enabled
   */
  private async toggleCategory(currentState: LogToggleState): Promise<void> {
    // Block if master is off
    if (!currentState.enabled) {
      console.log('LogToggle: Master is off, enable it first');
      return;
    }

    const isEnabled = LogToggle.isCategoryEnabled(currentState, this.category!);

    try {
      const result = await LogsConfig.execute({
        persona: this.personaId,
        action: isEnabled ? 'disable' : 'enable',
        category: this.category
      } as any) as any;

      if (result.success && result.personaConfig) {
        this.onStateChange({
          enabled: result.personaConfig.enabled,
          categories: result.personaConfig.categories || []
        });
      }
    } catch (error) {
      console.error('LogToggle: Error toggling category:', error);
    }
  }

  /**
   * Render a master toggle button (HTML)
   */
  static renderMasterButton(state: LogToggleState): string {
    return `
      <button class="log-toggle ${state.enabled ? 'enabled' : ''}"
              data-action="toggle-logging"
              title="${state.enabled ? 'Logging ON - Click to disable' : 'Logging OFF - Click to enable'}">
        ${state.enabled ? '📝' : '📋'}
      </button>
    `;
  }

  /**
   * Render a category toggle for SVG (returns SVG group)
   */
  static renderSvgToggle(
    category: string,
    state: LogToggleState,
    x: number,
    y: number
  ): string {
    const isEnabled = LogToggle.isCategoryEnabled(state, category);
    const isDisabled = !state.enabled;

    return `
      <g class="module-log-toggle ${isEnabled ? 'enabled' : ''} ${isDisabled ? 'disabled' : ''}"
         data-category="${category}">
        <rect x="${x}" y="${y}" width="22" height="16" rx="2" class="log-toggle-bg"/>
        <text x="${x + 11}" y="${y + 12}" class="log-toggle-icon">${isEnabled ? '📝' : '📋'}</text>
      </g>
    `;
  }

  /**
   * CSS for log toggles (to be included in widget styles)
   */
  static get styles(): string {
    return `
      /* Master log toggle button */
      .log-toggle {
        background: rgba(0, 15, 25, 0.8);
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 4px;
        padding: 4px 8px;
        font-size: 14px;
        cursor: pointer;
        transition: all 0.2s ease;
        opacity: 0.6;
      }

      .log-toggle:hover {
        opacity: 1;
        border-color: rgba(0, 212, 255, 0.6);
        background: rgba(0, 30, 50, 0.9);
      }

      .log-toggle.enabled {
        opacity: 1;
        border-color: rgba(0, 255, 100, 0.5);
        background: rgba(0, 40, 20, 0.9);
        box-shadow: 0 0 8px rgba(0, 255, 100, 0.3);
      }

      .log-toggle.enabled:hover {
        border-color: rgba(0, 255, 100, 0.8);
        box-shadow: 0 0 12px rgba(0, 255, 100, 0.4);
      }

      /* SVG category toggles */
      .module-log-toggle {
        cursor: pointer;
        opacity: 0.5;
        transition: all 0.2s ease;
      }

      .module-log-toggle:hover {
        opacity: 1;
      }

      .module-log-toggle .log-toggle-bg {
        fill: rgba(0, 15, 25, 0.9);
        stroke: rgba(0, 212, 255, 0.3);
        stroke-width: 1;
        transition: all 0.2s ease;
      }

      .module-log-toggle:hover .log-toggle-bg {
        stroke: rgba(0, 212, 255, 0.6);
        fill: rgba(0, 30, 50, 0.9);
      }

      .module-log-toggle.enabled {
        opacity: 1;
      }

      .module-log-toggle.enabled .log-toggle-bg {
        stroke: rgba(0, 255, 100, 0.5);
        fill: rgba(0, 40, 20, 0.9);
        filter: drop-shadow(0 0 4px rgba(0, 255, 100, 0.4));
      }

      .module-log-toggle.enabled:hover .log-toggle-bg {
        stroke: rgba(0, 255, 100, 0.8);
        filter: drop-shadow(0 0 8px rgba(0, 255, 100, 0.5));
      }

      .module-log-toggle .log-toggle-icon {
        font-size: 10px;
        text-anchor: middle;
        dominant-baseline: middle;
        pointer-events: none;
      }

      .module-log-toggle.disabled {
        opacity: 0.3;
        cursor: not-allowed;
        pointer-events: none;
      }
    `;
  }
}
