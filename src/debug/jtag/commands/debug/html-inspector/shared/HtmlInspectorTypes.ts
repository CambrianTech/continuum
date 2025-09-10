/**
 * HTML Inspector Types - Shadow DOM debugging
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

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
}

export const createHtmlInspectorResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<HtmlInspectorResult>, 'context' | 'sessionId'>
): HtmlInspectorResult => createPayload(context, sessionId, {
  success: false,
  html: '',
  text: '',
  structure: {},
  ...data
});