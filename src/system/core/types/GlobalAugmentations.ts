/**
 * GlobalAugmentations — Type-safe access to JTAG globals on window and globalThis
 *
 * Eliminates `(window as any).jtag` / `(globalThis as any).__JTAG_COMMAND_DAEMON__`
 * casts by declaring the custom properties TypeScript doesn't know about.
 *
 * These globals are assigned at runtime during bootstrap and hold various object
 * shapes depending on initialization order. We type them broadly enough to avoid
 * `as any` at access sites while keeping known property access type-checked.
 */

import type { Events as EventsClass } from '../shared/Events';

/* eslint-disable @typescript-eslint/no-explicit-any */

/**
 * Properties we attach to `window` at runtime (browser context).
 */
export interface JTAGWindowProperties {
  /** JTAGClient or system connection (set by widget init / JTAG system bootstrap) */
  jtag?: {
    connect?: () => Promise<any>;
    commands?: Record<string, any>;
    context?: any;
    [key: string]: any;
  };

  /** Widget daemon instance (alternative to jtag for widget-first init) */
  widgetDaemon?: {
    jtagClient?: any;
    serverClient?: any;
    router?: { commands?: any; events?: any; [key: string]: any };
    [key: string]: any;
  };

  /** Events singleton exposed for cross-module access without import cycles */
  JTAGEvents?: typeof EventsClass;

  /** JTAGClient factory singleton */
  JTAGClientFactory?: { getInstance(): any };

  /** Debug verbosity flag */
  JTAG_VERBOSE?: boolean;

  /** Theme registry singleton (set by ThemeWidget init) */
  ThemeRegistry?: { themes?: Map<string, unknown>; currentTheme?: string; [key: string]: unknown };

  /** Monaco editor instance (if loaded) */
  monaco?: { languages?: { typescript?: unknown; [key: string]: unknown }; [key: string]: unknown };

  /** requestIdleCallback (not in all TS lib targets) */
  requestIdleCallback?: (callback: (deadline: { didTimeout: boolean; timeRemaining: () => number }) => void, options?: { timeout: number }) => number;
  cancelIdleCallback?: (handle: number) => void;
}

/**
 * Properties we attach to `globalThis` at runtime (server + browser).
 */
export interface JTAGGlobalProperties {
  /** JTAGClient or system (server-side, set by JTAGSystem bootstrap) */
  jtag?: { context?: any; commands?: any; [key: string]: any };

  /** Command daemon for server-side direct routing (bypasses WebSocket) */
  __JTAG_COMMAND_DAEMON__?: { commands?: any; [key: string]: any };

  /** Server context string */
  __JTAG_CONTEXT__?: string;

  /** Server session ID */
  __JTAG_SESSION_ID__?: string;

  /** Debug verbosity flag (server-side) */
  JTAG_VERBOSE?: boolean;
}

/* eslint-enable @typescript-eslint/no-explicit-any */

/**
 * Cast helpers — use these instead of `(window as any)`.
 *
 * These are typed narrowing helpers, not runtime-changing casts.
 * They return the same object with augmented type information.
 */
export const jtagWindow = (typeof window !== 'undefined' ? window : undefined) as
  (Window & JTAGWindowProperties) | undefined;

export const jtagGlobal = globalThis as typeof globalThis & JTAGGlobalProperties;

/**
 * Global augmentation — extends Window and globalThis so that
 * `window.JTAG_VERBOSE`, `window.ThemeRegistry`, etc. are type-safe
 * without importing anything or casting to `any`.
 */
declare global {
  interface Window extends JTAGWindowProperties {}
  // eslint-disable-next-line no-var
  var JTAG_VERBOSE: boolean | undefined;
  // eslint-disable-next-line no-var
  var __JTAG_COMMAND_DAEMON__: JTAGGlobalProperties['__JTAG_COMMAND_DAEMON__'];
  // eslint-disable-next-line no-var
  var __JTAG_CONTEXT__: string | undefined;
  // eslint-disable-next-line no-var
  var __JTAG_SESSION_ID__: string | undefined;
}
