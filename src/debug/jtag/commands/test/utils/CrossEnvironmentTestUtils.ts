/**
 * Cross-Environment Testing Utilities
 * 
 * Ensures all commands are tested across both browser and server environments.
 * Provides patterns for testing location transparency and environment-specific behavior.
 */

import type { JTAGClient, JTAGClientConnectOptions } from '../../../system/core/client/shared/JTAGClient';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { 
  testClientCommandExecution,
  testClientConnection,
  type ConnectionTestScenario,
  type ConnectionTestResult
} from './ClientTestUtils';

/**
 * Environment test configuration
 */
export interface EnvironmentTestConfig {
  environments: ('browser' | 'server')[];
  commandName: string;
  testParams?: any;
  expectedFields?: string[];
  performanceThresholdMs?: number;
  validateEnvironmentData?: boolean;
}

/**
 * Cross-environment test result
 */
export interface CrossEnvironmentTestResult {
  commandName: string;
  environments: {
    environment: 'browser' | 'server';
    success: boolean;
    executionTime: number;
    result?: any;
    error?: string;
    connectionType?: 'local' | 'remote';
  }[];
  summary: {
    totalTests: number;
    passed: number;
    failed: number;
    avgExecutionTime: number;
    environmentsTestedSuccessfully: string[];
  };
}

/**
 * Test command across multiple environments
 */
export async function testCommandCrossEnvironment(
  config: EnvironmentTestConfig
): Promise<CrossEnvironmentTestResult> {
  console.log(`🌍 Testing ${config.commandName} across environments: ${config.environments.join(', ')}`);
  
  const results: CrossEnvironmentTestResult['environments'] = [];
  
  for (const environment of config.environments) {
    console.log(`\n🔄 Testing ${config.commandName} in ${environment} environment...`);
    
    try {
      // Connect to the target environment
      const { jtag } = await import('../../../server-index');
      const client = await jtag.connect({ targetEnvironment: environment });
      
      const connectionInfo = client.getConnectionInfo();
      
      // Execute command in this environment
      const { result, executionTime } = await testClientCommandExecution(
        client,
        config.commandName,
        config.testParams || {},
        config.performanceThresholdMs || 10000
      );
      
      // Type guard for result validation - JTAGPayload with additional properties
      if (!result || typeof result !== 'object') {
        throw new Error(`Invalid result type from ${environment}: expected object`);
      }
      
      const typedResult = result as JTAGPayload & Record<string, unknown>;
      
      // Validate result if fields specified
      if (config.expectedFields) {
        for (const field of config.expectedFields) {
          if (!(field in typedResult)) {
            throw new Error(`Missing expected field '${field}' in ${environment} result`);
          }
        }
      }
      
      // Validate environment data if requested
      if (config.validateEnvironmentData && typedResult.environment) {
        validateEnvironmentSpecificData(typedResult.environment, environment);
      }
      
      results.push({
        environment,
        success: true,
        executionTime,
        result,
        connectionType: connectionInfo.connectionType
      });
      
      console.log(`✅ ${config.commandName} succeeded in ${environment} (${executionTime}ms, ${connectionInfo.connectionType})`);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      results.push({
        environment,
        success: false,
        executionTime: 0,
        error: errorMessage
      });
      
      console.log(`❌ ${config.commandName} failed in ${environment}: ${errorMessage}`);
    }
  }
  
  // Calculate summary
  const passed = results.filter(r => r.success).length;
  const failed = results.filter(r => !r.success).length;
  const avgExecutionTime = results
    .filter(r => r.success)
    .reduce((sum, r) => sum + r.executionTime, 0) / Math.max(passed, 1);
  
  const environmentsTestedSuccessfully = results
    .filter(r => r.success)
    .map(r => r.environment);
  
  return {
    commandName: config.commandName,
    environments: results,
    summary: {
      totalTests: results.length,
      passed,
      failed,
      avgExecutionTime,
      environmentsTestedSuccessfully
    }
  };
}

/**
 * Validate environment-specific data structure
 */
function validateEnvironmentSpecificData(envData: any, expectedEnvironment: 'browser' | 'server'): void {
  if (!envData || typeof envData !== 'object') {
    throw new Error('Environment data must be an object');
  }
  
  if (!envData.type) {
    throw new Error('Environment data must have type field');
  }
  
  if (!envData.timestamp) {
    throw new Error('Environment data must have timestamp field');
  }
  
  if (!envData.platform) {
    throw new Error('Environment data must have platform field');
  }
  
  // Validate environment-specific fields
  if (expectedEnvironment === 'browser') {
    // Browser should have these fields (when actually running in browser)
    // Note: When testing through server with browser target, we might get server data
    if (envData.type === 'browser') {
      const requiredBrowserFields = ['userAgent', 'screenResolution', 'viewport'];
      for (const field of requiredBrowserFields) {
        if (!(field in envData)) {
          throw new Error(`Browser environment missing required field: ${field}`);
        }
      }
    }
  } else if (expectedEnvironment === 'server') {
    // Server should have these fields
    if (envData.type === 'server') {
      const requiredServerFields = ['nodeVersion', 'memory', 'processId'];
      for (const field of requiredServerFields) {
        if (!(field in envData)) {
          throw new Error(`Server environment missing required field: ${field}`);
        }
      }
    }
  }
}

/**
 * Test multiple commands across environments in batch
 */
export async function testMultipleCommandsCrossEnvironment(
  configs: EnvironmentTestConfig[]
): Promise<CrossEnvironmentTestResult[]> {
  console.log(`🧪 Testing ${configs.length} commands across environments`);
  
  const results: CrossEnvironmentTestResult[] = [];
  
  for (const config of configs) {
    try {
      const result = await testCommandCrossEnvironment(config);
      results.push(result);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Failed to test ${config.commandName}: ${errorMessage}`);
      
      // Add failed result
      results.push({
        commandName: config.commandName,
        environments: config.environments.map(env => ({
          environment: env,
          success: false,
          executionTime: 0,
          error: errorMessage
        })),
        summary: {
          totalTests: config.environments.length,
          passed: 0, 
          failed: config.environments.length,
          avgExecutionTime: 0,
          environmentsTestedSuccessfully: []
        }
      });
    }
  }
  
  return results;
}

/**
 * Generate comprehensive test report
 */
export function generateCrossEnvironmentReport(
  results: CrossEnvironmentTestResult[]
): string {
  const lines: string[] = [];
  
  lines.push('# Cross-Environment Test Report');
  lines.push(''); 
  
  // Overall summary
  const totalCommands = results.length;
  const successfulCommands = results.filter(r => r.summary.passed === r.summary.totalTests).length;
  const partialCommands = results.filter(r => r.summary.passed > 0 && r.summary.passed < r.summary.totalTests).length;
  const failedCommands = results.filter(r => r.summary.passed === 0).length;
  
  lines.push('## Overall Summary');
  lines.push(`- **Total Commands Tested**: ${totalCommands}`);
  lines.push(`- **Fully Successful**: ${successfulCommands}`);
  lines.push(`- **Partially Successful**: ${partialCommands}`);
  lines.push(`- **Failed**: ${failedCommands}`);
  lines.push('');
  
  // Environment compatibility matrix
  lines.push('## Environment Compatibility');
  lines.push('| Command | Browser | Server | Avg Time (ms) |');
  lines.push('|---------|---------|--------|---------------|');
  
  for (const result of results) {
    const browserStatus = result.environments.find(e => e.environment === 'browser')?.success ? '✅' : '❌';
    const serverStatus = result.environments.find(e => e.environment === 'server')?.success ? '✅' : '❌';
    const avgTime = result.summary.avgExecutionTime.toFixed(1);
    
    lines.push(`| ${result.commandName} | ${browserStatus} | ${serverStatus} | ${avgTime} |`);
  }
  lines.push('');
  
  // Detailed results
  lines.push('## Detailed Results');
  
  for (const result of results) {
    lines.push(`### ${result.commandName}`);
    lines.push(`- **Success Rate**: ${result.summary.passed}/${result.summary.totalTests}`);
    lines.push(`- **Average Execution Time**: ${result.summary.avgExecutionTime.toFixed(1)}ms`);
    lines.push(`- **Successful Environments**: ${result.summary.environmentsTestedSuccessfully.join(', ') || 'None'}`);
    
    for (const envResult of result.environments) {
      const status = envResult.success ? '✅' : '❌';
      const timeInfo = envResult.success ? ` (${envResult.executionTime}ms)` : '';
      const errorInfo = envResult.error ? ` - ${envResult.error}` : '';
      
      lines.push(`  - **${envResult.environment}**: ${status}${timeInfo}${errorInfo}`);
    }
    
    lines.push('');
  }
  
  return lines.join('\n');
}

/**
 * Standard cross-environment test configurations for common commands
 */
export const STANDARD_CROSS_ENVIRONMENT_CONFIGS: EnvironmentTestConfig[] = [
  {
    environments: ['browser', 'server'],
    commandName: 'ping',
    testParams: { includeEnvironment: true, includeTimestamp: true },
    expectedFields: ['success', 'message', 'environment', 'timestamp'],
    performanceThresholdMs: 3000,
    validateEnvironmentData: true
  },
  {
    environments: ['browser', 'server'],
    commandName: 'list',
    testParams: { category: 'all', includeDescription: true },
    expectedFields: ['success', 'commands', 'totalCount'],
    performanceThresholdMs: 5000,
    validateEnvironmentData: false
  },
  {
    environments: ['browser', 'server'],
    commandName: 'test-error',
    testParams: { triggerIn: 'validation', errorType: 'warning' },
    expectedFields: ['success'],
    performanceThresholdMs: 2000,
    validateEnvironmentData: false
  }
];

/**
 * Validate cross-environment test results meet requirements
 */
export function validateCrossEnvironmentResults(
  results: CrossEnvironmentTestResult[],
  requirements: {
    minimumSuccessRate?: number; // 0.0 to 1.0
    maxAvgExecutionTime?: number; // milliseconds
    requiredEnvironments?: ('browser' | 'server')[];
  } = {}
): { passed: boolean; violations: string[] } {
  const violations: string[] = [];
  
  const {
    minimumSuccessRate = 0.8,
    maxAvgExecutionTime = 5000,
    requiredEnvironments = ['browser', 'server']
  } = requirements;
  
  for (const result of results) {
    const successRate = result.summary.passed / result.summary.totalTests;
    
    // Check success rate
    if (successRate < minimumSuccessRate) {
      violations.push(`${result.commandName}: Success rate ${(successRate * 100).toFixed(1)}% below minimum ${(minimumSuccessRate * 100)}%`);
    }
    
    // Check performance
    if (result.summary.avgExecutionTime > maxAvgExecutionTime) {
      violations.push(`${result.commandName}: Average execution time ${result.summary.avgExecutionTime.toFixed(1)}ms exceeds limit ${maxAvgExecutionTime}ms`);
    }
    
    // Check required environments
    for (const requiredEnv of requiredEnvironments) {
      const envResult = result.environments.find(e => e.environment === requiredEnv);
      if (!envResult || !envResult.success) {
        violations.push(`${result.commandName}: Required environment '${requiredEnv}' not successful`);
      }
    }
  }
  
  return {
    passed: violations.length === 0,
    violations
  };
}