/**
 * Scroll Test Debug Command Types - Clean Testing Interface
 *
 * Animated scroll testing for debugging intersection observers and scroll behaviors.
 * Useful for testing infinite scroll, chat positioning, and scroll restoration.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface ScrollTestParams extends CommandParams {
  readonly target: 'top' | 'bottom' | 'position';
  readonly position?: number;
  readonly behavior?: 'smooth' | 'instant' | 'auto';
  readonly selector?: string;
  readonly waitTime?: number;
  readonly captureMetrics?: boolean;
  readonly repeat?: number; // Repeat scroll action multiple times for intersection observer testing
  readonly preset?: 'chat-top' | 'chat-bottom' | 'instant-top'; // Quick preset shortcuts
}

export interface ScrollTestResult extends CommandResult {
  readonly scrollPerformed: boolean;
  readonly targetElement: string;
  readonly initialPosition: number;
  readonly finalPosition: number;
  readonly scrollDuration?: number;
  readonly metrics?: {
    readonly scrollHeight: number;
    readonly clientHeight: number;
    readonly messagesCount: number;
    readonly sentinelVisible: boolean;
  };
}

// Common scroll targets for debugging
export const SCROLL_TARGETS = {
  CHAT_WIDGET: 'chat-widget .chat-messages',
  MAIN_CONTENT: 'main-widget .main-content',
  SIDEBAR: 'continuum-sidebar .sidebar-content',
  BODY: 'body'
} as const;

// Scroll test presets for common debugging scenarios
export const SCROLL_TEST_PRESETS = {
  CHAT_TO_TOP: {
    target: 'top' as const,
    behavior: 'smooth' as const,
    selector: SCROLL_TARGETS.CHAT_WIDGET,
    captureMetrics: true,
    waitTime: 1000
  },

  CHAT_TO_BOTTOM: {
    target: 'bottom' as const,
    behavior: 'smooth' as const,
    selector: SCROLL_TARGETS.CHAT_WIDGET,
    captureMetrics: true,
    waitTime: 1000
  },

  INSTANT_TOP: {
    target: 'top' as const,
    behavior: 'instant' as const,
    selector: SCROLL_TARGETS.CHAT_WIDGET,
    captureMetrics: true
  }
} as const;

// Helper function to create proper ScrollTestResult
export function createScrollTestResult(
  context: JTAGContext,
  sessionId: string,
  data: Omit<ScrollTestResult, 'context' | 'sessionId'>
): ScrollTestResult {
  return {
    ...data,
    context,
    sessionId
  };
}
/**
 * ScrollTest — Type-safe command executor
 *
 * Usage:
 *   import { ScrollTest } from '...shared/ScrollTestTypes';
 *   const result = await ScrollTest.execute({ ... });
 */
export const ScrollTest = {
  execute(params: CommandInput<ScrollTestParams>): Promise<ScrollTestResult> {
    return Commands.execute<ScrollTestParams, ScrollTestResult>('development/debug/scroll-test', params as Partial<ScrollTestParams>);
  },
  commandName: 'development/debug/scroll-test' as const,
} as const;
