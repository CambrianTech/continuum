/**
 * Auto-generated Command Manifest
 * Generated at: 2025-07-25T17:29:17.097Z
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
    "click": {
      "className": "ClickBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/click/browser/ClickBrowserCommand"
    },
    "compile-typescript": {
      "className": "CompileTypescriptBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/compile-typescript/browser/CompileTypescriptBrowserCommand"
    },
    "get-text": {
      "className": "GetTextBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/get-text/browser/GetTextBrowserCommand"
    },
    "navigate": {
      "className": "NavigateBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/navigate/browser/NavigateBrowserCommand"
    },
    "screenshot": {
      "className": "ScreenshotBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/screenshot/browser/ScreenshotBrowserCommand"
    },
    "scroll": {
      "className": "ScrollBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/scroll/browser/ScrollBrowserCommand"
    },
    "type": {
      "className": "TypeBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/type/browser/TypeBrowserCommand"
    },
    "wait-for-element": {
      "className": "WaitForElementBrowserCommand",
      "importPath": "../daemons/command-daemon/commands/wait-for-element/browser/WaitForElementBrowserCommand"
    }
  },
  "server": {
    "click": {
      "className": "ClickServerCommand",
      "importPath": "../daemons/command-daemon/commands/click/server/ClickServerCommand"
    },
    "compile-typescript": {
      "className": "CompileTypescriptServerCommand",
      "importPath": "../daemons/command-daemon/commands/compile-typescript/server/CompileTypescriptServerCommand"
    },
    "get-text": {
      "className": "GetTextServerCommand",
      "importPath": "../daemons/command-daemon/commands/get-text/server/GetTextServerCommand"
    },
    "navigate": {
      "className": "NavigateServerCommand",
      "importPath": "../daemons/command-daemon/commands/navigate/server/NavigateServerCommand"
    },
    "screenshot": {
      "className": "ScreenshotServerCommand",
      "importPath": "../daemons/command-daemon/commands/screenshot/server/ScreenshotServerCommand"
    },
    "scroll": {
      "className": "ScrollServerCommand",
      "importPath": "../daemons/command-daemon/commands/scroll/server/ScrollServerCommand"
    },
    "type": {
      "className": "TypeServerCommand",
      "importPath": "../daemons/command-daemon/commands/type/server/TypeServerCommand"
    },
    "wait-for-element": {
      "className": "WaitForElementServerCommand",
      "importPath": "../daemons/command-daemon/commands/wait-for-element/server/WaitForElementServerCommand"
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
