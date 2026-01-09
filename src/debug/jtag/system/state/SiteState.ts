/**
 * SiteState - Global session-lifetime state
 *
 * Part of the Positronic Reactive State Architecture (Site → Page → Widget → Control).
 *
 * This store holds state that persists for the entire session:
 * - Current user identity
 * - Theme preference
 * - Session ID
 * - Global preferences
 *
 * Sources:
 * - UserStateEntity (from server on login)
 * - localStorage (for theme persistence)
 *
 * Listeners:
 * - All widgets (for user/theme info)
 * - RAG builder (for AI context)
 */

import type { UUID } from '../core/types/CrossPlatformUUID';
import { ReactiveStore } from './ReactiveStore';
import { asyncStorage } from '../core/browser/AsyncStorage';

/**
 * Site-level state data structure
 */
export interface SiteStateData {
  /** Current user's UUID */
  userId?: UUID;
  /** Current user's display name */
  displayName?: string;
  /** Current user's unique ID (slug-like identifier) */
  uniqueId?: string;
  /** User type: human, persona, agent */
  userType?: 'human' | 'persona' | 'agent';
  /** Current theme name */
  theme: string;
  /** Session ID (browser tab instance) */
  sessionId?: string;
  /** Whether user is authenticated */
  isAuthenticated: boolean;
  /** Timestamp when site state was last updated */
  updatedAt: number;
}

/**
 * Default site state
 */
const DEFAULT_SITE_STATE: SiteStateData = {
  theme: 'cyberpunk',
  isAuthenticated: false,
  updatedAt: Date.now()
};

/**
 * Site state store singleton
 */
class SiteStateStore extends ReactiveStore<SiteStateData> {
  constructor() {
    super(DEFAULT_SITE_STATE);
    this.loadFromStorage();
  }

  /**
   * Set current user (called on login/auth)
   */
  setUser(user: {
    id: UUID;
    displayName: string;
    uniqueId?: string;
    type?: 'human' | 'persona' | 'agent';
  }): void {
    this.update({
      userId: user.id,
      displayName: user.displayName,
      uniqueId: user.uniqueId,
      userType: user.type || 'human',
      isAuthenticated: true,
      updatedAt: Date.now()
    });

    console.log(`🏠 SiteState: User set to ${user.displayName}`);
  }

  /**
   * Set session ID
   */
  setSession(sessionId: string): void {
    this.update({
      sessionId,
      updatedAt: Date.now()
    });
  }

  /**
   * Set theme and persist to localStorage
   * Uses asyncStorage for non-blocking write
   */
  setTheme(theme: string): void {
    this.update({
      theme,
      updatedAt: Date.now()
    });

    // Persist theme preference (non-blocking via asyncStorage)
    try {
      asyncStorage.setItem('jtag-theme', theme);
    } catch {
      // localStorage not available
    }

    console.log(`🏠 SiteState: Theme set to ${theme}`);
  }

  /**
   * Clear user (logout)
   */
  clearUser(): void {
    this.update({
      userId: undefined,
      displayName: undefined,
      uniqueId: undefined,
      userType: undefined,
      isAuthenticated: false,
      updatedAt: Date.now()
    });
  }

  /**
   * Load persisted state from localStorage
   * Uses asyncStorage.getItem for consistency (checks pending writes first)
   */
  private loadFromStorage(): void {
    try {
      const savedTheme = asyncStorage.getItem('jtag-theme');
      if (savedTheme) {
        this.update({ theme: savedTheme });
      }
    } catch {
      // localStorage not available (SSR, incognito, etc.)
    }
  }

  /**
   * Convenience getters
   */
  get userId(): UUID | undefined {
    return this.get().userId;
  }

  get displayName(): string | undefined {
    return this.get().displayName;
  }

  get theme(): string {
    return this.get().theme;
  }

  get isAuthenticated(): boolean {
    return this.get().isAuthenticated;
  }

  get sessionId(): string | undefined {
    return this.get().sessionId;
  }
}

/**
 * Singleton instance
 */
export const siteState = new SiteStateStore();

/**
 * Export type for external use
 */
export type { SiteStateStore };
