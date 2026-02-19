/**
 * HTML Inspector Types - Shadow DOM debugging
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface HtmlInspectorParams extends CommandParams {
  selector: string;
  includeStyles?: boolean;
  maxDepth?: number;
}

export interface HtmlInspectorResult extends CommandResult {
  success: boolean;
  error?: string;
  html: string;
  text: string;
  structure: any;
  tagName?: string;
  className?: string;
  id?: string;
  dimensions?: {
    width: number;
    height: number;
    top: number;
    left: number;
    bottom: number;
    right: number;
  };
  computedStyles?: {
    display: string;
    position: string;
    flexDirection: string;
    height: string;
    minHeight: string;
    maxHeight: string;
    overflow: string;
    overflowY: string;
    flex: string;
    flexGrow: string;
    flexShrink: string;
    flexBasis: string;
  };
}

export const createHtmlInspectorResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<HtmlInspectorResult>, 'context' | 'sessionId'>
): HtmlInspectorResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: false,
  html: '',
  text: '',
  structure: {},
  ...data
});
/**
 * HtmlInspector — Type-safe command executor
 *
 * Usage:
 *   import { HtmlInspector } from '...shared/HtmlInspectorTypes';
 *   const result = await HtmlInspector.execute({ ... });
 */
export const HtmlInspector = {
  execute(params: CommandInput<HtmlInspectorParams>): Promise<HtmlInspectorResult> {
    return Commands.execute<HtmlInspectorParams, HtmlInspectorResult>('development/debug/html-inspector', params as Partial<HtmlInspectorParams>);
  },
  commandName: 'development/debug/html-inspector' as const,
} as const;
