/**
 * LogLevelRegistry — Per-component log level overrides (runtime-mutable)
 *
 * Like audio mute buttons: each component/category can be independently
 * set to a different log level at runtime without restarting the system.
 *
 * Architecture:
 *   1. ComponentLogger calls `LogLevelRegistry.levelFor(component)` on every log call
 *   2. If an override exists → use it (can mute noisy components to ERROR/SILENT)
 *   3. If no override → fall back to global LOG_LEVEL
 *   4. Overrides can be set via Events (from UI toggles or CLI commands)
 *
 * Usage:
 *   // Mute a noisy component
 *   LogLevelRegistry.instance.setLevel('PersonaResponseGenerator', LogLevel.ERROR);
 *
 *   // Mute an entire category
 *   LogLevelRegistry.instance.setLevel('daemons/*', LogLevel.SILENT);
 *
 *   // Unmute (restore to global default)
 *   LogLevelRegistry.instance.clearLevel('PersonaResponseGenerator');
 *
 *   // Bulk configure
 *   LogLevelRegistry.instance.configure({
 *     'PersonaUser': LogLevel.WARN,
 *     'ChatCoordinationStream': LogLevel.ERROR,
 *     'daemons/AIProviderDaemonServer': LogLevel.INFO,
 *   });
 */

import { LogLevel } from './LoggerTypes';

export class LogLevelRegistry {
  private static _instance: LogLevelRegistry;

  // Per-component overrides: component name → minimum log level
  private _overrides: Map<string, LogLevel> = new Map();

  // Per-category overrides (with wildcard support): category pattern → level
  private _categoryOverrides: Map<string, LogLevel> = new Map();

  // Global default (from LOG_LEVEL env var, set by Logger)
  private _globalLevel: LogLevel = LogLevel.INFO;

  private constructor() {}

  static get instance(): LogLevelRegistry {
    if (!LogLevelRegistry._instance) {
      LogLevelRegistry._instance = new LogLevelRegistry();
    }
    return LogLevelRegistry._instance;
  }

  /**
   * Set the global default level (called by Logger on startup)
   */
  set globalLevel(level: LogLevel) {
    this._globalLevel = level;
  }

  get globalLevel(): LogLevel {
    return this._globalLevel;
  }

  /**
   * Set log level override for a specific component.
   * Pass LogLevel.SILENT to completely mute a component.
   */
  setLevel(componentOrCategory: string, level: LogLevel): void {
    if (componentOrCategory.includes('/') || componentOrCategory.includes('*')) {
      this._categoryOverrides.set(componentOrCategory, level);
    } else {
      this._overrides.set(componentOrCategory, level);
    }
  }

  /**
   * Clear override for a component (restores global default)
   */
  clearLevel(componentOrCategory: string): void {
    this._overrides.delete(componentOrCategory);
    this._categoryOverrides.delete(componentOrCategory);
  }

  /**
   * Bulk configure overrides
   */
  configure(overrides: Record<string, LogLevel>): void {
    for (const [key, level] of Object.entries(overrides)) {
      this.setLevel(key, level);
    }
  }

  /**
   * Clear all overrides
   */
  clearAll(): void {
    this._overrides.clear();
    this._categoryOverrides.clear();
  }

  /**
   * Get effective log level for a component.
   * Priority: component override > category override > global default
   */
  levelFor(component: string, category?: string): LogLevel {
    // Check direct component override first
    const componentLevel = this._overrides.get(component);
    if (componentLevel !== undefined) {
      return componentLevel;
    }

    // Check category overrides (exact match, then wildcard)
    if (category) {
      const categoryLevel = this._categoryOverrides.get(category);
      if (categoryLevel !== undefined) {
        return categoryLevel;
      }

      // Wildcard matching: "daemons/*" matches "daemons/AIProviderDaemonServer"
      for (const [pattern, level] of this._categoryOverrides) {
        if (pattern.endsWith('/*')) {
          const prefix = pattern.slice(0, -2);
          if (category.startsWith(prefix)) {
            return level;
          }
        }
      }
    }

    return this._globalLevel;
  }

  /**
   * Check if a specific log level should be logged for this component.
   * Returns true if the message should be logged, false if it should be filtered.
   */
  shouldLog(component: string, level: LogLevel, category?: string): boolean {
    return level >= this.levelFor(component, category);
  }

  /**
   * Get a snapshot of all overrides (for serialization/UI display)
   */
  get overrides(): ReadonlyMap<string, LogLevel> {
    return new Map([...this._overrides, ...this._categoryOverrides]);
  }

  /**
   * Get count of active overrides
   */
  get overrideCount(): number {
    return this._overrides.size + this._categoryOverrides.size;
  }
}
