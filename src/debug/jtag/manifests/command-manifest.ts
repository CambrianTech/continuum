/**
 * Auto-generated Command Manifest
 * Generated at: 2025-07-23T22:38:42.905Z
 * 
 * This file maps command names to their import paths for both browser and server environments.
 * Used for auto-discovery and dynamic loading of commands.
 */

export interface CommandManifestEntry {
  className: string;
  importPath: string;
}

export interface CommandManifest {
  browser: Record<string, CommandManifestEntry>;
  server: Record<string, CommandManifestEntry>;
}

export const COMMAND_MANIFEST: CommandManifest = {
  "browser": {
    "screenshot": {
      "className": "ScreenshotBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/screenshot/browser/ScreenshotBrowserCommand"
    }
  },
  "server": {
    "screenshot": {
      "className": "ScreenshotServerCommand",
      "importPath": "../daemons/command-daemon/commands/screenshot/server/ScreenshotServerCommand"
    }
  }
};

/**
 * Get command manifest for specific environment
 */
export function getCommandManifest(environment: 'browser' | 'server'): Record<string, CommandManifestEntry> {
  return COMMAND_MANIFEST[environment];
}

/**
 * Get all command names for environment
 */
export function getCommandNames(environment: 'browser' | 'server'): string[] {
  return Object.keys(COMMAND_MANIFEST[environment]);
}
