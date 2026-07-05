/**
 * Interface Page Fill Command - Server Implementation
 *
 * Fill form fields on a web page using puppeteer. Use interface/page/forms first
 * to discover available forms and their fields.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InterfacePageFillParams, InterfacePageFillResult, FieldError } from '../shared/InterfacePageFillTypes';
import { createInterfacePageFillResultFromParams } from '../shared/InterfacePageFillTypes';
import { checkPuppeteer, launchAndNavigate, getFormSelector } from '../../shared/PuppeteerHelper';

export class InterfacePageFillServerCommand extends CommandBase<InterfacePageFillParams, InterfacePageFillResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/page/fill', context, subpath, commander);
  }

  async execute(params: InterfacePageFillParams): Promise<InterfacePageFillResult> {
    console.log('🔧 SERVER: Filling form fields', params.formId, 'on', params.url);

    // Validate required parameters
    if (!params.url || params.url.trim() === '') {
      throw new ValidationError('url', 'Missing required parameter "url".');
    }
    if (!params.formId || params.formId.trim() === '') {
      throw new ValidationError('formId', 'Missing required parameter "formId". Use interface/page/forms to discover available forms.');
    }
    if (!params.values || Object.keys(params.values).length === 0) {
      throw new ValidationError('values', 'Missing required parameter "values". Provide an object mapping field names to values.');
    }

    // Check if puppeteer is available
    const puppeteerCheck = await checkPuppeteer();
    if (!puppeteerCheck.available) {
      return createInterfacePageFillResultFromParams(params, {
        success: false,
        formId: params.formId,
        filledFields: [],
        failedFields: [],
        remainingRequired: [],
        hint: '',
        errorMessage: puppeteerCheck.reason,
      });
    }

    try {
      const result = await this.fillFormWithPuppeteer(
        params.url,
        params.formId,
        params.values,
        params.waitForSelector
      );

      return createInterfacePageFillResultFromParams(params, result);
    } catch (error) {
      return createInterfacePageFillResultFromParams(params, {
        success: false,
        formId: params.formId,
        filledFields: [],
        failedFields: [],
        remainingRequired: [],
        hint: '',
        errorMessage: `Failed to fill form: ${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  private async fillFormWithPuppeteer(
    url: string,
    formId: string,
    values: Record<string, string>,
    waitForSelector?: string
  ): Promise<Omit<InterfacePageFillResult, 'context' | 'sessionId'>> {
    const { browser, page } = await launchAndNavigate(url, waitForSelector);

    try {
      const formSelector = getFormSelector(formId);

      // Check if form exists
      const formExists = await page.$(formSelector);
      if (!formExists) {
        return {
          success: false,
          formId,
          filledFields: [],
          failedFields: [],
          remainingRequired: [],
          hint: '',
          errorMessage: `Form "${formId}" not found on page. Use interface/page/forms to discover available forms.`,
        };
      }

      // Fill form using puppeteer's type() for React/Vue compatibility
      const filledFields: string[] = [];
      const failedFields: FieldError[] = [];

      for (const [fieldName, value] of Object.entries(values)) {
        const fieldSelector = `${formSelector} [name="${fieldName}"]`;
        const field = await page.$(fieldSelector);

        if (!field) {
          failedFields.push({ name: fieldName, reason: `Field "${fieldName}" not found in form` });
          continue;
        }

        try {
          // Get field type
          const tagName = await page.evaluate((sel) => {
            const el = document.querySelector(sel);
            return el?.tagName || '';
          }, fieldSelector);

          if (tagName === 'SELECT') {
            // For select, use evaluate to set value
            const success = await page.evaluate((sel, val) => {
              const selectEl = document.querySelector(sel) as HTMLSelectElement | null;
              if (!selectEl) return false;
              const optionExists = Array.from(selectEl.options).some(opt => opt.value === val || opt.textContent === val);
              if (!optionExists) return false;
              selectEl.value = val;
              selectEl.dispatchEvent(new Event('change', { bubbles: true }));
              return true;
            }, fieldSelector, value);
            if (!success) {
              failedFields.push({ name: fieldName, reason: `Option "${value}" not found in select` });
              continue;
            }
          } else {
            // Get input type to handle special cases
            const inputType = await page.evaluate((sel) => {
              const el = document.querySelector(sel) as HTMLInputElement | null;
              return el?.type || 'text';
            }, fieldSelector);

            // Clear existing value reliably
            await page.evaluate((sel) => {
              const el = document.querySelector(sel) as HTMLInputElement | null;
              if (el) {
                el.value = '';
                el.dispatchEvent(new Event('input', { bubbles: true }));
              }
            }, fieldSelector);

            // For date/number inputs, set value directly then dispatch events
            if (inputType === 'date' || inputType === 'number') {
              await page.evaluate((sel, val) => {
                const el = document.querySelector(sel) as HTMLInputElement | null;
                if (el) {
                  el.value = val;
                  el.dispatchEvent(new Event('input', { bubbles: true }));
                  el.dispatchEvent(new Event('change', { bubbles: true }));
                }
              }, fieldSelector, value);
            } else {
              // For text inputs, use type() for React/Vue compatibility
              await field.click();
              await field.type(value, { delay: 10 });
            }
          }
          filledFields.push(fieldName);
        } catch (e) {
          failedFields.push({ name: fieldName, reason: String(e) });
        }
      }

      // Get remaining required fields
      const remainingRequired = await page.evaluate((selector) => {
        const form = document.querySelector(selector) as HTMLFormElement | null;
        if (!form) return [];
        const remaining: string[] = [];
        const allInputs = form.querySelectorAll('input, select, textarea');
        allInputs.forEach((input) => {
          const el = input as HTMLInputElement;
          if (el.required && !el.value && el.name) {
            remaining.push(el.name);
          }
        });
        return remaining;
      }, formSelector);

      const result = { filledFields, failedFields, remainingRequired };

      // Generate helpful hint
      const hint = this.generateHint(result.filledFields, result.failedFields, result.remainingRequired, formId);

      return {
        success: result.failedFields.length === 0,
        formId,
        filledFields: result.filledFields,
        failedFields: result.failedFields as FieldError[],
        remainingRequired: result.remainingRequired,
        hint,
      };
    } finally {
      await browser.close();
    }
  }

  private generateHint(
    filledFields: string[],
    failedFields: FieldError[],
    remainingRequired: string[],
    formId: string
  ): string {
    if (failedFields.length > 0) {
      const failedNames = failedFields.map(f => f.name).join(', ');
      return `Failed to fill some fields: ${failedNames}. Check field names with interface/page/forms.`;
    }

    if (remainingRequired.length > 0) {
      return `Filled ${filledFields.length} fields. Still need: ${remainingRequired.join(', ')}. Call interface/page/fill again with missing values.`;
    }

    if (filledFields.length > 0) {
      return `Successfully filled ${filledFields.length} fields. Ready to submit! Use interface/page/submit --formId="${formId}" to submit the form.`;
    }

    return 'No fields were filled. Check field names with interface/page/forms.';
  }
}
