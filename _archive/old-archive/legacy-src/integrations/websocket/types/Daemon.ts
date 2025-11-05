/**
 * Daemon Types - TypeScript daemon integration interfaces
 */

export interface DaemonConnection {
  readonly connected: boolean;
  readonly commandProcessor?: CommandProcessor;
  readonly eventProcessor?: EventProcessor;
  readonly lastConnectAttempt?: Date;
  readonly connectionAttempts: number;
}

export interface CommandProcessor {
  readonly initialized: boolean;
  executeCommand(command: string, params: any, context: any): Promise<CommandResult>;
  getCommands?(): string[];
  getDefinition?(command: string): CommandDefinition;
}

export interface EventProcessor {
  readonly initialized: boolean;
  processEvent(event: string, payload: any, context: any): Promise<EventResult>;
  subscribe?(events: string[]): void;
  unsubscribe?(events: string[]): void;
}

export interface CommandDefinition {
  readonly name: string;
  readonly description: string;
  readonly category: string;
  readonly parameters?: Record<string, ParameterDefinition>;
  readonly examples?: string[];
}

export interface ParameterDefinition {
  readonly type: string;
  readonly description?: string;
  readonly required?: boolean;
  readonly default?: any;
}

export interface CommandResult {
  readonly success: boolean;
  readonly data?: any;
  readonly error?: string;
  readonly duration?: number;
  readonly processor?: string;
}

export interface EventResult {
  readonly handled: boolean;
  readonly response?: any;
  readonly error?: string;
}