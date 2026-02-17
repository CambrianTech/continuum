/**
 * Interface Page Forms Command - Shared Types
 *
 * Discover all forms on a web page. Returns structured form definitions with field names,
 * types, labels, and submit buttons. Works on ANY page with HTML forms - no WebMCP required.
 * Use this first to understand what you can interact with, then use interface/page/fill
 * and interface/page/submit.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Form field definition - describes an input in a form
 */
export interface FormField {
  /** Field name attribute */
  name: string;
  /** Input type (text, email, number, select, textarea, etc.) */
  type: string;
  /** Human-readable label */
  label: string;
  /** Whether field is required */
  required: boolean;
  /** Placeholder text hint */
  placeholder?: string;
  /** Current value if pre-filled */
  value?: string;
  /** For select fields, the available options */
  options?: string[];
}

/**
 * Submit button definition
 */
export interface SubmitButton {
  /** Button text */
  text: string;
  /** CSS selector to target this button */
  selector: string;
}

/**
 * Form definition - describes a complete form on the page
 */
export interface FormDefinition {
  /** Unique identifier for this form (use in fill/submit commands) */
  formId: string;
  /** Human-readable form name from aria-label, title, or inferred from content */
  name: string;
  /** Form action URL */
  action: string;
  /** HTTP method (GET/POST) */
  method: string;
  /** Input fields in this form */
  fields: FormField[];
  /** Submit button if found */
  submitButton?: SubmitButton;
}

/**
 * Interface Page Forms Command Parameters
 */
export interface InterfacePageFormsParams extends CommandParams {
  /** The URL of the page to analyze */
  url: string;
  /** CSS selector to wait for before analyzing (useful for dynamic pages) */
  waitForSelector?: string;
}

/**
 * Factory function for creating InterfacePageFormsParams
 */
export const createInterfacePageFormsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    url: string;
    waitForSelector?: string;
  }
): InterfacePageFormsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Interface Page Forms Command Result
 */
export interface InterfacePageFormsResult extends CommandResult {
  /** Whether form discovery succeeded */
  success: boolean;
  /** The final URL after any redirects */
  pageUrl: string;
  /** The page title */
  pageTitle: string;
  /** Array of discovered forms with their fields */
  forms: FormDefinition[];
  /** Guidance on what to do next based on discovered forms */
  hint: string;
  /** Error message if discovery failed */
  errorMessage?: string;
  /** Structured error */
  error?: JTAGError;
}

/**
 * Factory function for creating InterfacePageFormsResult with defaults
 */
export const createInterfacePageFormsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    pageUrl?: string;
    pageTitle?: string;
    forms?: FormDefinition[];
    hint?: string;
    errorMessage?: string;
    error?: JTAGError;
  }
): InterfacePageFormsResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: data.success,
  pageUrl: data.pageUrl ?? '',
  pageTitle: data.pageTitle ?? '',
  forms: data.forms ?? [],
  hint: data.hint ?? '',
  errorMessage: data.errorMessage,
  error: data.error,
});

/**
 * Smart Interface Page Forms-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfacePageFormsResultFromParams = (
  params: InterfacePageFormsParams,
  differences: Omit<InterfacePageFormsResult, 'context' | 'sessionId'>
): InterfacePageFormsResult => transformPayload(params, differences);

/**
 * Interface Page Forms - Type-safe command executor
 *
 * Usage:
 *   import { InterfacePageForms } from '...shared/InterfacePageFormsTypes';
 *   const result = await InterfacePageForms.execute({ url: 'https://example.com' });
 */
export const InterfacePageForms = {
  execute(params: CommandInput<InterfacePageFormsParams>): Promise<InterfacePageFormsResult> {
    return Commands.execute<InterfacePageFormsParams, InterfacePageFormsResult>('interface/page/forms', params as Partial<InterfacePageFormsParams>);
  },
  commandName: 'interface/page/forms' as const,
} as const;
