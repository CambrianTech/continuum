/**
 * Auto-generated Daemon Manifest
 * Generated at: 2025-07-23T16:41:02.779Z
 * 
 * This file maps daemon names to their import paths for both browser and server environments.
 * Used for auto-discovery and dynamic loading of daemons.
 */

export interface DaemonManifestEntry {
  className: string;
  importPath: string;
}

export interface DaemonManifest {
  browser: Record<string, DaemonManifestEntry>;
  server: Record<string, DaemonManifestEntry>;
}

export const DAEMON_MANIFEST: DaemonManifest = {
  "browser": {
    "CommandDaemon": {
      "className": "CommandDaemonBrowser",
      "importPath": "../daemons/command-daemon/browser/CommandDaemonBrowser"
    },
    "ConsoleDaemon": {
      "className": "ConsoleDaemonBrowser",
      "importPath": "../daemons/console-daemon/browser/ConsoleDaemonBrowser"
    }
  },
  "server": {
    "CommandDaemon": {
      "className": "CommandDaemonServer",
      "importPath": "../daemons/command-daemon/server/CommandDaemonServer"
    },
    "ConsoleDaemon": {
      "className": "ConsoleDaemonServer",
      "importPath": "../daemons/console-daemon/server/ConsoleDaemonServer"
    }
  }
};

/**
 * Get daemon manifest for specific environment
 */
export function getDaemonManifest(environment: 'browser' | 'server'): Record<string, DaemonManifestEntry> {
  return DAEMON_MANIFEST[environment];
}

/**
 * Get all daemon names for environment
 */
export function getDaemonNames(environment: 'browser' | 'server'): string[] {
  return Object.keys(DAEMON_MANIFEST[environment]);
}
