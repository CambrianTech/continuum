/**
 * CommandNaming - Command-specific naming conventions
 *
 * Applies command-specific patterns on top of generic TokenBuilder utilities.
 * Handles path -> class name mappings for commands, params, results, etc.
 */

import { TokenBuilder } from './TokenBuilder';

export interface CommandSpec {
  name: string;           // e.g., "docs/read" or "screenshot" or "echo"
  description: string;    // Human-readable description
  params: ParamSpec[];    // Parameter definitions
  results: ResultSpec[];  // Result field definitions
  examples?: ExampleSpec[];
  accessLevel?: 'ai-safe' | 'internal' | 'system' | 'dangerous';
  implementation?: 'server' | 'browser' | 'both';  // Defaults to 'server' (DEPRECATED: use environment)
  environment?: 'server' | 'browser' | 'both';     // Execution environment (determines test strategy)
}

export interface ParamSpec {
  name: string;
  type: string;
  optional?: boolean;
  description?: string;
}

export interface ResultSpec {
  name: string;
  type: string;
  description?: string;
}

export interface ExampleSpec {
  description: string;
  command: string;
  expectedResult?: string;
}

/**
 * Command-specific naming helper
 * Converts command paths (e.g. "docs/read", "echo") into proper class names, types, and paths
 */
export class CommandNaming {
  constructor(private spec: CommandSpec) {}

  /**
   * Base name in PascalCase
   * @example "echo" -> "Echo"
   * @example "docs/read" -> "DocsRead"
   */
  get baseName(): string {
    return TokenBuilder.toClassName(this.spec.name);
  }

  /**
   * Params type name
   * @example "echo" -> "EchoParams"
   */
  get paramsType(): string {
    return `${this.baseName}Params`;
  }

  /**
   * Result type name
   * @example "echo" -> "EchoResult"
   */
  get resultType(): string {
    return `${this.baseName}Result`;
  }

  /**
   * Server class name
   * @example "echo" -> "EchoServerCommand"
   */
  get serverClass(): string {
    return `${this.baseName}ServerCommand`;
  }

  /**
   * Browser class name
   * @example "echo" -> "EchoBrowserCommand"
   */
  get browserClass(): string {
    return `${this.baseName}BrowserCommand`;
  }

  /**
   * Command directory path
   * @example "echo" -> "commands/echo"
   * @example "docs/read" -> "commands/docs/read"
   */
  get directoryPath(): string {
    return `commands/${this.spec.name}`;
  }

  /**
   * Types file path
   * @example "echo" -> "commands/echo/shared/EchoTypes.ts"
   */
  get typesFilePath(): string {
    return `${this.directoryPath}/shared/${this.baseName}Types.ts`;
  }

  /**
   * Server file path
   * @example "echo" -> "commands/echo/server/EchoServerCommand.ts"
   */
  get serverFilePath(): string {
    return `${this.directoryPath}/server/${this.serverClass}.ts`;
  }

  /**
   * Browser file path
   * @example "echo" -> "commands/echo/browser/EchoBrowserCommand.ts"
   */
  get browserFilePath(): string {
    return `${this.directoryPath}/browser/${this.browserClass}.ts`;
  }

  /**
   * Implementation mode (defaults to 'server')
   */
  get implementation(): 'server' | 'browser' | 'both' {
    return this.spec.implementation || 'server';
  }
}
