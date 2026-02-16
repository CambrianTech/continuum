/**
 * Help Command - Shared Types
 *
 * Discover and display help documentation from command READMEs, auto-generating templates for gaps
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../system/core/shared/Commands';

/**
 * A discovered help topic (command group or individual command)
 */
export interface HelpTopic {
  /** Command path (e.g., 'interface', 'interface/screenshot') */
  path: string;
  /** Human-readable title */
  title: string;
  /** Whether a README.md exists for this topic */
  hasReadme: boolean;
  /** List of subcommands under this path */
  commands?: string[];
}

/**
 * Help Command Parameters
 */
export interface HelpParams extends CommandParams {
  // Command path (e.g., 'interface', 'interface/screenshot')
  path?: string;
  // Output format for different consumers
  format?: 'markdown' | 'json' | 'rag';
  // List all available help topics
  list?: boolean;
}

/**
 * Factory function for creating HelpParams
 */
export const createHelpParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Command path (e.g., 'interface', 'interface/screenshot')
    path?: string;
    // Output format for different consumers
    format?: 'markdown' | 'json' | 'rag';
    // List all available help topics
    list?: boolean;
  }
): HelpParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  path: data.path ?? '',
  format: data.format ?? undefined,
  list: data.list ?? false,
  ...data
});

/**
 * Help Command Result
 */
export interface HelpResult extends CommandResult {
  success: boolean;
  // The help path that was queried
  path: string;
  // The help content (markdown, json, or condensed for RAG)
  content: string;
  // List of available help topics (when list=true)
  topics: HelpTopic[];
  // Whether the content was auto-generated (no README found)
  generated: boolean;
  // The format used for output
  format: string;
  error?: JTAGError;
}

/**
 * Factory function for creating HelpResult with defaults
 */
export const createHelpResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The help path that was queried
    path?: string;
    // The help content (markdown, json, or condensed for RAG)
    content?: string;
    // List of available help topics (when list=true)
    topics?: HelpTopic[];
    // Whether the content was auto-generated (no README found)
    generated?: boolean;
    // The format used for output
    format?: string;
    error?: JTAGError;
  }
): HelpResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  path: data.path ?? '',
  content: data.content ?? '',
  topics: data.topics ?? [],
  generated: data.generated ?? false,
  format: data.format ?? '',
  ...data
});

/**
 * Smart Help-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createHelpResultFromParams = (
  params: HelpParams,
  differences: Omit<HelpResult, 'context' | 'sessionId'>
): HelpResult => transformPayload(params, differences);

/**
 * Help — Type-safe command executor
 *
 * Usage:
 *   import { Help } from '...shared/HelpTypes';
 *   const result = await Help.execute({ ... });
 */
export const Help = {
  execute(params: CommandInput<HelpParams>): Promise<HelpResult> {
    return Commands.execute<HelpParams, HelpResult>('help', params as Partial<HelpParams>);
  },
  commandName: 'help' as const,
} as const;
