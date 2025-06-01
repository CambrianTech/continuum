/**
 * Main DevBridge class that orchestrates browser testing and CLI execution
 */

import path from 'path';
import fs from 'fs/promises';
import { BrowserTester } from './browser-tester';
import { CLIRunner } from './cli-runner';
import { DevBridgeOptions, BrowserTestResult, CLITestResult } from './types';

export class DevBridge {
  private browserTester: BrowserTester;
  private cliRunner: CLIRunner;
  private options: DevBridgeOptions;

  constructor(options: DevBridgeOptions = {}) {
    this.options = {
      headless: true,
      timeout: 30000,
      screenshotDir: './screenshots',
      verboseLogging: false,
      ...options
    };
    
    this.browserTester = new BrowserTester();
    this.cliRunner = new CLIRunner();
  }

  async init(): Promise<void> {
    await this.browserTester.launch(this.options.headless);
    
    // Ensure screenshot directory exists
    if (this.options.screenshotDir) {
      await fs.mkdir(this.options.screenshotDir, { recursive: true });
    }
  }

  async testWebApp(url: string, options: {
    screenshot?: boolean;
    waitTime?: number;
    screenshotPath?: string;
  } = {}): Promise<BrowserTestResult> {
    
    const result = await this.browserTester.test(url, {
      screenshot: options.screenshot,
      waitTime: options.waitTime || 3000,
    });

    // Save screenshot if requested
    if (options.screenshot && result.screenshot && options.screenshotPath) {
      const buffer = Buffer.from(result.screenshot, 'base64');
      await fs.writeFile(options.screenshotPath, buffer);
    }

    if (this.options.verboseLogging) {
      console.log(`🌐 Tested ${url}`);
      console.log(`📊 Console messages: ${result.console.length}`);
      console.log(`❌ Errors: ${result.errors.length}`);
      if (result.errors.length > 0) {
        result.errors.forEach(error => console.log(`   ${error}`));
      }
    }

    return result;
  }

  async compareImplementations(config: {
    pythonCLI: {
      script: string;
      args?: string[];
      cwd?: string;
    };
    webDemo: {
      url: string;
      waitTime?: number;
    };
    screenshotBaseName?: string;
  }): Promise<{
    python: CLITestResult;
    web: BrowserTestResult;
    screenshots?: {
      web: string;
    };
  }> {
    
    // Test Python CLI
    const pythonResult = await this.cliRunner.testPythonCLI(
      config.pythonCLI.script,
      config.pythonCLI.args,
      { cwd: config.pythonCLI.cwd, timeout: this.options.timeout }
    );

    // Test web demo
    const webResult = await this.testWebApp(config.webDemo.url, {
      screenshot: true,
      waitTime: config.webDemo.waitTime
    });

    // Save screenshots with descriptive names
    let screenshots;
    if (webResult.screenshot && this.options.screenshotDir) {
      const baseName = config.screenshotBaseName || 'comparison';
      const webPath = path.join(this.options.screenshotDir, `${baseName}-web.png`);
      
      const buffer = Buffer.from(webResult.screenshot, 'base64');
      await fs.writeFile(webPath, buffer);
      
      screenshots = { web: webPath };
    }

    return {
      python: pythonResult,
      web: webResult,
      screenshots
    };
  }

  async debugWebApp(url: string): Promise<{
    consoleErrors: string[];
    jsErrors: string[];
    performanceIssues: string[];
    screenshot: string;
  }> {
    const result = await this.testWebApp(url, { 
      screenshot: true, 
      waitTime: 5000 
    });
    
    const consoleErrors = result.console
      .filter(msg => msg.type === 'error')
      .map(msg => msg.text);
    
    const jsErrors = result.errors;
    
    const performanceIssues: string[] = [];
    if (result.performance?.loadTime && result.performance.loadTime > 3000) {
      performanceIssues.push(`Slow load time: ${result.performance.loadTime}ms`);
    }

    // Save debug screenshot
    const debugPath = path.join(
      this.options.screenshotDir || './screenshots',
      `debug-${Date.now()}.png`
    );
    
    if (result.screenshot) {
      const buffer = Buffer.from(result.screenshot, 'base64');
      await fs.writeFile(debugPath, buffer);
    }

    return {
      consoleErrors,
      jsErrors,
      performanceIssues,
      screenshot: debugPath
    };
  }

  async close(): Promise<void> {
    await this.browserTester.close();
  }
}