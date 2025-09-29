/**
 * LocalStorageStateManager - Browser-side state persistence
 *
 * Provides immediate state persistence via localStorage for:
 * - Theme preferences
 * - User interface state
 * - Session-specific settings
 *
 * Works in hybrid mode with UserState entity for full persistence:
 * 1. localStorage: Immediate persistence, single device
 * 2. UserState: Database persistence, cross-device sync
 *
 * Architecture decision: localStorage as cache layer for UserState
 */

import type { UUID } from '../types/CrossPlatformUUID';
import { stringToUUID } from '../types/CrossPlatformUUID';

export interface LocalStateData {
  userId: UUID;
  theme?: string;
  preferences?: {
    maxOpenTabs?: number;
    autoCloseAfterDays?: number;
    rememberScrollPosition?: boolean;
    syncAcrossDevices?: boolean;
  };
  contentState?: {
    openItems?: Array<{
      id: UUID;
      type: string;
      entityId: UUID;
      title: string;
      lastAccessedAt: string;
    }>;
    currentItemId?: UUID;
  };
  ui?: {
    sidebarWidth?: number;
    currentRoomId?: UUID;
    themeSettings?: {
      currentTheme: string;
      defaultTheme: string;
      lastModified: number;
    };
  };
  console?: {
    logs?: Array<any>;
    maxLogEntries?: number;
  };
  lastSync?: string;
  version?: number;
}

export class LocalStorageStateManager {
  private static readonly STORAGE_KEY = 'continuum-state';
  private static readonly VERSION = 1;

  /**
   * Get the current anonymous user ID (consistent with ThemeWidget approach)
   */
  private static getAnonymousUserId(): UUID {
    return stringToUUID('anonymous');
  }

  /**
   * Load state from localStorage with proper error handling
   */
  static loadState(): LocalStateData | null {
    try {
      const storedData = localStorage.getItem(this.STORAGE_KEY);
      if (!storedData) {
        console.log('🔧 LocalStorageStateManager: No stored state found');
        return null;
      }

      const parsedData = JSON.parse(storedData) as LocalStateData;

      // Validate basic structure
      if (!parsedData.userId || !parsedData.version) {
        console.warn('⚠️ LocalStorageStateManager: Invalid stored state structure');
        return null;
      }

      // Version compatibility check
      if (parsedData.version !== this.VERSION) {
        console.warn(`⚠️ LocalStorageStateManager: Version mismatch (stored: ${parsedData.version}, current: ${this.VERSION})`);
        // Could add migration logic here in the future
        return null;
      }

      console.log('✅ LocalStorageStateManager: State loaded successfully');
      return parsedData;

    } catch (error) {
      console.error('❌ LocalStorageStateManager: Failed to load state:', error);
      return null;
    }
  }

  /**
   * Save state to localStorage with proper error handling
   */
  static saveState(stateData: Partial<LocalStateData>): boolean {
    try {
      const currentState = this.loadState() || {
        userId: this.getAnonymousUserId(),
        version: this.VERSION,
        lastSync: new Date().toISOString()
      };

      // Merge with existing state
      const updatedState: LocalStateData = {
        ...currentState,
        ...stateData,
        userId: currentState.userId, // Preserve user ID
        version: this.VERSION, // Ensure version consistency
        lastSync: new Date().toISOString()
      };

      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(updatedState));
      console.log('✅ LocalStorageStateManager: State saved successfully');
      return true;

    } catch (error) {
      console.error('❌ LocalStorageStateManager: Failed to save state:', error);
      return false;
    }
  }

  /**
   * Get specific theme preference from localStorage
   */
  static getTheme(): string | null {
    const state = this.loadState();
    return state?.theme || null;
  }

  /**
   * Set specific theme preference in localStorage
   */
  static setTheme(theme: string): boolean {
    return this.saveState({ theme });
  }

  /**
   * Get user preferences from localStorage
   */
  static getPreferences(): LocalStateData['preferences'] | null {
    const state = this.loadState();
    return state?.preferences || null;
  }

  /**
   * Set user preferences in localStorage
   */
  static setPreferences(preferences: LocalStateData['preferences']): boolean {
    return this.saveState({ preferences });
  }

  /**
   * Get content state from localStorage
   */
  static getContentState(): LocalStateData['contentState'] | null {
    const state = this.loadState();
    return state?.contentState || null;
  }

  /**
   * Set content state in localStorage
   */
  static setContentState(contentState: LocalStateData['contentState']): boolean {
    return this.saveState({ contentState });
  }

  /**
   * Clear all stored state (for testing/cleanup)
   */
  static clearState(): void {
    try {
      localStorage.removeItem(this.STORAGE_KEY);
      console.log('✅ LocalStorageStateManager: State cleared successfully');
    } catch (error) {
      console.error('❌ LocalStorageStateManager: Failed to clear state:', error);
    }
  }

  /**
   * Check if localStorage is available in the current environment
   */
  static isAvailable(): boolean {
    try {
      const testKey = '__continuum_test__';
      localStorage.setItem(testKey, 'test');
      localStorage.removeItem(testKey);
      return true;
    } catch (error) {
      console.warn('⚠️ LocalStorageStateManager: localStorage not available:', error);
      return false;
    }
  }

  /**
   * Initialize state for a new user/session
   */
  static initializeState(): LocalStateData {
    const initialState: LocalStateData = {
      userId: this.getAnonymousUserId(),
      theme: 'base',
      preferences: {
        maxOpenTabs: 10,
        autoCloseAfterDays: 30,
        rememberScrollPosition: true,
        syncAcrossDevices: true
      },
      contentState: {
        openItems: [],
        currentItemId: undefined
      },
      version: this.VERSION,
      lastSync: new Date().toISOString()
    };

    this.saveState(initialState);
    console.log('✅ LocalStorageStateManager: Initial state created');
    return initialState;
  }

  /**
   * Get current state or initialize if not present
   */
  static getOrInitializeState(): LocalStateData {
    const existingState = this.loadState();
    return existingState || this.initializeState();
  }

  /**
   * Debug helper: Get all stored data
   */
  static debug(): void {
    console.group('🔧 LocalStorageStateManager Debug');
    console.log('Storage key:', this.STORAGE_KEY);
    console.log('Version:', this.VERSION);
    console.log('Available:', this.isAvailable());
    console.log('Current state:', this.loadState());
    console.groupEnd();
  }
}