/**
 * Connection Configuration Factory - Shared App Code
 * 
 * This is "shared app code" that examples can use to create their connection config.
 * Examples import this from '@continuum/jtag/example-shared' and use it.
 * 
 * This handles all the config reading (package.json, env) so examples don't have to.
 */

import type { ConnectionConfig } from '@continuum/jtag/types';
import { validateConnectionConfig } from '@continuum/jtag/types';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Create connection configuration by reading from package.json and environment
 * 
 * This function does ALL the config reading so examples don't have to.
 * Examples just call this function and get a clean ConnectionConfig struct.
 */
export function createConnectionConfig(exampleDir?: string): ConnectionConfig {
  // Determine the example directory
  const workingDir = exampleDir || process.cwd();
  const packageJsonPath = path.join(workingDir, 'package.json');
  
  // Read the example's package.json
  if (!fs.existsSync(packageJsonPath)) {
    throw new Error(`ConnectionConfigFactory: No package.json found at ${packageJsonPath}. Each example must have a package.json with config.port setting.`);
  }
  
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
  const httpPort = packageJson.config?.port;
  
  if (!httpPort) {
    throw new Error(`ConnectionConfigFactory: No port configuration found in ${packageJsonPath}. Please add "config": { "port": <port_number> } to package.json`);
  }
  
  // Calculate WebSocket port (HTTP port - 1)
  const websocketPort = httpPort - 1;
  
  // Determine example name from directory or package name
  const exampleName = path.basename(workingDir) || 
                     packageJson.name?.split('/').pop() || 
                     'unknown-example';
  
  // Create the configuration struct
  const config: ConnectionConfig = {
    websocketPort,
    httpPort,
    workingDir,
    exampleName,
    // Future: multicastPort, unicastPort as needed
  };
  
  // Validate the configuration
  const validation = validateConnectionConfig(config);
  if (!validation.valid) {
    throw new Error(`ConnectionConfigFactory: Invalid configuration: ${validation.errors.join(', ')}`);
  }
  
  return config;
}

/**
 * Create connection configuration from environment variables
 * 
 * Alternative method for when config is provided via environment.
 * Still does all the reading so examples don't have to.
 */
export function createConnectionConfigFromEnv(): ConnectionConfig {
  const websocketPort = parseInt(process.env.JTAG_WEBSOCKET_PORT || '');
  const httpPort = parseInt(process.env.JTAG_HTTP_PORT || '');
  const workingDir = process.env.JTAG_WORKING_DIR || process.cwd();
  const exampleName = process.env.JTAG_EXAMPLE_NAME || 'env-example';
  
  if (!websocketPort || !httpPort) {
    throw new Error('ConnectionConfigFactory: JTAG_WEBSOCKET_PORT and JTAG_HTTP_PORT environment variables required');
  }
  
  const config: ConnectionConfig = {
    websocketPort,
    httpPort,
    workingDir,
    exampleName
  };
  
  const validation = validateConnectionConfig(config);
  if (!validation.valid) {
    throw new Error(`ConnectionConfigFactory: Invalid environment configuration: ${validation.errors.join(', ')}`);
  }
  
  return config;
}

/**
 * Auto-detect configuration method and create config
 * 
 * Tries package.json first, falls back to environment variables.
 * Examples can just call this for automatic configuration.
 */
export function createConnectionConfigAuto(exampleDir?: string): ConnectionConfig {
  try {
    return createConnectionConfig(exampleDir);
  } catch (packageError) {
    try {
      return createConnectionConfigFromEnv();
    } catch (envError) {
      const packageMsg = packageError instanceof Error ? packageError.message : String(packageError);
      const envMsg = envError instanceof Error ? envError.message : String(envError);
      throw new Error(
        `ConnectionConfigFactory: Could not create configuration.\n` +
        `Package.json error: ${packageMsg}\n` +
        `Environment error: ${envMsg}`
      );
    }
  }
}