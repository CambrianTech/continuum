/**
 * Interface Page Fill Command - Shared Types
 *
 * Fill form fields on a web page. Use interface/page/forms first to discover available forms
 * and their fields. This command fills fields but does NOT submit - use interface/page/submit
 * after filling.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Error for a field that could not be filled
 */
export interface FieldError {
  /** Field name */
  name: string;
  /** Why the field could not be filled */
  reason: string;
}

/**
 * Interface Page Fill Command Parameters
 */
export interface InterfacePageFillParams extends CommandParams {
  /** The URL of the page containing the form */
  url: string;
  /** The formId from interface/page/forms response */
  formId: string;
  /** Object mapping field names to values, e.g. {"from": "NYC", "to": "LAX"} */
  values: Record<string, string>;
  /** CSS selector to wait for before filling */
  waitForSelector?: string;
}

/**
 * Factory function for creating InterfacePageFillParams
 */
export const createInterfacePageFillParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    url: string;
    formId: string;
    values: Record<string, string>;
    waitForSelector?: string;
  }
): InterfacePageFillParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Interface Page Fill Command Result
 */
export interface InterfacePageFillResult extends CommandResult {
  /** Whether all fields were filled successfully */
  success: boolean;
  /** The form that was filled */
  formId: string;
  /** List of field names that were successfully filled */
  filledFields: string[];
  /** Fields that could not be filled, with reasons */
  failedFields: FieldError[];
  /** Required fields that still need values */
  remainingRequired: string[];
  /** Guidance on next steps (submit if ready, or fill remaining required fields) */
  hint: string;
  /** Error message if operation failed entirely */
  errorMessage?: string;
  /** Structured error */
  error?: JTAGError;
}

/**
 * Factory function for creating InterfacePageFillResult with defaults
 */
export const createInterfacePageFillResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    formId?: string;
    filledFields?: string[];
    failedFields?: FieldError[];
    remainingRequired?: string[];
    hint?: string;
    errorMessage?: string;
    error?: JTAGError;
  }
): InterfacePageFillResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: data.success,
  formId: data.formId ?? '',
  filledFields: data.filledFields ?? [],
  failedFields: data.failedFields ?? [],
  remainingRequired: data.remainingRequired ?? [],
  hint: data.hint ?? '',
  errorMessage: data.errorMessage,
  error: data.error,
});

/**
 * Smart Interface Page Fill-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfacePageFillResultFromParams = (
  params: InterfacePageFillParams,
  differences: Omit<InterfacePageFillResult, 'context' | 'sessionId'>
): InterfacePageFillResult => transformPayload(params, differences);

/**
 * Interface Page Fill - Type-safe command executor
 *
 * Usage:
 *   import { InterfacePageFill } from '...shared/InterfacePageFillTypes';
 *   const result = await InterfacePageFill.execute({
 *     url: 'https://example.com',
 *     formId: 'search-form',
 *     values: { query: 'test' }
 *   });
 */
export const InterfacePageFill = {
  execute(params: CommandInput<InterfacePageFillParams>): Promise<InterfacePageFillResult> {
    return Commands.execute<InterfacePageFillParams, InterfacePageFillResult>('interface/page/fill', params as Partial<InterfacePageFillParams>);
  },
  commandName: 'interface/page/fill' as const,
} as const;
