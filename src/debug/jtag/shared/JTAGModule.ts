// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAG Module Base Class
 * 
 * Universal base class providing context awareness and module identification.
 * Core architectural component that ALL JTAG submodules inherit from:
 * 
 * INHERITANCE HIERARCHY:
 * - JTAGSystem (environment orchestration)
 * - JTAGBaseDaemon and subclasses (service management) 
 * - Command implementations (operation execution)
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Context management and module identification
 * - Integration tests: Cross-module context sharing verification
 * - Architecture tests: Inheritance pattern compliance
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Provides consistent toString() format for debugging across all modules
 * - Context UUID enables cross-environment module correlation
 * - Module path format: {environment}/{name} for routing clarity
 */

import type { JTAGContext } from '@shared/JTAGTypes';

export abstract class JTAGModule {
  readonly name: string;
  readonly context: JTAGContext;

  constructor(name: string, context: JTAGContext) {
    this.name = name;
    this.context = context;
  }

  /**
   * Module routing path: {environment}/{name}
   * Used by router for cross-context message routing
   */
  get modulePath(): string {
    return `${this.context.environment}/${this.name}`;
  }

  get contextUUID(): string {
    return this.context.uuid;
  }

  get environment(): string {
    return this.context.environment;
  }

  /**
   * Context comparison for module correlation
   * Essential for cross-module communication validation
   */
  isSameContext(other: JTAGModule): boolean {
    return this.context.uuid === other.context.uuid;
  }

  /**
   * Standardized debugging format: ClassName[name@environment:uuid8]
   * Consistent across all JTAG modules for log correlation
   */
  toString(): string {
    return `${this.constructor.name}[${this.name}@${this.context.environment}:${this.context.uuid.substring(0, 8)}]`;
  }
}