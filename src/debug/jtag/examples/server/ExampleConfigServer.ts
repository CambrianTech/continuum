/**
 * Example Configuration Server - Node.js implementation
 *
 * SERVER-ONLY: Uses file system, ServerConfig, and Node.js APIs.
 * Browser code should NOT import this file.
 */

import * as fs from 'fs';
import * as path from 'path';
import { getServerConfig } from '../../system/config/ServerConfig';
import type { ExampleConfig, ExampleDefinition, ExamplePorts } from '../shared/ExampleConfigTypes';

/**
 * Singleton configuration loaded from package.json + config.env
 */
class ExampleConfigServerImpl {
  private static instance: ExampleConfigServerImpl | null = null;
  private config: ExampleConfig | null = null;

  private constructor() {}

  static getInstance(): ExampleConfigServerImpl {
    if (!ExampleConfigServerImpl.instance) {
      ExampleConfigServerImpl.instance = new ExampleConfigServerImpl();
    }
    return ExampleConfigServerImpl.instance;
  }

  /**
   * Load configuration from package.json discovery pattern
   */
  loadConfig(): ExampleConfig {
    if (this.config) {
      return this.config;
    }

    try {
      // 1. Read active_example from main package.json
      const mainPackageJsonPath = path.join(__dirname, '../../package.json');
      const mainPackageJson = JSON.parse(fs.readFileSync(mainPackageJsonPath, 'utf-8'));
      const activeExample = mainPackageJson.config?.active_example || 'test-bench';

      // 2. Read ports from config.env via ServerConfig (sole accessor)
      const serverConfig = getServerConfig();
      const httpPort = serverConfig.getHttpPort();
      const websocketPort = serverConfig.getWsPort();

      // 3. Read example-specific metadata from package.json
      const exampleDir = `examples/${activeExample}`;
      const examplePackageJsonPath = path.join(__dirname, '../../', exampleDir, 'package.json');
      const examplePackageJson = JSON.parse(fs.readFileSync(examplePackageJsonPath, 'utf-8'));

      // 4. Determine HTML file based on example
      const htmlFile = activeExample === 'widget-ui' ? 'index.html' : 'public/demo.html';

      // 5. Build configuration
      this.config = {
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
            features: {}
          }
        }
      };

      if (process.env.JTAG_VERBOSE === 'true') {
        console.log(`📋 ExampleConfigServer: Loaded - active: ${this.config.active_example}`);
      }

      return this.config;
    } catch (error) {
      throw new Error(`ExampleConfigServer: Failed to load config: ${(error as Error).message}`);
    }
  }

  /**
   * Get active example name
   */
  getActiveExampleName(): string {
    return this.loadConfig().active_example;
  }

  /**
   * Get active example definition
   */
  getActiveExample(): ExampleDefinition {
    const config = this.loadConfig();
    const activeExample = config.examples[config.active_example];
    if (!activeExample) {
      throw new Error(`Unknown example: ${config.active_example}`);
    }
    return activeExample;
  }

  /**
   * Get active example ports
   */
  getActivePorts(): ExamplePorts {
    const example = this.getActiveExample();
    if (process.env.JTAG_VERBOSE === 'true') {
      console.log(`📋 getActivePorts:`, JSON.stringify(example.ports, null, 2));
    }
    return example.ports;
  }

  /**
   * Get active example ports (sync version)
   */
  getActivePortsSync(): ExamplePorts {
    return this.getActivePorts();
  }

  /**
   * Get active example absolute path
   */
  getActiveExamplePath(): string {
    const example = this.getActiveExample();
    return path.join(__dirname, '../../', example.paths.directory);
  }

  /**
   * Reset singleton (for testing)
   */
  static reset(): void {
    if (ExampleConfigServerImpl.instance) {
      ExampleConfigServerImpl.instance.config = null;
    }
    ExampleConfigServerImpl.instance = null;
  }
}

// Export singleton instance and convenience functions
const instance = ExampleConfigServerImpl.getInstance();

export const getActiveExampleName = () => instance.getActiveExampleName();
export const getActiveExample = () => instance.getActiveExample();
export const getActivePorts = () => instance.getActivePorts();
export const getActivePortsSync = () => instance.getActivePortsSync();
export const getActiveExamplePath = () => instance.getActiveExamplePath();
export const loadConfig = () => instance.loadConfig();
