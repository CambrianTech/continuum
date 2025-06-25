/**
 * Browser adapter interface - plugin architecture for different browsers
 * Supports Chrome, Firefox, Safari, Opera, Edge, etc.
 */

export interface IBrowserAdapter {
  readonly name: string;
  readonly executablePaths: string[];
  readonly supportsHeadless: boolean;
  readonly supportsRemoteDebugging: boolean;

  /**
   * Build command-line arguments for launching this browser
   */
  buildLaunchArgs(config: BrowserLaunchConfig): string[];

  /**
   * Get default executable path for this platform
   */
  getExecutablePath(): string | null;

  /**
   * Verify browser is available on system
   */
  isAvailable(): Promise<boolean>;

  /**
   * Get DevTools endpoint URL pattern
   */
  getDevToolsEndpoint(port: number): string;

  /**
   * Browser-specific ready detection
   */
  isReady(port: number): Promise<boolean>;
}

export interface BrowserLaunchConfig {
  port: number;
  userDataDir: string;
  initialUrl: string;
  windowTitle: string;
  headless?: boolean;
  visible?: boolean;
  minimized?: boolean;
  position?: { x: number; y: number };
  size?: { width: number; height: number };
  additionalArgs?: string[];
}

export enum BrowserType {
  CHROME = 'chrome',
  FIREFOX = 'firefox',
  SAFARI = 'safari',
  OPERA = 'opera',
  OPERA_GX = 'opera-gx',
  EDGE = 'edge'
}