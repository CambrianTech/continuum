/**
 * Secure JTAG Context Factory - Security-First Context Creation
 * 
 * SECURITY ARCHITECTURE:
 * - Each environment gets only the configuration it needs
 * - Server contexts never leak to client contexts
 * - Type-safe configuration access prevents misuse
 * - Factory pattern ensures proper initialization
 */

import { generateUUID, type UUID } from '../types/CrossPlatformUUID';
import type { JTAGContext, JTAGEnvironment, JTAGContextConfig } from '../types/JTAGTypes';
import { 
  getDefaultServerConfig, 
  getDefaultClientConfig, 
  getDefaultTestConfig, 
  isTestEnvironment 
} from '../../shared/BrowserSafeConfig';

/**
 * Create a server-side JTAG context with full server configuration access
 * SERVER ONLY - contains sensitive configuration data
 */
export function createServerContext(uuid?: UUID): JTAGContext {
  return {
    uuid: uuid || generateUUID(),
    environment: 'server',
    getConfig(): JTAGContextConfig {
      const config = isTestEnvironment() ? 
        getDefaultTestConfig() : 
        getDefaultServerConfig();
      
      return {
        type: isTestEnvironment() ? 'test' : 'server',
        config: config as any
      };
    }
  };
}

/**
 * Create a client-side JTAG context with client-safe configuration only
 * CLIENT SAFE - no sensitive server data exposed
 */
export function createClientContext(uuid?: UUID): JTAGContext {
  return {
    uuid: uuid || generateUUID(),
    environment: 'browser',
    getConfig(): JTAGContextConfig {
      return {
        type: 'client',
        config: getDefaultClientConfig()
      };
    }
  };
}

/**
 * Create a test JTAG context with test-specific configuration
 * TEST ONLY - isolated test configuration
 */
export function createTestContext(uuid?: UUID): JTAGContext {
  return {
    uuid: uuid || generateUUID(),
    environment: 'server', // Tests run in server environment but with test config
    getConfig(): JTAGContextConfig {
      return {
        type: 'test',
        config: getDefaultTestConfig()
      };
    }
  };
}

/**
 * Utility functions for type-safe configuration access
 */
export function getServerConfigFromContext(context: JTAGContext) {
  const config = context.getConfig();
  if (config.type !== 'server' && config.type !== 'test') {
    throw new Error(`Server configuration not available in ${config.type} context`);
  }
  return config.config;
}

export function getClientConfigFromContext(context: JTAGContext) {
  const config = context.getConfig();
  if (config.type !== 'client') {
    throw new Error(`Client configuration not available in ${config.type} context`);
  }
  return config.config;
}

export function getTestConfigFromContext(context: JTAGContext) {
  const config = context.getConfig();
  if (config.type !== 'test') {
    throw new Error(`Test configuration not available in ${config.type} context`);
  }
  return config.config;
}