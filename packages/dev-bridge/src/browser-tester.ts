/**
 * Browser automation for web app testing and screenshots
 */

import puppeteer, { Browser, Page } from 'puppeteer';
import { BrowserTestResult, ConsoleMessage, ScreenshotOptions } from './types';

export class BrowserTester {
  private browser: Browser | null = null;
  private page: Page | null = null;

  async launch(headless: boolean = true): Promise<void> {
    this.browser = await puppeteer.launch({ 
      headless,
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    this.page = await this.browser.newPage();
  }

  async test(url: string, options: { 
    screenshot?: boolean;
    screenshotOptions?: ScreenshotOptions;
    waitTime?: number;
  } = {}): Promise<BrowserTestResult> {
    if (!this.page) {
      throw new Error('Browser not launched. Call launch() first.');
    }

    const console: ConsoleMessage[] = [];
    const errors: string[] = [];
    
    // Capture console messages
    this.page.on('console', (msg) => {
      console.push({
        type: msg.type() as any,
        text: msg.text(),
        timestamp: Date.now()
      });
    });

    // Capture page errors
    this.page.on('pageerror', (error) => {
      errors.push(error.message);
    });

    const startTime = Date.now();
    
    try {
      await this.page.goto(url, { waitUntil: 'networkidle0' });
      
      // Wait additional time if specified
      if (options.waitTime) {
        await this.page.waitForTimeout(options.waitTime);
      }

      const loadTime = Date.now() - startTime;

      let screenshot: string | undefined;
      if (options.screenshot) {
        const screenshotBuffer = await this.page.screenshot({
          fullPage: options.screenshotOptions?.fullPage ?? false,
          clip: options.screenshotOptions?.clip,
        });
        screenshot = screenshotBuffer.toString('base64');
      }

      return {
        url,
        screenshot,
        console,
        errors,
        performance: {
          loadTime,
          domContentLoaded: loadTime // Simplified for now
        }
      };
    } catch (error) {
      errors.push(error instanceof Error ? error.message : String(error));
      return {
        url,
        console,
        errors,
      };
    }
  }

  async screenshot(url: string, outputPath: string, options: ScreenshotOptions = {}): Promise<void> {
    if (!this.page) {
      throw new Error('Browser not launched. Call launch() first.');
    }

    await this.page.goto(url);
    await this.page.setViewport({
      width: options.width ?? 1280,
      height: options.height ?? 720
    });

    await this.page.screenshot({
      path: outputPath,
      fullPage: options.fullPage ?? false,
      clip: options.clip
    });
  }

  async close(): Promise<void> {
    if (this.browser) {
      await this.browser.close();
      this.browser = null;
      this.page = null;
    }
  }
}