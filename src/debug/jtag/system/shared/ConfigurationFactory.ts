/**
 * Configuration Factory - Dependency Injection for JTAG Configuration
 * 
 * Provides centralized configuration creation and injection for eliminating hardcoded values.
 * All configuration comes from package.json discovery, gets injected into constructors.
 */

import type { 
  JTAGConfig,
  InstanceConfiguration
} from './SecureConfigTypes';

export interface ConfigurationContext {
  readonly instance: InstanceConfiguration;
  readonly websocketPort: number;
  readonly httpPort: number;
  readonly baseDirectory: string;
}

export interface PortConfiguration {
  readonly websocket_server: number;
  readonly http_server: number;
}

/**
 * Configuration Factory - Creates properly configured contexts for dependency injection
 */
export class ConfigurationFactory {
  private static instance: ConfigurationFactory | null = null;
  private configurationContext: ConfigurationContext | null = null;

  private constructor() {}

  public static getInstance(): ConfigurationFactory {
    if (!ConfigurationFactory.instance) {
      ConfigurationFactory.instance = new ConfigurationFactory();
    }
    return ConfigurationFactory.instance;
  }

  /**
   * Initialize configuration from package.json discovery (server only)
   */
  public initializeFromPackageJson(): ConfigurationContext {
    if (this.configurationContext) {
      return this.configurationContext;
    }

    // Only initialize in server environment
    if (typeof require !== 'undefined' && typeof process !== 'undefined') {
      try {
        const { getActiveExample } = eval('require')('./ExampleConfig');
        const example = getActiveExample();
        
        this.configurationContext = {
          instance: {
            name: example.name,
            description: example.description,
            ports: example.ports,
            paths: example.paths,
            capabilities: {}
          },
          websocketPort: example.ports.websocket_server,
          httpPort: example.ports.http_server,
          baseDirectory: example.paths.directory
        };

        return this.configurationContext;
      } catch (error) {
        throw new Error(`ConfigurationFactory: Failed to initialize from package.json: ${(error as Error).message}`);
      }
    }

    throw new Error('ConfigurationFactory: Server environment required for package.json initialization');
  }

  /**
   * Initialize configuration for browser environment
   */
  public initializeForBrowser(ports: PortConfiguration, baseDirectory: string): ConfigurationContext {
    if (this.configurationContext) {
      return this.configurationContext;
    }

    const websocketPort = ports.websocket_server;
    const httpPort = ports.http_server;
    
    const isWidgetUI = httpPort === 9003;
    
    this.configurationContext = {
      instance: {
        name: isWidgetUI ? 'JTAG Widget Development UI' : 'JTAG Test Bench',
        description: isWidgetUI ? 'Focused widget development environment' : 'Full-featured testing environment',
        ports: {
          http_server: httpPort,
          websocket_server: websocketPort
        },
        paths: { 
          directory: isWidgetUI ? 'examples/widget-ui' : 'examples/test-bench',
          html_file: isWidgetUI ? 'index.html' : 'public/demo.html',
          build_output: 'dist'
        },
        capabilities: {}
      },
      websocketPort,
      httpPort,
      baseDirectory
    };

    return this.configurationContext;
  }

  /**
   * Get current configuration context (must be initialized first)
   */
  public getContext(): ConfigurationContext {
    if (!this.configurationContext) {
      throw new Error('ConfigurationFactory: Context not initialized. Call initializeFromPackageJson() or initializeForBrowser() first.');
    }
    return this.configurationContext;
  }

  /**
   * Get port configuration for dependency injection
   */
  public getPortConfiguration(): PortConfiguration {
    const context = this.getContext();
    return {
      websocket_server: context.websocketPort,
      http_server: context.httpPort
    };
  }

  /**
   * Create a pre-configured JTAGConfig for dependency injection
   */
  public createJTAGConfig(): JTAGConfig {
    const context = this.getContext();
    
    return {
      instance: context.instance,
      server: {
        server: {
          port: context.websocketPort,
          host: 'localhost',
          protocol: 'ws',
          bind_interface: '127.0.0.1',
          max_connections: 100,
          enable_cors: false
        },
        paths: {
          logs: `.continuum/jtag/logs`,
          screenshots: `.continuum/jtag/screenshots`,
          data_directory: `.continuum/jtag/data`,
          pid_file: `.continuum/jtag/server.pid`
        },
        security: {
          enable_authentication: false,
          session_timeout_ms: 3600000,
          rate_limiting: {
            enabled: false,
            requests_per_minute: 60
          }
        },
        environment: {
          log_level: 'info',
          debug_mode: false
        }
      },
      client: {
        client: {
          ui_port: context.httpPort,
          host: 'localhost',
          protocol: 'http',
          auto_connect: true,
          reconnect_attempts: 3
        },
        browser: {
          headless: false,
          devtools: true,
          width: 1200,
          height: 800,
          user_agent: 'JTAG-TestBrowser/1.0'
        },
        ui: {
          theme: 'dark',
          enable_animations: true,
          show_debug_panel: false
        }
      },
      test: {
        server: {
          port: context.websocketPort,
          host: 'localhost',
          protocol: 'ws'
        },
        client: {
          ui_port: context.httpPort,
          host: 'localhost',
          protocol: 'http'
        },
        test_settings: {
          timeout_ms: 30000,
          retry_attempts: 3,
          screenshot_on_failure: true,
          cleanup_after_test: true
        },
        environment: {
          test_mode: true,
          verbose_logging: true,
          isolated_sessions: true
        }
      }
    };
  }

  /**
   * Reset factory state (for testing)
   */
  public static reset(): void {
    if (ConfigurationFactory.instance) {
      ConfigurationFactory.instance.configurationContext = null;
    }
    ConfigurationFactory.instance = null;
  }
}

/**
 * Convenience function for getting configured context
 */
export function getConfiguredContext(): ConfigurationContext {
  return ConfigurationFactory.getInstance().getContext();
}

/**
 * Convenience function for getting port configuration
 */
export function getConfiguredPorts(): PortConfiguration {
  return ConfigurationFactory.getInstance().getPortConfiguration();
}