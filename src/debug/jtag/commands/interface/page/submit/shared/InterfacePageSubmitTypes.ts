/**
 * Interface Page Submit Command - Shared Types
 *
 * Submit a form on a web page. Use interface/page/forms to discover forms,
 * interface/page/fill to populate fields, then this command to submit.
 * Returns the resulting page state after submission.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Interface Page Submit Command Parameters
 */
export interface InterfacePageSubmitParams extends CommandParams {
  /** The URL of the page containing the form */
  url: string;
  /** The formId from interface/page/forms response */
  formId: string;
  /** Optional: fill these values before submitting (combines fill + submit) */
  values?: Record<string, string>;
  /** Wait for page navigation after submit (default: true) */
  waitForNavigation?: boolean;
  /** Wait for this selector on the result page */
  waitForSelector?: string;
}

/**
 * Factory function for creating InterfacePageSubmitParams
 */
export const createInterfacePageSubmitParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    url: string;
    formId: string;
    values?: Record<string, string>;
    waitForNavigation?: boolean;
    waitForSelector?: string;
  }
): InterfacePageSubmitParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Interface Page Submit Command Result
 */
export interface InterfacePageSubmitResult extends CommandResult {
  /** Whether form was submitted successfully */
  success: boolean;
  /** The form that was submitted */
  formId: string;
  /** The URL after form submission (may be same page or new page) */
  navigatedTo: string;
  /** Title of the resulting page */
  pageTitle: string;
  /** Brief summary of the result page content (first 500 chars of visible text) */
  pageContent: string;
  /** Whether the result page has forms (call interface/page/forms to discover) */
  hasMoreForms: boolean;
  /** Guidance on what to do with the result page */
  hint: string;
  /** Error message if submission failed */
  errorMessage?: string;
  /** Structured error */
  error?: JTAGError;
}

/**
 * Factory function for creating InterfacePageSubmitResult with defaults
 */
export const createInterfacePageSubmitResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    formId?: string;
    navigatedTo?: string;
    pageTitle?: string;
    pageContent?: string;
    hasMoreForms?: boolean;
    hint?: string;
    errorMessage?: string;
    error?: JTAGError;
  }
): InterfacePageSubmitResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: data.success,
  formId: data.formId ?? '',
  navigatedTo: data.navigatedTo ?? '',
  pageTitle: data.pageTitle ?? '',
  pageContent: data.pageContent ?? '',
  hasMoreForms: data.hasMoreForms ?? false,
  hint: data.hint ?? '',
  errorMessage: data.errorMessage,
  error: data.error,
});

/**
 * Smart Interface Page Submit-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfacePageSubmitResultFromParams = (
  params: InterfacePageSubmitParams,
  differences: Omit<InterfacePageSubmitResult, 'context' | 'sessionId'>
): InterfacePageSubmitResult => transformPayload(params, differences);

/**
 * Interface Page Submit - Type-safe command executor
 *
 * Usage:
 *   import { InterfacePageSubmit } from '...shared/InterfacePageSubmitTypes';
 *   const result = await InterfacePageSubmit.execute({
 *     url: 'https://example.com',
 *     formId: 'search-form'
 *   });
 */
export const InterfacePageSubmit = {
  execute(params: CommandInput<InterfacePageSubmitParams>): Promise<InterfacePageSubmitResult> {
    return Commands.execute<InterfacePageSubmitParams, InterfacePageSubmitResult>('interface/page/submit', params as Partial<InterfacePageSubmitParams>);
  },
  commandName: 'interface/page/submit' as const,
} as const;
