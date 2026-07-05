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
  isTestEnvironment 
} from '../../shared/BrowserSafeConfig';
import type { 
  JTAGConfig, 
  JTAGServerConfiguration, 
  JTAGClientConfiguration, 
  JTAGTestConfiguration,
  InstanceConfiguration
} from '../../shared/SecureConfigTypes';
import { 
  validateServerConfig, 
  validateClientConfig, 
  validateJTAGConfig 
} from '../../shared/SecureConfigTypes';

/**
 * Create a server-side JTAG context with validated configuration
 */
export function createServerContext(config: JTAGConfig, uuid?: UUID): JTAGContext {
  // Validate config at runtime
  if (!validateJTAGConfig(config)) {
    throw new Error('Invalid JTAG configuration provided to createServerContext');
  }

  return {
    uuid: uuid || generateUUID(),
    environment: 'server',
    config,
    getConfig(): JTAGContextConfig {
      return {
        type: 'server',
        config: config.server
      };
    }
  };
}

/**
 * Create a client-side JTAG context with validated configuration
 */
export function createClientContext(config: JTAGConfig, uuid?: UUID): JTAGContext {
  // Validate config at runtime
  if (!validateJTAGConfig(config)) {
    throw new Error('Invalid JTAG configuration provided to createClientContext');
  }

  return {
    uuid: uuid || generateUUID(),
    environment: 'browser',
    config,
    getConfig(): JTAGContextConfig {
      return {
        type: 'client',
        config: config.client
      };
    }
  };
}

/**
 * Create a test JTAG context with validated configuration
 */
export function createTestContext(config: JTAGConfig, uuid?: UUID): JTAGContext {
  // Validate config at runtime
  if (!validateJTAGConfig(config)) {
    throw new Error('Invalid JTAG configuration provided to createTestContext');
  }

  return {
    uuid: uuid || generateUUID(),
    environment: 'server',
    config,
    getConfig(): JTAGContextConfig {
      return {
        type: 'test',
        config: config.test!
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