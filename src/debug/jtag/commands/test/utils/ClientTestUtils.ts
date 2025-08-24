/**
 * Client Testing Utilities - Reusable patterns for testing JTAG client connections
 * 
 * These utilities provide consistent patterns for testing client connections, bootstrap sessions,
 * and command execution across different environments.
 */

import type { JTAGClient, JTAGClientConnectOptions, JTAGClientConnectionResult } from '../../../system/core/client/shared/JTAGClient';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';
import type { ListResult } from '../../list/shared/ListTypes';

/**
 * Connection test scenario configuration
 */
export interface ConnectionTestScenario {
  name: string;
  options?: JTAGClientConnectOptions;
  expectedConnectionType?: 'local' | 'remote';
  expectedEnvironment?: 'browser' | 'server';
  shouldSucceed?: boolean;
  expectedError?: string;
  minimumCommands?: number;
}

/**
 * Interface for client classes that have static connect method
 */
export interface JTAGClientClass {
  new (context: JTAGContext): JTAGClient;
  connect: (options?: JTAGClientConnectOptions) => Promise<JTAGClientConnectionResult>;
}

/**
 * Connection test result validation
 */
export interface ConnectionTestResult {
  scenario: string;
  success: boolean;
  connectionType?: 'local' | 'remote';
  sessionId?: UUID;
  commandCount?: number;
  isBootstrapSession?: boolean;
  error?: string;
  executionTime?: number;
}

/**
 * Test client connection with full validation
 */
export async function testClientConnection(
  clientClass: JTAGClientClass,
  scenario: ConnectionTestScenario
): Promise<ConnectionTestResult> {
  const startTime = Date.now();
  
  try {
    console.log(`🧪 Testing scenario: ${scenario.name}`);
    
    // Connect using the client class
    const connectionResult = await clientClass.connect(scenario.options);
    
    // Validate connection result structure
    validateConnectionResult(connectionResult, scenario.name);
    
    // Get connection info for detailed validation
    const connectionInfo = connectionResult.client.getConnectionInfo();
    
    // Validate connection properties
    if (scenario.expectedConnectionType && connectionInfo.connectionType !== scenario.expectedConnectionType) {
      throw new Error(`Expected connectionType '${scenario.expectedConnectionType}', got '${connectionInfo.connectionType}'`);
    }
    
    if (scenario.expectedEnvironment && connectionInfo.environment !== scenario.expectedEnvironment) {
      throw new Error(`Expected environment '${scenario.expectedEnvironment}', got '${connectionInfo.environment}'`);
    }
    
    if (scenario.minimumCommands && connectionResult.listResult.totalCount < scenario.minimumCommands) {
      throw new Error(`Expected at least ${scenario.minimumCommands} commands, got ${connectionResult.listResult.totalCount}`);
    }
    
    const executionTime = Date.now() - startTime;
    
    return {
      scenario: scenario.name,
      success: true,
      connectionType: connectionInfo.connectionType,
      sessionId: connectionInfo.sessionId,
      commandCount: connectionResult.listResult.totalCount,
      isBootstrapSession: connectionInfo.isBootstrapSession,
      executionTime
    };
    
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    const executionTime = Date.now() - startTime;
    
    if (scenario.shouldSucceed === false) {
      // Expected failure - validate error message if specified
      if (scenario.expectedError && !errorMessage.includes(scenario.expectedError)) {
        return {
          scenario: scenario.name,
          success: false,
          error: `Expected error containing "${scenario.expectedError}", got: "${errorMessage}"`,
          executionTime
        };
      }
      
      return {
        scenario: scenario.name,
        success: true, // Expected failure is success
        error: errorMessage,
        executionTime
      };
    }
    
    return {
      scenario: scenario.name,
      success: false,
      error: errorMessage,
      executionTime
    };
  }
}

/**
 * Validate connection result has required structure
 */
export function validateConnectionResult(
  result: unknown, 
  scenarioName: string = 'connection test'
): asserts result is JTAGClientConnectionResult {
  if (!result || typeof result !== 'object') {
    throw new Error(`${scenarioName}: Connection result must be an object`);
  }
  
  const typedResult = result as Record<string, unknown>;
  
  if (!typedResult.client || typeof typedResult.client !== 'object') {
    throw new Error(`${scenarioName}: Connection result must have 'client' property`);
  }
  
  const client = typedResult.client as Record<string, unknown>;
  if (typeof client.getConnectionInfo !== 'function') {
    throw new Error(`${scenarioName}: Client must have getConnectionInfo() method`);
  }
  
  if (!typedResult.listResult || typeof typedResult.listResult !== 'object') {
    throw new Error(`${scenarioName}: Connection result must have 'listResult' property`);
  }
  
  const listResult = typedResult.listResult as Record<string, unknown>;
  if (typeof listResult.totalCount !== 'number') {
    throw new Error(`${scenarioName}: listResult must have 'totalCount' number`);
  }
}

/**
 * Test bootstrap session handling
 */
export async function testBootstrapSessionHandling(
  clientClass: JTAGClientClass,
  options?: JTAGClientConnectOptions
): Promise<{
  initialSessionId: UUID;
  finalSessionId: UUID;
  bootstrapCompleted: boolean;
  sessionAssigned: boolean;
}> {
  console.log('🏷️ Testing bootstrap session handling...');
  
  const connectionResult = await clientClass.connect(options);
  const connectionInfo = connectionResult.client.getConnectionInfo();
  
  // Check if we started with UNKNOWN_SESSION (deadbeef)
  const wasBootstrap = connectionInfo.sessionId === 'deadbeef-cafe-4bad-8ace-5e551000c0de';
  const hasRealSession = connectionInfo.sessionId !== 'deadbeef-cafe-4bad-8ace-5e551000c0de';
  
  return {
    initialSessionId: 'deadbeef-cafe-4bad-8ace-5e551000c0de', // Always starts with this
    finalSessionId: connectionInfo.sessionId,
    bootstrapCompleted: !connectionInfo.isBootstrapSession,
    sessionAssigned: hasRealSession
  };
}

/**
 * Test command execution through client
 */
export async function testClientCommandExecution<TResult extends JTAGPayload = JTAGPayload>(
  client: JTAGClient,
  commandName: string,
  params: JTAGPayload,
  timeoutMs: number = 10000
): Promise<{ result: TResult; executionTime: number }> {
  console.log(`⚡ Testing command execution: ${commandName}`);
  
  const startTime = Date.now();
  
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`Command '${commandName}' timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    
    // Execute command through client - properly typed dynamic command access
    ((client.commands as any)[commandName](params) as Promise<TResult>)
      .then((result: TResult) => {
        clearTimeout(timer);
        const executionTime = Date.now() - startTime;
        resolve({ result, executionTime });
      })
      .catch((error: Error) => {
        clearTimeout(timer);
        reject(error);
      });
  });
}

/**
 * Test multiple connection scenarios in batch
 */
export async function testConnectionScenarios(
  clientClass: JTAGClientClass,
  scenarios: ConnectionTestScenario[]
): Promise<ConnectionTestResult[]> {
  const results: ConnectionTestResult[] = [];
  
  for (const scenario of scenarios) {
    try {
      const result = await testClientConnection(clientClass, scenario);
      results.push(result);
      
      if (result.success) {
        console.log(`✅ ${scenario.name}: ${result.connectionType} connection with ${result.commandCount} commands`);
      } else {
        console.log(`❌ ${scenario.name}: ${result.error}`);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      results.push({
        scenario: scenario.name,
        success: false,
        error: errorMessage
      });
      console.log(`❌ ${scenario.name}: ${errorMessage}`);
    }
  }
  
  return results;
}

/**
 * Standard connection test scenarios for all commands
 */
export const STANDARD_CONNECTION_SCENARIOS: ConnectionTestScenario[] = [
  {
    name: 'Server target connection',
    options: { targetEnvironment: 'server', sessionId: SYSTEM_SCOPES.SYSTEM },
    expectedEnvironment: 'server',
    minimumCommands: 15,
    shouldSucceed: true
  },
  {
    name: 'Browser target connection', 
    options: { targetEnvironment: 'browser', sessionId: SYSTEM_SCOPES.SYSTEM },
    expectedEnvironment: 'browser',
    minimumCommands: 15,
    shouldSucceed: true
  },
  {
    name: 'Default connection',
    options: undefined,
    expectedEnvironment: 'server', // Default for server-index
    minimumCommands: 15,
    shouldSucceed: true
  }
];

/**
 * Create assertion helpers for connection tests
 */
export function assertConnectionResult(
  result: ConnectionTestResult,
  expectedSuccess: boolean = true,
  testName: string = 'connection test'
): void {
  if (result.success !== expectedSuccess) {
    throw new Error(`${testName}: Expected success=${expectedSuccess}, got ${result.success}. Error: ${result.error}`);
  }
  
  if (expectedSuccess && result.isBootstrapSession) {
    throw new Error(`${testName}: Client stuck with bootstrap session - session assignment failed`);
  }
  
  if (expectedSuccess && result.commandCount !== undefined && result.commandCount < 1) {
    throw new Error(`${testName}: No commands discovered - command system may be broken`);
  }
}