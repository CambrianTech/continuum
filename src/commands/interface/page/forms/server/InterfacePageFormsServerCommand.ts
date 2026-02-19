/**
 * Interface Page Forms Command - Server Implementation
 *
 * Discover all forms on a web page using puppeteer. Works on ANY page with HTML forms.
 * Returns structured form definitions that guide the AI on what to fill and submit.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InterfacePageFormsParams, InterfacePageFormsResult, FormDefinition, FormField, SubmitButton } from '../shared/InterfacePageFormsTypes';
import { createInterfacePageFormsResultFromParams } from '../shared/InterfacePageFormsTypes';

export class InterfacePageFormsServerCommand extends CommandBase<InterfacePageFormsParams, InterfacePageFormsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/page/forms', context, subpath, commander);
  }

  async execute(params: InterfacePageFormsParams): Promise<InterfacePageFormsResult> {
    console.log('🔧 SERVER: Discovering forms on page', params.url);

    // Validate required parameters
    if (!params.url || params.url.trim() === '') {
      throw new ValidationError(
        'url',
        'Missing required parameter "url". Provide the URL of the page to analyze for forms.'
      );
    }

    // Check if puppeteer is available
    const puppeteerCheck = await this.checkPuppeteer();
    if (!puppeteerCheck.available) {
      return createInterfacePageFormsResultFromParams(params, {
        success: false,
        pageUrl: params.url,
        pageTitle: '',
        forms: [],
        hint: '',
        errorMessage: puppeteerCheck.reason,
      });
    }

    try {
      const result = await this.discoverFormsWithPuppeteer(params.url, params.waitForSelector);

      // Generate a helpful hint based on what we found
      const hint = this.generateHint(result.forms);

      return createInterfacePageFormsResultFromParams(params, {
        success: true,
        pageUrl: result.pageUrl,
        pageTitle: result.pageTitle,
        forms: result.forms,
        hint,
      });
    } catch (error) {
      return createInterfacePageFormsResultFromParams(params, {
        success: false,
        pageUrl: params.url,
        pageTitle: '',
        forms: [],
        hint: '',
        errorMessage: `Failed to discover forms: ${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  /**
   * Generate a helpful hint based on discovered forms
   */
  private generateHint(forms: FormDefinition[]): string {
    if (forms.length === 0) {
      return 'No forms found on this page. The page may use JavaScript-based interactions instead of HTML forms.';
    }

    if (forms.length === 1) {
      const form = forms[0];
      const requiredFields = form.fields.filter(f => f.required).map(f => f.name);
      if (requiredFields.length > 0) {
        return `Found 1 form "${form.name}" (id: ${form.formId}). Required fields: ${requiredFields.join(', ')}. Use interface/page/fill --formId="${form.formId}" --values='{...}' to fill it, then interface/page/submit to submit.`;
      }
      return `Found 1 form "${form.name}" (id: ${form.formId}). Use interface/page/fill --formId="${form.formId}" --values='{...}' to fill it, then interface/page/submit to submit.`;
    }

    const formNames = forms.map(f => `"${f.name}" (${f.formId})`).join(', ');
    return `Found ${forms.length} forms: ${formNames}. Choose a form and use interface/page/fill --formId="<formId>" --values='{...}' to fill it.`;
  }

  private async checkPuppeteer(): Promise<{ available: boolean; reason: string }> {
    try {
      require.resolve('puppeteer');
      return { available: true, reason: '' };
    } catch {
      try {
        require.resolve('puppeteer-core');
        return { available: true, reason: '' };
      } catch {
        return {
          available: false,
          reason: 'Puppeteer not installed. Run: npm install puppeteer-core',
        };
      }
    }
  }

  private async discoverFormsWithPuppeteer(
    url: string,
    waitForSelector?: string
  ): Promise<{ pageUrl: string; pageTitle: string; forms: FormDefinition[] }> {
    // Dynamic import
    let puppeteer: typeof import('puppeteer-core');
    try {
      puppeteer = await import('puppeteer-core');
    } catch {
      puppeteer = await import('puppeteer') as unknown as typeof import('puppeteer-core');
    }

    // Try to find Chrome/Chromium
    const executablePath = await this.findChrome();

    const browser = await puppeteer.launch({
      executablePath,
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox'],
    });

    try {
      const page = await browser.newPage();
      await page.goto(url, { waitUntil: 'networkidle2', timeout: 30000 });

      // Wait for optional selector
      if (waitForSelector) {
        await page.waitForSelector(waitForSelector, { timeout: 10000 }).catch(() => {
          console.warn(`Selector "${waitForSelector}" not found within timeout`);
        });
      }

      const pageUrl = page.url();
      const pageTitle = await page.title();

      // Extract forms in page context
      const forms = await page.evaluate(() => {
        const formElements = document.querySelectorAll('form');
        const results: FormDefinition[] = [];

        formElements.forEach((form, index) => {
          // Generate a unique formId
          const formId = form.id || form.name || `form-${index}`;

          // Get form name from various sources
          const name = form.getAttribute('aria-label') ||
                       form.getAttribute('title') ||
                       form.querySelector('legend')?.textContent?.trim() ||
                       form.querySelector('h1, h2, h3, h4')?.textContent?.trim() ||
                       `Form ${index + 1}`;

          // Get form action and method
          const action = form.action || '';
          const method = (form.method || 'GET').toUpperCase();

          // Extract fields
          const fields: FormField[] = [];
          const inputs = form.querySelectorAll('input, select, textarea');

          inputs.forEach((input) => {
            const inputEl = input as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
            const fieldName = inputEl.name || inputEl.id || '';
            if (!fieldName) return; // Skip inputs without names

            // Skip hidden and submit inputs
            if (inputEl.type === 'hidden' || inputEl.type === 'submit' || inputEl.type === 'button') {
              return;
            }

            // Find label
            let label = '';
            const labelEl = form.querySelector(`label[for="${inputEl.id}"]`);
            if (labelEl) {
              label = labelEl.textContent?.trim() || '';
            } else {
              // Check for parent label
              const parentLabel = inputEl.closest('label');
              if (parentLabel) {
                label = parentLabel.textContent?.trim() || '';
              }
            }
            // Fallback to placeholder or aria-label
            if (!label) {
              label = inputEl.getAttribute('aria-label') ||
                      inputEl.getAttribute('placeholder') ||
                      fieldName;
            }

            const field: FormField = {
              name: fieldName,
              type: inputEl.tagName === 'SELECT' ? 'select' :
                    inputEl.tagName === 'TEXTAREA' ? 'textarea' :
                    (inputEl as HTMLInputElement).type || 'text',
              label,
              required: inputEl.required || inputEl.hasAttribute('aria-required'),
            };

            // Add placeholder if present
            if ('placeholder' in inputEl && inputEl.placeholder) {
              field.placeholder = inputEl.placeholder;
            }

            // Add current value if present
            if (inputEl.value) {
              field.value = inputEl.value;
            }

            // For select elements, get options
            if (inputEl.tagName === 'SELECT') {
              const selectEl = inputEl as HTMLSelectElement;
              field.options = Array.from(selectEl.options).map(opt => opt.value || opt.textContent || '');
            }

            fields.push(field);
          });

          // Find submit button
          let submitButton: SubmitButton | undefined;
          const submitEl = form.querySelector('button[type="submit"], input[type="submit"], button:not([type])');
          if (submitEl) {
            const buttonText = submitEl.textContent?.trim() ||
                              (submitEl as HTMLInputElement).value ||
                              'Submit';
            // Generate a unique selector
            let selector = '';
            if (submitEl.id) {
              selector = `#${submitEl.id}`;
            } else if (submitEl.className) {
              selector = `form[id="${formId}"] button.${submitEl.className.split(' ')[0]}`;
            } else {
              selector = `form[id="${formId}"] button[type="submit"], form[id="${formId}"] input[type="submit"]`;
            }
            submitButton = { text: buttonText, selector };
          }

          results.push({
            formId,
            name,
            action,
            method,
            fields,
            submitButton,
          });
        });

        return results;
      });

      return { pageUrl, pageTitle, forms };
    } finally {
      await browser.close();
    }
  }

  private async findChrome(): Promise<string> {
    const { execSync } = await import('child_process');
    const platform = process.platform;

    // Check common locations
    const paths: string[] = [];
    if (platform === 'darwin') {
      paths.push(
        '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        '/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary',
        '/Applications/Chromium.app/Contents/MacOS/Chromium',
      );
    } else if (platform === 'linux') {
      paths.push(
        '/usr/bin/google-chrome',
        '/usr/bin/google-chrome-stable',
        '/usr/bin/chromium',
        '/usr/bin/chromium-browser',
      );
    } else if (platform === 'win32') {
      const username = process.env.USERNAME || '';
      paths.push(
        `C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe`,
        `C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe`,
        `C:\\Users\\${username}\\AppData\\Local\\Google\\Chrome\\Application\\chrome.exe`,
      );
    }

    for (const path of paths) {
      try {
        if (platform === 'win32') {
          execSync(`where "${path}" 2>nul`);
        } else {
          execSync(`test -f "${path}"`);
        }
        return path;
      } catch {
        // Try next path
      }
    }

    throw new Error(
      'Chrome/Chromium not found. Install Chrome or set PUPPETEER_EXECUTABLE_PATH environment variable.'
    );
  }
}
