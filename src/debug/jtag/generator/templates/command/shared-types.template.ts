/**
 * {{COMMAND_NAME}} Command - Shared Types
 *
 * {{DESCRIPTION}}
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * {{COMMAND_NAME}} Command Parameters
 */
export interface {{CLASS_NAME}}Params extends CommandParams {
{{PARAM_FIELDS}}
}

/**
 * Factory function for creating {{CLASS_NAME}}Params
 */
export const create{{CLASS_NAME}}Params = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<{{CLASS_NAME}}Params>, 'context' | 'sessionId'>
): {{CLASS_NAME}}Params => createPayload(context, sessionId, data);

/**
 * {{COMMAND_NAME}} Command Result
 */
export interface {{CLASS_NAME}}Result extends CommandResult {
  success: boolean;
{{RESULT_FIELDS}}
  error?: JTAGError;
}

/**
 * Factory function for creating {{CLASS_NAME}}Result with defaults
 */
export const create{{CLASS_NAME}}Result = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<{{CLASS_NAME}}Result>, 'context' | 'sessionId'>
): {{CLASS_NAME}}Result => createPayload(context, sessionId, {
  success: false,
  ...data
});

/**
 * Smart {{COMMAND_NAME}}-specific inheritance from params
 * Auto-inherits common fields from params
 * Only specify what changed: success, error, and result-specific fields
 */
export const create{{CLASS_NAME}}ResultFromParams = (
  params: {{CLASS_NAME}}Params,
  differences: Omit<Partial<{{CLASS_NAME}}Result>, 'context' | 'sessionId'>
): {{CLASS_NAME}}Result => transformPayload(params, {
  success: false,
  ...differences
});
