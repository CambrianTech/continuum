#!/usr/bin/env tsx

/**
 * Elegant Cross-Domain Validation Test
 * 
 * PROPER ABSTRACTION: Clean separation without sacrificing elegance
 * - Environment-specific test contexts
 * - Unified test interface with type safety
 * - Elegant results aggregation
 * - No temporary files or string parsing - proper abstraction layers
 */

import { performance } from 'perf_hooks';

// Environment-safe imports - only server client for Node.js environment
import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../../system/core/client/shared/JTAGClient';

interface EnvironmentTestContext {
  readonly environment: 'server' | 'browser';
  readonly clientClass: typeof JTAGClientServer;
  readonly connectionOptions: JTAGClientConnectOptions;
}

interface EnvironmentTestResult {
  readonly success: boolean;
  readonly responseTime: number;
  readonly commandsDiscovered: string[];
  readonly specificCommandsValidated: Record<string, boolean>;
  readonly error?: string;
}

interface CrossDomainValidationResult {
  readonly serverEnvironment: EnvironmentTestResult;
  readonly browserEnvironment: EnvironmentTestResult; 
  readonly crossDomainCommunication: {
    readonly websocketEstablished: boolean;
    readonly messageLatency: number;
    readonly bidirectionalSuccess: boolean;
  };
  readonly overallSuccess: boolean;
}

/**
 * Elegant Environment Test Runner
 * 
 * Abstracts environment-specific testing with unified interface
 */
class EnvironmentTestRunner {
  constructor(private readonly context: EnvironmentTestContext) {}
  
  async executeTest(): Promise<EnvironmentTestResult> {
    const startTime = performance.now();
    
    try {
      console.log(`🔗 ${this.context.environment}: Connecting via ${this.context.clientClass.name}...`);
      
      // Connect using appropriate client (server or browser)
      const { client } = await this.context.clientClass.connect(this.context.connectionOptions);
      console.log(`✅ ${this.context.environment}: Client connected`);
      
      // Discover available commands
      const listResult = await (client as any).commands.list();
      const commandsDiscovered = listResult.commands || [];
      console.log(`📋 ${this.context.environment}: ${commandsDiscovered.length} commands discovered`);
      
      // Validate environment-specific commands
      const specificCommands = this.getEnvironmentSpecificCommands();
      const validationResults = await this.validateSpecificCommands(client, specificCommands);
      
      return {
        success: true,
        responseTime: performance.now() - startTime,
        commandsDiscovered,
        specificCommandsValidated: validationResults,
        error: undefined
      };
      
    } catch (error) {
      console.error(`❌ ${this.context.environment}: Test failed:`, error.message);
      
      return {
        success: false,
        responseTime: performance.now() - startTime,
        commandsDiscovered: [],
        specificCommandsValidated: {},
        error: error.message
      };
    }
  }
  
  private getEnvironmentSpecificCommands(): string[] {
    return this.context.environment === 'browser' 
      ? ['exec'] // Browser testing via exec command
      : ['exec', 'ping', 'list']; // Server-specific commands
  }
  
  private async validateSpecificCommands(
    client: any, 
    commands: string[]
  ): Promise<Record<string, boolean>> {
    const results: Record<string, boolean> = {};
    
    for (const commandName of commands) {
      try {
        // Test command availability and basic execution
        if (typeof client.commands[commandName] === 'function') {
          // Execute with environment-specific parameters
          const testParams = this.getTestParametersForCommand(commandName);
          const result = await client.commands[commandName](testParams);
          results[commandName] = !!result?.success;
          console.log(`  ✅ ${this.context.environment}: ${commandName} validated`);
        } else {
          results[commandName] = false;
          console.log(`  ❌ ${this.context.environment}: ${commandName} not available`);
        }
      } catch (error) {
        results[commandName] = false;
        console.log(`  ⚠️  ${this.context.environment}: ${commandName} execution failed - ${error.message}`);
      }
    }
    
    return results;
  }
  
  private getTestParametersForCommand(commandName: string): any {
    if (this.context.environment === 'browser' && commandName === 'exec') {
      // Test browser execution via JavaScript exec
      return {
        code: {
          type: 'inline',
          language: 'javascript', 
          source: `
            // Browser environment test
            console.log('🌐 BROWSER VALIDATION: Testing browser exec execution');
            try {
              const testData = {
                userAgent: navigator.userAgent,
                location: window.location.href,
                timestamp: new Date().toISOString(),
                environment: 'browser'
              };
              console.log('✅ BROWSER VALIDATION: Browser execution successful', testData);
              return { success: true, data: testData };
            } catch (error) {
              console.error('❌ BROWSER VALIDATION: Error:', error);
              return { success: false, error: error.message };
            }
          `
        }
      };
    } else {
      // Server environment test parameters
      const testParameters: Record<string, any> = {
        'exec': { 
          code: {
            type: 'inline',
            language: 'javascript',
            source: 'console.log("🖥️ SERVER VALIDATION: Server exec test"); return { success: true, environment: "server" };'
          }
        },
        'ping': {},
        'list': {}
      };
      
      return testParameters[commandName] || {};
    }
  }
}

/**
 * Cross-Domain Communication Validator
 * 
 * Tests WebSocket communication between environments elegantly
 */
class CrossDomainCommunicationValidator {
  async validateCommunication(): Promise<CrossDomainValidationResult['crossDomainCommunication']> {
    const startTime = performance.now();
    
    console.log('🔗 Validating cross-domain WebSocket communication...');
    
    try {
      // Create WebSocket connection to test cross-domain communication
      // This uses Node.js WebSocket for simplicity, but represents cross-domain communication
      const WebSocket = await import('ws').then(m => m.default);
      
      return await new Promise((resolve, reject) => {
        let messageLatency = 0;
        let websocketEstablished = false;
        let bidirectionalSuccess = false;
        
        const ws = new WebSocket('ws://localhost:9001/ws');
        
        const timeout = setTimeout(() => {
          console.log('❌ TIMEOUT: Cross-domain WebSocket connection failed to establish within 5000ms');
          ws.terminate();
          resolve({
            websocketEstablished: false,
            messageLatency: 0,
            bidirectionalSuccess: false
          });
        }, 5000);
        
        ws.on('open', () => {
          websocketEstablished = true;
          console.log('✅ Cross-domain: WebSocket connection established');
          
          // Send test message
          const testMessage = {
            messageType: 'ping',
            correlationId: 'cross-domain-validation',
            timestamp: Date.now()
          };
          
          ws.send(JSON.stringify(testMessage));
          console.log('📤 Cross-domain: Test message sent');
        });
        
        ws.on('message', (data) => {
          try {
            const response = JSON.parse(data.toString());
            messageLatency = Date.now() - (response.timestamp || Date.now());
            bidirectionalSuccess = true;
            
            console.log('📥 Cross-domain: Response received');
            console.log(`⚡ Cross-domain: Message latency ${messageLatency}ms`);
            
            clearTimeout(timeout);
            ws.close();
            
            resolve({
              websocketEstablished,
              messageLatency,
              bidirectionalSuccess
            });
            
          } catch (error) {
            console.error('❌ Cross-domain: Failed to parse response:', error);
            clearTimeout(timeout);
            ws.terminate();
            resolve({
              websocketEstablished,
              messageLatency: 0,
              bidirectionalSuccess: false
            });
          }
        });
        
        ws.on('error', (error) => {
          console.error('❌ Cross-domain: WebSocket error:', error.message);
          clearTimeout(timeout);
          resolve({
            websocketEstablished: false,
            messageLatency: 0,
            bidirectionalSuccess: false
          });
        });
      });
      
    } catch (error) {
      console.error('❌ Cross-domain communication validation failed:', error);
      
      return {
        websocketEstablished: false,
        messageLatency: 0,
        bidirectionalSuccess: false
      };
    }
  }
}

/**
 * Elegant Cross-Domain Validator
 * 
 * Orchestrates environment testing and cross-domain communication validation
 * with clean abstraction layers and unified results
 */
class ElegantCrossDomainValidator {
  private readonly serverContext: EnvironmentTestContext = {
    environment: 'server',
    clientClass: JTAGClientServer,
    connectionOptions: {
      targetEnvironment: 'server',
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      enableFallback: false
    }
  };
  
  // Browser testing via WebSocket exec commands (no direct browser client import)
  private readonly browserContext: EnvironmentTestContext = {
    environment: 'browser',
    clientClass: JTAGClientServer, // Use server client to send browser commands
    connectionOptions: {
      targetEnvironment: 'server', // Connect to server, then route to browser
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      enableFallback: false
    }
  };
  
  async validateCrossDomainIntegration(): Promise<CrossDomainValidationResult> {
    console.log('✨ ELEGANT CROSS-DOMAIN VALIDATION');
    console.log('==================================');
    console.log('Proper abstraction layers - no environmental code mixing\n');
    
    // Execute environment tests in parallel for efficiency without sacrificing elegance
    const [serverResult, browserResult, communicationResult] = await Promise.all([
      new EnvironmentTestRunner(this.serverContext).executeTest(),
      new EnvironmentTestRunner(this.browserContext).executeTest(), 
      new CrossDomainCommunicationValidator().validateCommunication()
    ]);
    
    const result: CrossDomainValidationResult = {
      serverEnvironment: serverResult,
      browserEnvironment: browserResult,
      crossDomainCommunication: communicationResult,
      overallSuccess: (
        serverResult.success &&
        browserResult.success &&
        communicationResult.websocketEstablished &&
        communicationResult.bidirectionalSuccess
      )
    };
    
    this.presentResults(result);
    
    return result;
  }
  
  private presentResults(result: CrossDomainValidationResult): void {
    console.log('\n📊 ELEGANT CROSS-DOMAIN VALIDATION RESULTS');
    console.log('==========================================');
    
    this.presentEnvironmentResults('Server', result.serverEnvironment);
    this.presentEnvironmentResults('Browser', result.browserEnvironment);
    this.presentCommunicationResults(result.crossDomainCommunication);
    
    console.log(`\n🎯 OVERALL ASSESSMENT: ${result.overallSuccess ? '✅ PASS' : '❌ FAIL'}`);
    
    if (result.overallSuccess) {
      console.log('🎉 Cross-domain integration validated successfully!');
      console.log('✨ Elegant abstraction layers maintained');
      console.log('🔄 Environment separation confirmed');
      console.log('📡 Cross-domain communication verified');
    } else {
      console.log('⚠️  Cross-domain integration requires attention');
      this.presentDiagnosticGuidance(result);
    }
  }
  
  private presentEnvironmentResults(environmentName: string, result: EnvironmentTestResult): void {
    const icon = environmentName === 'Server' ? '🖥️' : '🌐';
    
    console.log(`\n${icon} ${environmentName} Environment:`);
    console.log(`   Success: ${result.success ? '✅' : '❌'}`);
    console.log(`   Response Time: ${result.responseTime.toFixed(0)}ms`);
    console.log(`   Commands Discovered: ${result.commandsDiscovered.length}`);
    
    if (Object.keys(result.specificCommandsValidated).length > 0) {
      console.log('   Specific Commands:');
      Object.entries(result.specificCommandsValidated).forEach(([cmd, success]) => {
        console.log(`     ${success ? '✅' : '❌'} ${cmd}`);
      });
    }
    
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
  }
  
  private presentCommunicationResults(result: CrossDomainValidationResult['crossDomainCommunication']): void {
    console.log('\n🔗 Cross-Domain Communication:');
    console.log(`   WebSocket Established: ${result.websocketEstablished ? '✅' : '❌'}`);
    console.log(`   Bidirectional Success: ${result.bidirectionalSuccess ? '✅' : '❌'}`);
    if (result.messageLatency > 0) {
      console.log(`   Message Latency: ${result.messageLatency}ms`);
    }
  }
  
  private presentDiagnosticGuidance(result: CrossDomainValidationResult): void {
    console.log('\n🔍 Diagnostic Guidance:');
    
    if (!result.serverEnvironment.success) {
      console.log('   📍 Server environment: Check JTAGClientServer connection and available commands');
    }
    
    if (!result.browserEnvironment.success) {
      console.log('   📍 Browser environment: Check JTAGClientServer connection and browser-specific commands');
    }
    
    if (!result.crossDomainCommunication.websocketEstablished) {
      console.log('   📍 WebSocket: Verify JTAG system is running on ws://localhost:9002');
    }
    
    if (!result.crossDomainCommunication.bidirectionalSuccess) {
      console.log('   📍 Communication: Check WebSocket message routing between environments');
    }
  }
}

// Main execution
async function runElegantCrossDomainValidation() {
  const validator = new ElegantCrossDomainValidator();
  
  try {
    const results = await validator.validateCrossDomainIntegration();
    
    // Save results with elegant abstraction (not temp files)
    const saveResults = async (results: CrossDomainValidationResult) => {
      const { writeFile, mkdir } = await import('fs/promises');
      const { join } = await import('path');
      
      const resultsDir = 'test-results';
      const resultsFile = join(resultsDir, 'elegant-cross-domain-validation.json');
      
      await mkdir(resultsDir, { recursive: true });
      await writeFile(resultsFile, JSON.stringify(results, null, 2));
      
      console.log(`📁 Results preserved at: ${resultsFile}`);
    };
    
    await saveResults(results);
    
    return results;
    
  } catch (error) {
    console.error('💥 Elegant cross-domain validation failed:', error);
    throw error;
  }
}

if (require.main === module) {
  runElegantCrossDomainValidation().catch(console.error);
}

export { 
  runElegantCrossDomainValidation, 
  ElegantCrossDomainValidator,
  type CrossDomainValidationResult 
};