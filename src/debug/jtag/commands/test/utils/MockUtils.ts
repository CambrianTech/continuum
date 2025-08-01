/**
 * Mock Utilities - Reusable mock objects and test data for JTAG command testing
 * 
 * Provides consistent mock implementations that can be used across all command tests.
 */

import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';
import { generateUUID } from '@shared/CrossPlatformUUID';
import type { JTAGSystem } from '@shared/JTAGSystem';
import type { JTAGClient } from '@shared/JTAGClient';
import type { CommandsInterface } from '@shared/JTAGBase';
import type { ListResult, CommandSignature } from '../../list/shared/ListTypes';

/**
 * Mock JTAG Context for testing
 */
export function createMockContext(environment: 'browser' | 'server' = 'server'): JTAGContext {
  return {
    uuid: generateUUID(),
    environment
  };
}

/**
 * Mock Command Signature for testing
 */
export function createMockCommandSignature(
  name: string,
  description: string = `Mock ${name} command`
): CommandSignature {
  return {
    name,
    description,
    category: 'system',
    params: {
      testParam: {
        type: 'string',
        required: false,
        description: 'Test parameter'
      }
    },
    returns: {
      success: {
        type: 'boolean',
        description: 'Test success status'
      },
      data: {
        type: 'any',
        description: 'Test result data'
      }
    }
  };
}

/**
 * Mock List Result for testing
 */
export function createMockListResult(commandCount: number = 20): ListResult {
  const commands: CommandSignature[] = [];
  
  const standardCommands = [
    'list', 'ping', 'screenshot', 'click', 'type', 'navigate', 'scroll',
    'get-text', 'wait-for-element', 'file/save', 'file/load', 'file/append',
    'chat/send-message', 'chat/get-chat-history', 'compile-typescript',
    'test-error', 'proxy-navigate', 'chat/room-events', 'chat/send-room-event'
  ];
  
  for (let i = 0; i < Math.min(commandCount, standardCommands.length); i++) {
    commands.push(createMockCommandSignature(standardCommands[i]));
  }
  
  // Add additional mock commands if needed
  for (let i = standardCommands.length; i < commandCount; i++) {
    commands.push(createMockCommandSignature(`mock-command-${i}`));
  }
  
  return {
    success: true,
    commands,
    totalCount: commandCount,
    context: createMockContext(),
    sessionId: generateUUID()
  };
}

/**
 * Mock Commands Interface for testing
 */
export function createMockCommandsInterface(): CommandsInterface {
  const commandsMap = new Map();
  
  // Add mock commands that return success
  const mockCommands = ['list', 'ping', 'screenshot', 'click', 'navigate'];
  
  for (const commandName of mockCommands) {
    commandsMap.set(commandName, {
      name: commandName,
      execute: async (params: any) => ({
        success: true,
        data: `Mock result for ${commandName}`,
        timestamp: new Date().toISOString(),
        context: params.context,
        sessionId: params.sessionId
      })
    });
  }
  
  return commandsMap;
}

/**
 * Mock JTAG System for testing
 */
export class MockJTAGSystem {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;
  
  constructor(context?: JTAGContext) {
    this.context = context || createMockContext();
    this.sessionId = this.context.uuid;
  }
  
  getCommandsInterface(): CommandsInterface {
    return createMockCommandsInterface();
  }
  
  get commands() {
    const commandsMap = this.getCommandsInterface();
    
    return new Proxy({}, {
      get: (target, commandName: string) => {
        const command = commandsMap.get(commandName);
        if (!command) {
          throw new Error(`Mock command '${commandName}' not found`);
        }
        
        return async (params: any) => {
          return await command.execute({
            ...params,
            context: params?.context || this.context,
            sessionId: params?.sessionId || this.sessionId
          });
        };
      }
    });
  }
  
  static async connect(): Promise<MockJTAGSystem> {
    return new MockJTAGSystem();
  }
}

/**
 * Mock JTAG Client for testing
 */
export class MockJTAGClient {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;
  private mockCommands: Map<string, any> = new Map();
  
  constructor(context?: JTAGContext) {
    this.context = context || createMockContext();
    this.sessionId = this.context.uuid;
    
    // Set up default mock commands
    this.addMockCommand('list', () => createMockListResult());
    this.addMockCommand('ping', () => ({
      success: true,
      environment: {
        type: this.context.environment,
        timestamp: new Date().toISOString(),
        platform: 'test-platform'
      }
    }));
  }
  
  addMockCommand(name: string, implementation: (params?: any) => any): void {
    this.mockCommands.set(name, implementation);
  }
  
  get commands() {
    const self = this;
    
    return new Proxy({}, {
      get: (target, commandName: string) => {
        return async (params?: any) => {
          const mockImpl = self.mockCommands.get(commandName);
          if (!mockImpl) {
            throw new Error(`Mock command '${commandName}' not implemented`);
          }
          
          return mockImpl({
            ...params,
            context: params?.context || self.context,
            sessionId: params?.sessionId || self.sessionId
          });
        };
      }
    });
  }
  
  getConnectionInfo() {
    return {
      connectionType: 'local' as const,
      reason: 'Mock connection',
      localSystemAvailable: true,
      sessionId: this.sessionId,
      environment: this.context.environment,
      isBootstrapSession: false
    };
  }
  
  static async connect(): Promise<{ client: MockJTAGClient; listResult: ListResult }> {
    const client = new MockJTAGClient();
    const listResult = await (client as any).list();
    
    return { client, listResult };
  }
}

/**
 * Mock environment data for testing
 */
export const MOCK_BROWSER_ENV = {
  type: 'browser' as const,
  userAgent: 'Mozilla/5.0 (Test Browser)',
  platform: 'test-platform',
  language: 'en-US',
  cookieEnabled: true,
  onLine: true,
  screenResolution: { width: 1920, height: 1080, colorDepth: 24 },
  viewport: { width: 1200, height: 800 },
  url: 'http://localhost:9002/test',
  timestamp: '2025-08-01T12:00:00.000Z'
};

export const MOCK_SERVER_ENV = {
  type: 'server' as const,
  nodeVersion: 'v18.0.0',
  platform: 'test-platform',
  arch: 'x64',
  processId: 12345,
  uptime: 300,
  memory: { used: 50000000, total: 100000000, usage: '50.0%' },
  cpuUsage: { user: 1000, system: 500 },
  timestamp: '2025-08-01T12:00:00.000Z'
};

/**
 * Create mock test payloads
 */
export function createMockPayload<T>(
  data: T,
  context?: JTAGContext,
  sessionId?: UUID
): T & JTAGPayload {
  return {
    context: context || createMockContext(),
    sessionId: sessionId || generateUUID(),
    ...data
  };
}

/**
 * Mock error scenarios for testing
 */
export const MOCK_ERROR_SCENARIOS = {
  NETWORK_ERROR: {
    name: 'Network Error',
    error: new Error('Network connection failed'),
    expectedMessage: 'Network connection failed'
  },
  TIMEOUT_ERROR: {
    name: 'Timeout Error', 
    error: new Error('Request timeout after 5000ms'),
    expectedMessage: 'timeout'
  },
  VALIDATION_ERROR: {
    name: 'Validation Error',
    error: new Error('Invalid parameters: missing required field'),
    expectedMessage: 'Invalid parameters'
  },
  COMMAND_NOT_FOUND: {
    name: 'Command Not Found',
    error: new Error('Command not available'),
    expectedMessage: 'not available'
  }
};

/**
 * Helper to create mock promises that resolve/reject for testing
 */
export function createMockPromise<T>(
  result: T,
  delay: number = 10,
  shouldReject: boolean = false
): Promise<T> {
  return new Promise((resolve, reject) => {
    setTimeout(() => {
      if (shouldReject) {
        reject(result);
      } else {
        resolve(result);
      }
    }, delay);
  });
}