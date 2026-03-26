/**
 * Development Verify Web Command - Server Implementation
 *
 * Opens a web page in headless Playwright, captures console errors + screenshot.
 * Used by Academy teacher to grade coding output. No blind training.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { DevelopmentVerifyWebParams, DevelopmentVerifyWebResult } from '../shared/DevelopmentVerifyWebTypes';
import { createDevelopmentVerifyWebResultFromParams } from '../shared/DevelopmentVerifyWebTypes';
import { existsSync } from 'fs';
import { resolve } from 'path';

export class DevelopmentVerifyWebServerCommand extends CommandBase<DevelopmentVerifyWebParams, DevelopmentVerifyWebResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/verify-web', context, subpath, commander);
  }

  async execute(params: DevelopmentVerifyWebParams): Promise<DevelopmentVerifyWebResult> {
    const startTime = Date.now();

    // Validate: need filePath or url
    if (!params.filePath && !params.url) {
      throw new ValidationError(
        'filePath',
        'Either filePath or url is required. See development/verify-web README.'
      );
    }

    // Resolve URL
    let targetUrl: string;
    if (params.filePath) {
      const absPath = resolve(params.filePath);
      if (!existsSync(absPath)) {
        throw new ValidationError('filePath', `File not found: ${absPath}`);
      }
      targetUrl = `file://${absPath}`;
    } else {
      targetUrl = params.url!;
    }

    const waitMs = params.waitMs ?? 2000;
    const doScreenshot = params.screenshot !== false;
    const viewport = params.viewport ?? '1280x720';
    const [vw, vh] = viewport.split('x').map(Number);

    // Dynamic import — playwright is an optional dependency
    let chromium: typeof import('playwright').chromium;
    try {
      const pw = await import('playwright');
      chromium = pw.chromium;
    } catch {
      return createDevelopmentVerifyWebResultFromParams(params, {
        success: false,
        errors: ['Playwright not installed. Run: npm install playwright'],
        consoleOutput: [],
        screenshotPath: '',
        screenshotBase64: '',
        pageTitle: '',
        loadTimeMs: Date.now() - startTime,
      });
    }

    const errors: string[] = [];
    const consoleOutput: string[] = [];
    let screenshotPath = '';
    let screenshotBase64 = '';
    let pageTitle = '';

    let browser;
    try {
      browser = await chromium.launch({ headless: true });
      const page = await browser.newPage({ viewport: { width: vw || 1280, height: vh || 720 } });

      // Capture errors + console
      page.on('pageerror', (err: Error) => errors.push(err.message));
      page.on('console', (msg: import('playwright').ConsoleMessage) => {
        const text = `[${msg.type()}] ${msg.text()}`;
        consoleOutput.push(text);
        if (msg.type() === 'error') errors.push(msg.text());
      });

      await page.goto(targetUrl, { waitUntil: 'domcontentloaded' });
      await page.waitForTimeout(waitMs);

      pageTitle = await page.title();

      if (doScreenshot) {
        const ssPath = params.screenshotPath ?? `/tmp/verify-web-${Date.now()}.png`;
        const buffer = await page.screenshot({ path: ssPath, fullPage: false });
        screenshotPath = ssPath;
        screenshotBase64 = buffer.toString('base64');
      }

    } catch (err) {
      errors.push(`Browser error: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      if (browser) await browser.close();
    }

    const loadTimeMs = Date.now() - startTime;

    return createDevelopmentVerifyWebResultFromParams(params, {
      success: errors.length === 0,
      errors,
      consoleOutput,
      screenshotPath,
      screenshotBase64,
      pageTitle,
      loadTimeMs,
    });
  }
}
