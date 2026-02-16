/**
 * Process Registry Command - Shared Base
 * 
 * Provides the common interface and validation logic for process registry operations.
 * Follows the modular command architecture pattern.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type {
  RegisterProcessParams,
  RegisterProcessResult,
  ListProcessesParams,
  ListProcessesResult,
  CleanupProcessesParams,
  CleanupProcessesResult,
  ProcessType,
  ProcessCapability,
  ProcessRegistryEntry
} from './ProcessRegistryTypes';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';

/**
 * Process Registry Command Base Class
 * 
 * Provides the common base for process management operations:
 * - registerProcess: Register current process in global registry
 * - listProcesses: Get active JTAG processes with filtering
 * - cleanupProcesses: Smart cleanup respecting process registry
 */
export abstract class ProcessRegistryCommand extends CommandBase<RegisterProcessParams | ListProcessesParams | CleanupProcessesParams, RegisterProcessResult | ListProcessesResult | CleanupProcessesResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('process-registry', context, subpath, commander);
  }

  /**
   * Register the current process in the global registry
   */
  abstract registerProcess(params: RegisterProcessParams): Promise<RegisterProcessResult>;
  
  /**
   * List active JTAG processes with optional filtering
   */
  abstract listProcesses(params: ListProcessesParams): Promise<ListProcessesResult>;
  
  /**
   * Perform intelligent cleanup respecting process registry
   */
  abstract cleanupProcesses(params: CleanupProcessesParams): Promise<CleanupProcessesResult>;

  /**
   * Default execute method - routes to appropriate operation based on method name in context
   */
  async execute(params: RegisterProcessParams | ListProcessesParams | CleanupProcessesParams): Promise<RegisterProcessResult | ListProcessesResult | CleanupProcessesResult> {
    // This is a multi-operation command, so we need to determine which operation to perform
    // For now, we'll implement a simple routing mechanism
    // In practice, this would be handled by the command daemon routing
    throw new Error('ProcessRegistryCommand.execute() should not be called directly - use specific methods (registerProcess, listProcesses, cleanupProcesses)');
  }

  /**
   * Generate default parameters for process registration
   */
  public getDefaultParams(sessionId: UUID, context: JTAGContext): RegisterProcessParams {
    return {
      userId: SYSTEM_SCOPES.SYSTEM,
      context,
      sessionId,
      processType: 'server',
      description: 'Default JTAG process'
    };
  }
}

/**
 * Validate process registration parameters
 */
export function validateRegisterProcessParams(params: RegisterProcessParams): string | null {
  if (!params.processType) {
    return 'processType is required';
  }
  
  if (!params.description || params.description.trim() === '') {
    return 'description is required and cannot be empty';
  }
  
  const validProcessTypes: ProcessType[] = ['server', 'browser', 'test', 'client'];
  if (!validProcessTypes.includes(params.processType)) {
    return `processType must be one of: ${validProcessTypes.join(', ')}`;
  }
  
  if (params.ports && params.ports.some(port => port <= 0 || port > 65535)) {
    return 'all ports must be between 1 and 65535';
  }
  
  return null;
}

/**
 * Get process capabilities based on process type
 */
export function getProcessCapabilities(processType: ProcessType): ProcessCapability[] {
  const capabilities: Record<ProcessType, ProcessCapability[]> = {
    server: ['websocket-server', 'command-execution', 'file-operations', 'console-logging'],
    browser: ['screenshot', 'dom-interaction', 'browser-automation', 'console-forwarding'],
    test: ['test-execution', 'process-spawning', 'validation'],
    client: ['command-sending', 'result-receiving']
  };

  return capabilities[processType] || [];
}

/**
 * Generate unique process identifier
 */
export function generateProcessId(): string {
  const timestamp = Date.now().toString(36);
  const random = Math.random().toString(36).substring(2, 10);
  const pid = (typeof process !== 'undefined' ? process.pid : Math.floor(Math.random() * 10000)).toString(36);
  return `jtag-${timestamp}-${pid}-${random}`;
}