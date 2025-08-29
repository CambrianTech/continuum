/**
 * Centralized Example Configuration - Single source of truth
 * 
 * Browser-safe configuration management that reads config ONCE at startup.
 * NO environment variables, NO repeated file system access.
 */

interface ExampleConfig {
  readonly active_example: string;
  readonly examples: Record<string, {
    readonly name: string;
    readonly description: string;
    readonly ports: {
      readonly http_server: number;
      readonly websocket_server: number;
    };
    readonly paths: {
      readonly directory: string;
      readonly html_file: string;
      readonly build_output: string;
    };
    readonly features?: Record<string, boolean>; // Optional - all features always enabled
  }>;
}

// Global config loaded ONCE at module initialization
let config: ExampleConfig | null = null;

/**
 * Load configuration ONCE from package.json discovery pattern (server-only)
 * Browser gets config passed in via configureForBrowser()
 */
function loadConfig(): ExampleConfig {
  if (config) {
    return config;
  }

  // Browser environment - config must be provided externally
  if (typeof window !== 'undefined' || typeof document !== 'undefined') {
    throw new Error('ExampleConfig: Browser environment detected. Use configureForBrowser() to provide config.');
  }

  // Server environment - load from package.json discovery pattern
  if (typeof require !== 'undefined' && typeof process !== 'undefined') {
    try {
      const fs = eval('require')('fs');
      const path = eval('require')('path');
      
      // 1. Read active_example from main package.json
      const mainPackageJsonPath = path.join(__dirname, '../../package.json');
      const mainPackageJson = JSON.parse(fs.readFileSync(mainPackageJsonPath, 'utf-8'));
      const activeExample = mainPackageJson.config?.active_example || 'test-bench';
      
      // 2. Read example-specific config from its package.json
      const exampleDir = `examples/${activeExample}`;
      const examplePackageJsonPath = path.join(__dirname, '../../', exampleDir, 'package.json');
      const examplePackageJson = JSON.parse(fs.readFileSync(examplePackageJsonPath, 'utf-8'));
      const httpPort = examplePackageJson.config?.port;
      if (!httpPort) {
        throw new Error(`ExampleConfig: No port configuration found in ${examplePackageJsonPath}. Please add "config": { "port": <port_number> } to package.json`);
      }
      const websocketPort = httpPort - 1; // WebSocket port is HTTP port - 1
      
      // 3. Determine HTML file based on example
      const htmlFile = activeExample === 'widget-ui' ? 'index.html' : 'public/demo.html';
      
      // 4. Build configuration dynamically (no artificial features)
      config = {
        active_example: activeExample,
        examples: {
          [activeExample]: {
            name: examplePackageJson.name || `JTAG ${activeExample}`,
            description: examplePackageJson.description || `${activeExample} development environment`,
            ports: {
              http_server: httpPort,
              websocket_server: websocketPort
            },
            paths: {
              directory: exampleDir,
              html_file: htmlFile,
              build_output: 'dist'
            },
            features: {} // All features always enabled - no artificial limitations
          }
        }
      };
      console.log(`📋 ExampleConfig: Loaded via package.json discovery - active: ${config!.active_example}`);
      return config!;
    } catch (error) {
      throw new Error(`ExampleConfig: Failed to load config file: ${(error as Error).message}`);
    }
  }

  throw new Error('ExampleConfig: Unable to determine environment or load configuration');
}

/**
 * Configure for browser environment (called by SystemConfiguration)
 */
export function configureForBrowser(browserConfig: ExampleConfig): void {
  if (config) {
    console.warn('ExampleConfig: Already configured, ignoring duplicate configuration');
    return;
  }
  config = browserConfig;
  console.log(`📋 ExampleConfig: Configured for browser - active: ${config.active_example}`);
}

// Simple API - NO environment variables, direct JSON access

export function getActiveExampleName(): string {
  return loadConfig().active_example;
}

export async function getActivePorts(): Promise<Record<string, number>> {
  const activeExampleName = getActiveExampleName();
  const cfg = loadConfig();
  const activeExample = cfg.examples[activeExampleName];
  
  if (!activeExample) {
    throw new Error(`Example '${activeExampleName}' not found in configuration`);
  }
  
  console.log(`📋 getActivePorts: Using configured ports from examples.json:`, JSON.stringify(activeExample.ports, null, 2));
  return activeExample.ports;
}

export function getActiveExamplePath(): string {
  const activeExampleName = getActiveExampleName();
  const example = loadConfig().examples[activeExampleName];
  if (!example) {
    throw new Error(`Unknown example: ${activeExampleName}`);
  }
  
  // Return path for server environment only
  if (typeof require !== 'undefined' && typeof process !== 'undefined') {
    const path = eval('require')('path');
    return path.join(__dirname, '../../', example.paths.directory);
  }
  
  return example.paths.directory; // Browser just gets relative path
}

export function getActiveExample() {
  const activeExampleName = getActiveExampleName();
  const example = loadConfig().examples[activeExampleName];
  if (!example) {
    throw new Error(`Unknown example: ${activeExampleName}`);
  }
  return example;
}

// Sync version for backwards compatibility
export function getActivePortsSync(): Record<string, number> {
  const activeExampleName = getActiveExampleName();
  const cfg = loadConfig();
  const activeExample = cfg.examples[activeExampleName];
  
  if (!activeExample) {
    throw new Error(`Example '${activeExampleName}' not found in configuration`);
  }
  
  return activeExample.ports;
}