/**
 * VisualSentinel - Takes screenshots of generated content for visual feedback
 *
 * Uses Puppeteer to:
 * 1. Launch headless Chrome
 * 2. Navigate to local file or URL
 * 3. Take screenshot
 * 4. Return image path
 */

import * as path from 'path';
import * as fs from 'fs';
import { launchAndNavigate, checkPuppeteer } from '../../commands/interface/page/shared/PuppeteerHelper';
import type { ScreenshotSentinelDefinition } from './SentinelDefinition';

export interface VisualSentinelConfig {
  outputDir: string;
  viewport?: { width: number; height: number };
}

export interface ScreenshotResult {
  success: boolean;
  imagePath?: string;
  error?: string;
}

export class VisualSentinel {
  private config: Required<VisualSentinelConfig>;
  private _target?: string;
  private _filename?: string;

  constructor(config: VisualSentinelConfig) {
    this.config = {
      viewport: { width: 800, height: 600 },
      ...config,
    };
  }

  /**
   * Create a VisualSentinel from a portable definition
   */
  static fromDefinition(def: ScreenshotSentinelDefinition): VisualSentinel {
    const sentinel = new VisualSentinel({
      outputDir: def.outputDir || '/tmp/sentinel-screenshots',
      viewport: def.viewport,
    });
    sentinel._target = def.target;
    sentinel._filename = def.filename;
    return sentinel;
  }

  /**
   * Export to portable JSON definition
   */
  toDefinition(name?: string, target?: string, filename?: string): ScreenshotSentinelDefinition {
    return {
      type: 'screenshot',
      name: name || `screenshot-${Date.now()}`,
      version: '1.0',
      target: target || this._target || '',
      filename: filename || this._filename,
      outputDir: this.config.outputDir,
      viewport: this.config.viewport,
      createdAt: new Date().toISOString(),
    };
  }

  /**
   * Run from definition
   */
  async runFromDefinition(): Promise<ScreenshotResult> {
    if (!this._target) {
      return { success: false, error: 'No target specified in definition' };
    }
    if (this._target.startsWith('http://') || this._target.startsWith('https://')) {
      return this.screenshotUrl(this._target, this._filename);
    } else {
      return this.screenshotFile(this._target, this._filename);
    }
  }

  /**
   * Take a screenshot of a local HTML file
   */
  async screenshotFile(htmlPath: string, filename: string = 'screenshot.png'): Promise<ScreenshotResult> {
    // Check Puppeteer availability
    const check = await checkPuppeteer();
    if (!check.available) {
      return { success: false, error: check.reason };
    }

    // Resolve to absolute path
    const absolutePath = path.resolve(htmlPath);
    if (!fs.existsSync(absolutePath)) {
      return { success: false, error: `File not found: ${absolutePath}` };
    }

    const fileUrl = `file://${absolutePath}`;
    return this.screenshotUrl(fileUrl, filename);
  }

  /**
   * Take a screenshot of a URL
   */
  async screenshotUrl(url: string, filename: string = 'screenshot.png'): Promise<ScreenshotResult> {
    // Check Puppeteer availability
    const check = await checkPuppeteer();
    if (!check.available) {
      return { success: false, error: check.reason };
    }

    try {
      const context = await launchAndNavigate(url);

      // Set viewport
      await context.page.setViewport(this.config.viewport);

      // Ensure output directory exists
      if (!fs.existsSync(this.config.outputDir)) {
        fs.mkdirSync(this.config.outputDir, { recursive: true });
      }

      // Take screenshot
      const outputPath = path.join(this.config.outputDir, filename);
      await context.page.screenshot({ path: outputPath, fullPage: true });

      // Close browser
      await context.browser.close();

      return { success: true, imagePath: outputPath };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Serve a directory and take a screenshot (for multi-file apps)
   */
  async screenshotWithServer(
    directory: string,
    filename: string = 'screenshot.png',
    port: number = 9876
  ): Promise<ScreenshotResult> {
    const http = await import('http');
    const fsPromises = await import('fs/promises');

    // Simple static file server
    const server = http.createServer(async (req, res) => {
      const reqPath = req.url === '/' ? '/index.html' : req.url!;
      const filePath = path.join(directory, reqPath);

      try {
        const content = await fsPromises.readFile(filePath);
        const ext = path.extname(filePath);
        const mimeTypes: Record<string, string> = {
          '.html': 'text/html',
          '.css': 'text/css',
          '.js': 'application/javascript',
          '.json': 'application/json',
          '.png': 'image/png',
          '.jpg': 'image/jpeg',
        };
        res.setHeader('Content-Type', mimeTypes[ext] || 'text/plain');
        res.end(content);
      } catch {
        res.statusCode = 404;
        res.end('Not found');
      }
    });

    return new Promise((resolve) => {
      server.listen(port, async () => {
        const result = await this.screenshotUrl(`http://localhost:${port}`, filename);
        server.close();
        resolve(result);
      });
    });
  }
}

// Test function
export async function testVisualSentinel() {
  const sentinel = new VisualSentinel({
    outputDir: '/tmp/sentinel-screenshots',
  });

  console.log('Testing VisualSentinel...');

  // Test with a file URL
  const result = await sentinel.screenshotFile(
    '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/system/sentinel/olympics/snake/index.html',
    'snake-test.png'
  );

  console.log('Result:', result);
  return result;
}
