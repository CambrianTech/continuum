/**
 * UIPreferences Entity - Browser UI State Management
 *
 * Stores browser-specific UI preferences that don't need server sync:
 * - Layout dimensions (sidebar width, panel sizes)
 * - Theme preferences
 * - UI behavior settings
 *
 * Stored in localStorage only, per-browser instance
 */

import { TextField, JsonField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

export type ThemeName = 'dark' | 'light' | 'cyberpunk' | 'matrix' | 'solarized';

export interface ThemeSettings {
  name: ThemeName;
  customColors?: Record<string, string>;
  fontSize?: number;
  fontFamily?: string;
}

export interface LayoutSettings {
  sidebarWidth: number;
  panelSizes?: Record<string, number>;
  collapsed?: Record<string, boolean>;
}

export interface BehaviorSettings {
  enableAnimations: boolean;
  enableSounds: boolean;
  autoSave: boolean;
  autoSaveIntervalMs: number;
}

/**
 * UIPreferences Entity - Per-browser UI preferences
 *
 * Manages UI state that should persist across page refreshes
 * but doesn't need to sync to server or across devices
 */
export class UIPreferencesEntity extends BaseEntity {
  static readonly collection = 'UIPreferences';

  @TextField({ index: true })
  deviceId: string; // Browser/device identifier

  @JsonField()
  theme: ThemeSettings;

  @JsonField()
  layout: LayoutSettings;

  @JsonField()
  behavior: BehaviorSettings;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();

    // Default values
    this.deviceId = 'default';
    this.theme = {
      name: 'dark'
    };
    this.layout = {
      sidebarWidth: 400
    };
    this.behavior = {
      enableAnimations: true,
      enableSounds: false,
      autoSave: true,
      autoSaveIntervalMs: 5000
    };
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return UIPreferencesEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate UI preferences data
   */
  validate(): { success: boolean; error?: string } {
    // Validate deviceId
    if (!this.deviceId?.trim()) {
      return { success: false, error: 'UIPreferences deviceId is required' };
    }

    // Validate theme
    if (!this.theme) {
      return { success: false, error: 'UIPreferences theme is required' };
    }

    const validThemes: ThemeName[] = ['dark', 'light', 'cyberpunk', 'matrix', 'solarized'];
    if (!validThemes.includes(this.theme.name)) {
      return { success: false, error: `UIPreferences theme.name must be one of: ${validThemes.join(', ')}` };
    }

    // Validate layout
    if (!this.layout) {
      return { success: false, error: 'UIPreferences layout is required' };
    }

    if (typeof this.layout.sidebarWidth !== 'number') {
      return { success: false, error: 'UIPreferences layout.sidebarWidth must be a number' };
    }

    if (this.layout.sidebarWidth < 100 || this.layout.sidebarWidth > 800) {
      return { success: false, error: 'UIPreferences layout.sidebarWidth must be between 100 and 800' };
    }

    // Validate behavior
    if (!this.behavior) {
      return { success: false, error: 'UIPreferences behavior is required' };
    }

    if (typeof this.behavior.enableAnimations !== 'boolean') {
      return { success: false, error: 'UIPreferences behavior.enableAnimations must be a boolean' };
    }

    if (typeof this.behavior.autoSaveIntervalMs !== 'number' || this.behavior.autoSaveIntervalMs < 1000) {
      return { success: false, error: 'UIPreferences behavior.autoSaveIntervalMs must be a number >= 1000' };
    }

    return { success: true };
  }

  /**
   * Update sidebar width
   */
  setSidebarWidth(width: number): void {
    if (width < 100 || width > 800) {
      throw new Error('Sidebar width must be between 100 and 800');
    }
    this.layout.sidebarWidth = width;
  }

  /**
   * Update theme
   */
  setTheme(theme: ThemeName): void {
    this.theme.name = theme;
  }

  /**
   * Migrate from old localStorage format
   * Note: This method should only be called in browser environment
   * The actual migration will be done by browser-specific code that has access to localStorage
   */
  static migrateFromLegacy(oldData?: { sidebarWidth?: number; theme?: ThemeName }): UIPreferencesEntity {
    const entity = new UIPreferencesEntity();

    // Apply migrated data if provided
    if (oldData) {
      if (oldData.sidebarWidth && oldData.sidebarWidth >= 100 && oldData.sidebarWidth <= 800) {
        entity.layout.sidebarWidth = oldData.sidebarWidth;
      }
      if (oldData.theme) {
        entity.theme.name = oldData.theme;
      }
    }

    return entity;
  }
}