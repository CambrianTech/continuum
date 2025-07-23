/**
 * JTAG Module Base Class
 * 
 * Base class that ALL JTAG submodules inherit from:
 * - JTAGSystem
 * - JTAGBaseDaemon and subclasses  
 * - All command implementations
 */

import type { JTAGContext } from './JTAGTypes';

export abstract class JTAGModule {
  readonly name: string;
  readonly context: JTAGContext;

  constructor(name: string, context: JTAGContext) {
    this.name = name;
    this.context = context;
  }

  get modulePath(): string {
    return `${this.context.environment}/${this.name}`;
  }

  get contextUUID(): string {
    return this.context.uuid;
  }

  get environment(): string {
    return this.context.environment;
  }

  isSameContext(other: JTAGModule): boolean {
    return this.context.uuid === other.context.uuid;
  }

  toString(): string {
    return `${this.constructor.name}[${this.name}@${this.context.environment}:${this.context.uuid.substring(0, 8)}]`;
  }
}