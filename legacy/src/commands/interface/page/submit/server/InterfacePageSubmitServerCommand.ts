/**
 * Interface Page Submit Command - Server Implementation
 *
 * Submit a form on a web page using puppeteer. Optionally fill values before submitting.
 * Returns the resulting page state after submission.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InterfacePageSubmitParams, InterfacePageSubmitResult } from '../shared/InterfacePageSubmitTypes';
import { createInterfacePageSubmitResultFromParams } from '../shared/InterfacePageSubmitTypes';
import { checkPuppeteer, launchAndNavigate, getFormSelector } from '../../shared/PuppeteerHelper';

export class InterfacePageSubmitServerCommand extends CommandBase<InterfacePageSubmitParams, InterfacePageSubmitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/page/submit', context, subpath, commander);
  }

  async execute(params: InterfacePageSubmitParams): Promise<InterfacePageSubmitResult> {
    console.log('🔧 SERVER: Submitting form', params.formId, 'on', params.url);

    // Validate required parameters
    if (!params.url || params.url.trim() === '') {
      throw new ValidationError('url', 'Missing required parameter "url".');
    }
    if (!params.formId || params.formId.trim() === '') {
      throw new ValidationError('formId', 'Missing required parameter "formId". Use interface/page/forms to discover available forms.');
    }

    // Check if puppeteer is available
    const puppeteerCheck = await checkPuppeteer();
    if (!puppeteerCheck.available) {
      return createInterfacePageSubmitResultFromParams(params, {
        success: false,
        formId: params.formId,
        navigatedTo: '',
        pageTitle: '',
        pageContent: '',
        hasMoreForms: false,
        hint: '',
        errorMessage: puppeteerCheck.reason,
      });
    }

    try {
      const result = await this.submitFormWithPuppeteer(
        params.url,
        params.formId,
        params.values,
        params.waitForNavigation ?? true,
        params.waitForSelector
      );

      return createInterfacePageSubmitResultFromParams(params, result);
    } catch (error) {
      return createInterfacePageSubmitResultFromParams(params, {
        success: false,
        formId: params.formId,
        navigatedTo: '',
        pageTitle: '',
        pageContent: '',
        hasMoreForms: false,
        hint: '',
        errorMessage: `Failed to submit form: ${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  private async submitFormWithPuppeteer(
    url: string,
    formId: string,
    values?: Record<string, string>,
    waitForNavigation: boolean = true,
    waitForSelector?: string
  ): Promise<Omit<InterfacePageSubmitResult, 'context' | 'sessionId'>> {
    const { browser, page } = await launchAndNavigate(url);

    try {
      const formSelector = getFormSelector(formId);

      // Check if form exists
      const formExists = await page.$(formSelector);
      if (!formExists) {
        return {
          success: false,
          formId,
          navigatedTo: '',
          pageTitle: '',
          pageContent: '',
          hasMoreForms: false,
          hint: '',
          errorMessage: `Form "${formId}" not found on page. Use interface/page/forms to discover available forms.`,
        };
      }

      // Fill values if provided - use puppeteer's type() for proper React/Vue compatibility
      if (values && Object.keys(values).length > 0) {
        for (const [fieldName, value] of Object.entries(values)) {
          const fieldSelector = `${formSelector} [name="${fieldName}"]`;
          const field = await page.$(fieldSelector);
          if (field) {
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
        }
      }

      // Submit the form
      const originalUrl = page.url();

      if (waitForNavigation) {
        // Submit and wait for navigation
        await Promise.all([
          page.waitForNavigation({ waitUntil: 'networkidle2', timeout: 30000 }).catch(() => null),
          page.evaluate((selector: string) => {
            const form = document.querySelector(selector) as HTMLFormElement | null;
            if (form) {
              const submitBtn = form.querySelector('button[type="submit"], input[type="submit"], button:not([type])') as HTMLButtonElement | null;
              if (submitBtn) {
                submitBtn.click();
              } else {
                form.submit();
              }
            }
          }, formSelector),
        ]);
      } else {
        // Just submit without waiting
        await page.evaluate((selector: string) => {
          const form = document.querySelector(selector) as HTMLFormElement | null;
          if (form) {
            const submitBtn = form.querySelector('button[type="submit"], input[type="submit"], button:not([type])') as HTMLButtonElement | null;
            if (submitBtn) {
              submitBtn.click();
            } else {
              form.submit();
            }
          }
        }, formSelector);
        // Small delay to let any client-side handling complete
        await new Promise(resolve => setTimeout(resolve, 1000));
      }

      // Wait for selector if specified
      if (waitForSelector) {
        await page.waitForSelector(waitForSelector, { timeout: 10000 }).catch(() => {
          console.warn(`Selector "${waitForSelector}" not found within timeout`);
        });
      }

      // Get result page info
      const navigatedTo = page.url();
      const pageTitle = await page.title();

      // Get page content summary
      const pageContent = await page.evaluate(() => {
        const body = document.body;
        if (!body) return '';
        // Get visible text, trim whitespace, limit to 500 chars
        const text = body.innerText || '';
        return text.replace(/\s+/g, ' ').trim().substring(0, 500);
      });

      // Check if result page has forms
      const hasMoreForms = await page.evaluate(() => {
        return document.querySelectorAll('form').length > 0;
      });

      // Generate hint
      const hint = this.generateHint(originalUrl, navigatedTo, hasMoreForms, pageTitle);

      return {
        success: true,
        formId,
        navigatedTo,
        pageTitle,
        pageContent,
        hasMoreForms,
        hint,
      };
    } finally {
      await browser.close();
    }
  }

  private generateHint(
    originalUrl: string,
    navigatedTo: string,
    hasMoreForms: boolean,
    pageTitle: string
  ): string {
    const navigated = originalUrl !== navigatedTo;

    if (navigated) {
      if (hasMoreForms) {
        return `Form submitted. Navigated to "${pageTitle}". This page has more forms - use interface/page/forms to discover them.`;
      }
      return `Form submitted successfully. Navigated to "${pageTitle}". Check the pageContent for results.`;
    }

    if (hasMoreForms) {
      return `Form submitted (same page). Page has forms - use interface/page/forms if you need to interact with them.`;
    }

    return `Form submitted. Page updated. Check pageContent for results.`;
  }
}
