/**
 * Type definitions for dev-bridge
 */

export interface ScreenshotOptions {
  width?: number;
  height?: number;
  fullPage?: boolean;
  clip?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
}

export interface ConsoleMessage {
  type: 'log' | 'error' | 'warn' | 'info' | 'debug';
  text: string;
  timestamp: number;
}

export interface BrowserTestResult {
  url: string;
  screenshot?: string;
  console: ConsoleMessage[];
  errors: string[];
  performance?: {
    loadTime: number;
    domContentLoaded: number;
  };
}

export interface CLITestResult {
  command: string;
  exitCode: number;
  stdout: string;
  stderr: string;
  duration: number;
}

export interface DevBridgeOptions {
  headless?: boolean;
  timeout?: number;
  screenshotDir?: string;
  verboseLogging?: boolean;
}