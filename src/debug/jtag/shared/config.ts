/**
 * Emergency JTAG Configuration
 * 
 * Loads configuration from package.json and provides it as shared config.
 * Available everywhere as jtag.config
 */

export interface JTAGConfigData {
  jtagPort: number;
  enableRemoteLogging: boolean;
  enableConsoleOutput: boolean;
  maxBufferSize: number;
  rootPath: string;
  logDirectory: string;
  screenshotDirectory: string;
}

/**
 * Load JTAG configuration from package.json
 */
function loadJTAGConfig(): JTAGConfigData {
  try {
    // Find repo root using git command
    const findRepoRoot = () => {
      if (typeof require !== 'undefined') {
        try {
          const { execSync } = require('child_process');
          const repoRoot = execSync('git rev-parse --show-toplevel', { 
            encoding: 'utf8',
            cwd: __dirname
          }).trim();
          return repoRoot;
        } catch (error) {
          // Fallback: use relative path from our location if git fails
          const path = require('path');
          return path.resolve(__dirname, '../../..');
        }
      }
      return '';
    };
    
    const repoRoot = findRepoRoot();
    
    // In server context, load package.json from JTAG module
    if (typeof require !== 'undefined') {
      try {
        const packageJson = require('../package.json');
        const config = packageJson.config || {};
        
        // Use the rootPath from package.json config, default to ".continuum/jtag"
        const baseDir = config.rootPath || ".continuum/jtag";
        const fullBasePath = `${repoRoot}/${baseDir}`;
        
        return {
          jtagPort: config.port || 9001,
          enableRemoteLogging: true,
          enableConsoleOutput: true,
          maxBufferSize: 1000,
          rootPath: fullBasePath,
          logDirectory: `${fullBasePath}/logs`,
          screenshotDirectory: `${fullBasePath}/screenshots`
        };
      } catch (packageError) {
        // If package.json fails, use defaults with proper repo root
        const fullBasePath = `${repoRoot}/.continuum/jtag`;
        return {
          jtagPort: 9001,
          enableRemoteLogging: true,
          enableConsoleOutput: true,
          maxBufferSize: 1000,
          rootPath: fullBasePath,
          logDirectory: `${fullBasePath}/logs`,
          screenshotDirectory: `${fullBasePath}/screenshots`
        };
      }
    }
    
    // Browser context - return defaults with repo root
    const fullBasePath = `${repoRoot}/.continuum/jtag`;
    return {
      jtagPort: 9001,
      enableRemoteLogging: true,
      enableConsoleOutput: true,
      maxBufferSize: 1000,
      rootPath: fullBasePath,
      logDirectory: `${fullBasePath}/logs`,
      screenshotDirectory: `${fullBasePath}/screenshots`
    };
  } catch (error) {
    // Fallback to defaults if everything fails
    return {
      jtagPort: 9001,
      enableRemoteLogging: true,
      enableConsoleOutput: true,
      maxBufferSize: 1000,
      rootPath: '.continuum/jtag',
      logDirectory: '.continuum/jtag/logs',
      screenshotDirectory: '.continuum/jtag/screenshots'
    };
  }
}

/**
 * Shared JTAG configuration - loaded once, available everywhere
 */
export const jtagConfig = loadJTAGConfig();