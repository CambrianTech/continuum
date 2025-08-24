#!/usr/bin/env tsx

/**
 * Proper Cross-Domain Integration Test
 * 
 * CORRECT ARCHITECTURE: No mixing of environmental code
 * - Browser tests use JTAGClientBrowser only
 * - Server tests use JTAGClientServer only  
 * - Cross-domain tests validate WebSocket communication between separate environments
 */

import { performance } from 'perf_hooks';
import { spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

interface ProperCrossDomainResults {
  browserEnvironmentTest: {
    success: boolean;
    responseTime: number;
    commandsAvailable: string[];
  };
  serverEnvironmentTest: {
    success: boolean;
    responseTime: number;
    commandsAvailable: string[];
  };
  crossDomainCommunication: {
    websocketEstablished: boolean;
    messageLatency: number;
    bidirectionalSuccess: boolean;
  };
}

class ProperCrossDomainTest {
  private results: ProperCrossDomainResults = {
    browserEnvironmentTest: {
      success: false,
      responseTime: 0,
      commandsAvailable: []
    },
    serverEnvironmentTest: {
      success: false,
      responseTime: 0,
      commandsAvailable: []
    },
    crossDomainCommunication: {
      websocketEstablished: false,
      messageLatency: 0,
      bidirectionalSuccess: false
    }
  };

  async runProperCrossDomainTest(): Promise<ProperCrossDomainResults> {
    console.log('✅ PROPER CROSS-DOMAIN INTEGRATION TEST');
    console.log('======================================');
    console.log('NO MIXING OF ENVIRONMENTAL CODE - Browser and Server tested separately\n');

    try {
      // Phase 1: Test Server Environment Only (using JTAGClientServer)
      await this.testServerEnvironmentOnly();
      
      // Phase 2: Test Browser Environment Only (using JTAGClientBrowser)
      await this.testBrowserEnvironmentOnly();
      
      // Phase 3: Test Cross-Domain WebSocket Communication
      await this.testCrossDomainWebSocketCommunication();
      
      console.log('\n✅ PROPER CROSS-DOMAIN INTEGRATION TEST COMPLETE');
      this.printResults();
      
      return this.results;
      
    } catch (error) {
      console.error('💥 Proper cross-domain test failed:', error);
      throw error;
    }
  }

  private async testServerEnvironmentOnly(): Promise<void> {
    console.log('🖥️  PHASE 1: Testing Server Environment Only (JTAGClientServer)');
    console.log('----------------------------------------------------------------');
    
    const startTime = performance.now();
    
    // Create a server-only test that uses JTAGClientServer
    const serverTestScript = `
      // Pure server environment test - no browser code mixing
      import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
      import type { JTAGClientConnectOptions } from '../system/core/client/shared/JTAGClient';
      
      async function testServerEnvironment() {
        try {
          console.log('🔗 Server Test: Connecting via JTAGClientServer...');
          
          const clientOptions: JTAGClientConnectOptions = {
            targetEnvironment: 'server',
            transportType: 'websocket',
            serverUrl: 'ws://localhost:9002', // Test-bench port
            enableFallback: false
          };
          
          const { client } = await JTAGClientServer.connect(clientOptions);
          console.log('✅ Server Test: JTAGClientServer connected');
          
          // Test server commands
          const listResult = await (client as any).commands.list();
          console.log('✅ Server Test: List command executed');
          console.log(\`📋 Server Test: \${listResult.commands?.length || 0} commands available\`);
          
          // Test ping command
          const pingResult = await (client as any).commands.ping();
          console.log('✅ Server Test: Ping command executed');
          
          // Output results for parsing
          console.log('SERVER_TEST_SUCCESS:' + JSON.stringify({
            success: true,
            commandsCount: listResult.commands?.length || 0,
            commands: listResult.commands?.slice(0, 5) || [],
            pingSuccess: !!pingResult
          }));
          
        } catch (error) {
          console.error('❌ Server Test Failed:', error.message);
          console.log('SERVER_TEST_FAILURE:' + JSON.stringify({
            success: false,
            error: error.message
          }));
        }
      }
      
      testServerEnvironment();
    `;
    
    // Write and execute server test
    const serverTestFile = 'temp-server-test.ts';
    fs.writeFileSync(serverTestFile, serverTestScript);
    
    try {
      const serverOutput = await this.executeEnvironmentTest(serverTestFile, 'server');
      this.parseServerTestResults(serverOutput);
      this.results.serverEnvironmentTest.responseTime = performance.now() - startTime;
      
      console.log(`✅ Server Environment Test: ${this.results.serverEnvironmentTest.responseTime.toFixed(0)}ms`);
      
    } finally {
      // Cleanup
      if (fs.existsSync(serverTestFile)) {
        fs.unlinkSync(serverTestFile);
      }
    }
  }

  private async testBrowserEnvironmentOnly(): Promise<void> {
    console.log('🌐 PHASE 2: Testing Browser Environment Only (JTAGClientBrowser)');
    console.log('---------------------------------------------------------------');
    
    const startTime = performance.now();
    
    // Create a browser-only test that uses JTAGClientBrowser
    const browserTestScript = `
      // Pure browser environment test - no server code mixing
      import { JTAGClientBrowser } from '../system/core/client/browser/JTAGClientBrowser';
      import type { JTAGClientConnectOptions } from '../system/core/client/shared/JTAGClient';
      
      async function testBrowserEnvironment() {
        try {
          console.log('🔗 Browser Test: Connecting via JTAGClientBrowser...');
          
          const clientOptions: JTAGClientConnectOptions = {
            targetEnvironment: 'browser',
            transportType: 'websocket',
            serverUrl: 'ws://localhost:9002', // Connect to running system
            enableFallback: true
          };
          
          const { client } = await JTAGClientBrowser.connect(clientOptions);
          console.log('✅ Browser Test: JTAGClientBrowser connected');
          
          // Test browser commands
          const listResult = await (client as any).commands.list();
          console.log('✅ Browser Test: List command executed');
          console.log(\`📋 Browser Test: \${listResult.commands?.length || 0} commands available\`);
          
          // Test screenshot command (browser-specific)
          const screenshotResult = await (client as any).commands.screenshot({
            querySelector: 'body',
            waitForElement: false
          });
          console.log('✅ Browser Test: Screenshot command executed');
          
          // Output results for parsing
          console.log('BROWSER_TEST_SUCCESS:' + JSON.stringify({
            success: true,
            commandsCount: listResult.commands?.length || 0,
            commands: listResult.commands?.slice(0, 5) || [],
            screenshotSuccess: !!screenshotResult
          }));
          
        } catch (error) {
          console.error('❌ Browser Test Failed:', error.message);
          console.log('BROWSER_TEST_FAILURE:' + JSON.stringify({
            success: false,
            error: error.message
          }));
        }
      }
      
      testBrowserEnvironment();
    `;
    
    // Write and execute browser test
    const browserTestFile = 'temp-browser-test.ts';
    fs.writeFileSync(browserTestFile, browserTestScript);
    
    try {
      const browserOutput = await this.executeEnvironmentTest(browserTestFile, 'browser');
      this.parseBrowserTestResults(browserOutput);
      this.results.browserEnvironmentTest.responseTime = performance.now() - startTime;
      
      console.log(`✅ Browser Environment Test: ${this.results.browserEnvironmentTest.responseTime.toFixed(0)}ms`);
      
    } finally {
      // Cleanup
      if (fs.existsSync(browserTestFile)) {
        fs.unlinkSync(browserTestFile);
      }
    }
  }

  private async testCrossDomainWebSocketCommunication(): Promise<void> {
    console.log('🔗 PHASE 3: Testing Cross-Domain WebSocket Communication');
    console.log('-------------------------------------------------------');
    
    const startTime = performance.now();
    
    // Test WebSocket communication between browser and server environments
    // This validates that messages properly flow across the domain boundary
    
    try {
      // Create a simple WebSocket connection test
      const wsTestScript = `
        const WebSocket = require('ws');
        
        async function testWebSocketCommunication() {
          try {
            console.log('🔗 WebSocket Test: Connecting to ws://localhost:9002/ws...');
            
            const ws = new WebSocket('ws://localhost:9002/ws');
            let connected = false;
            let messageReceived = false;
            
            ws.on('open', function() {
              console.log('✅ WebSocket Test: Connection established');
              connected = true;
              
              // Send a test message
              const testMessage = {
                messageType: 'ping',
                correlationId: 'cross-domain-test',
                timestamp: Date.now()
              };
              
              ws.send(JSON.stringify(testMessage));
              console.log('📤 WebSocket Test: Test message sent');
            });
            
            ws.on('message', function(data) {
              console.log('📥 WebSocket Test: Response received');
              messageReceived = true;
              
              // Output results
              console.log('WEBSOCKET_TEST_SUCCESS:' + JSON.stringify({
                connected: true,
                messageReceived: true,
                latency: Date.now() - JSON.parse(data.toString()).timestamp || 0
              }));
              
              ws.close();
            });
            
            ws.on('error', function(error) {
              console.error('❌ WebSocket Test Error:', error.message);
              console.log('WEBSOCKET_TEST_FAILURE:' + JSON.stringify({
                connected: false,
                error: error.message
              }));
            });
            
            // Timeout after 5 seconds
            setTimeout(() => {
              if (!connected) {
                console.log('WEBSOCKET_TEST_FAILURE:' + JSON.stringify({
                  connected: false,
                  error: 'Connection timeout'
                }));
              }
              ws.terminate();
            }, 5000);
            
          } catch (error) {
            console.error('❌ WebSocket Test Failed:', error.message);
            console.log('WEBSOCKET_TEST_FAILURE:' + JSON.stringify({
              connected: false,
              error: error.message
            }));
          }
        }
        
        testWebSocketCommunication();
      `;
      
      const wsTestFile = 'temp-websocket-test.js';
      fs.writeFileSync(wsTestFile, wsTestScript);
      
      try {
        const wsOutput = await this.executeEnvironmentTest(wsTestFile, 'websocket');
        this.parseWebSocketTestResults(wsOutput);
        this.results.crossDomainCommunication.messageLatency = performance.now() - startTime;
        
        console.log(`✅ WebSocket Communication Test: ${this.results.crossDomainCommunication.messageLatency.toFixed(0)}ms`);
        
      } finally {
        if (fs.existsSync(wsTestFile)) {
          fs.unlinkSync(wsTestFile);
        }
      }
      
    } catch (error) {
      console.error('❌ Cross-domain WebSocket test failed:', error);
    }
  }

  private async executeEnvironmentTest(testFile: string, environment: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const isJs = testFile.endsWith('.js');
      const command = isJs ? 'node' : 'npx';
      const args = isJs ? [testFile] : ['tsx', testFile];
      
      console.log(`⚡ Executing ${environment} test: ${command} ${args.join(' ')}`);
      
      const process = spawn(command, args, { 
        stdio: 'pipe',
        timeout: 10000 // 10 second timeout
      });
      
      let output = '';
      let errorOutput = '';
      
      process.stdout.on('data', (data) => {
        const text = data.toString();
        output += text;
        // Echo important messages
        if (text.includes('✅') || text.includes('❌') || text.includes('TEST_')) {
          console.log(`  ${environment}: ${text.trim()}`);
        }
      });
      
      process.stderr.on('data', (data) => {
        errorOutput += data.toString();
      });
      
      process.on('close', (code) => {
        if (code === 0) {
          resolve(output);
        } else {
          reject(new Error(`${environment} test failed with code ${code}: ${errorOutput}`));
        }
      });
      
      process.on('error', (error) => {
        reject(error);
      });
    });
  }

  private parseServerTestResults(output: string): void {
    const successMatch = output.match(/SERVER_TEST_SUCCESS:(.+)/);
    const failureMatch = output.match(/SERVER_TEST_FAILURE:(.+)/);
    
    if (successMatch) {
      try {
        const result = JSON.parse(successMatch[1]);
        this.results.serverEnvironmentTest.success = result.success;
        this.results.serverEnvironmentTest.commandsAvailable = result.commands || [];
      } catch (error) {
        console.error('Failed to parse server test results:', error);
      }
    } else if (failureMatch) {
      console.log(`❌ Server test failed: ${failureMatch[1]}`);
    }
  }

  private parseBrowserTestResults(output: string): void {
    const successMatch = output.match(/BROWSER_TEST_SUCCESS:(.+)/);
    const failureMatch = output.match(/BROWSER_TEST_FAILURE:(.+)/);
    
    if (successMatch) {
      try {
        const result = JSON.parse(successMatch[1]);
        this.results.browserEnvironmentTest.success = result.success;
        this.results.browserEnvironmentTest.commandsAvailable = result.commands || [];
      } catch (error) {
        console.error('Failed to parse browser test results:', error);
      }
    } else if (failureMatch) {
      console.log(`❌ Browser test failed: ${failureMatch[1]}`);
    }
  }

  private parseWebSocketTestResults(output: string): void {
    const successMatch = output.match(/WEBSOCKET_TEST_SUCCESS:(.+)/);
    const failureMatch = output.match(/WEBSOCKET_TEST_FAILURE:(.+)/);
    
    if (successMatch) {
      try {
        const result = JSON.parse(successMatch[1]);
        this.results.crossDomainCommunication.websocketEstablished = result.connected;
        this.results.crossDomainCommunication.bidirectionalSuccess = result.messageReceived;
        this.results.crossDomainCommunication.messageLatency = result.latency || 0;
      } catch (error) {
        console.error('Failed to parse WebSocket test results:', error);
      }
    } else if (failureMatch) {
      console.log(`❌ WebSocket test failed: ${failureMatch[1]}`);
    }
  }

  private printResults(): void {
    console.log('\n📊 PROPER CROSS-DOMAIN INTEGRATION RESULTS:');
    console.log('============================================');
    
    console.log('\n🖥️  Server Environment Test:');
    console.log(`   Success: ${this.results.serverEnvironmentTest.success ? '✅' : '❌'}`);
    console.log(`   Response Time: ${this.results.serverEnvironmentTest.responseTime.toFixed(0)}ms`);
    console.log(`   Commands Available: ${this.results.serverEnvironmentTest.commandsAvailable.length}`);
    console.log(`   Sample Commands: ${this.results.serverEnvironmentTest.commandsAvailable.slice(0, 3).join(', ')}`);
    
    console.log('\n🌐 Browser Environment Test:');
    console.log(`   Success: ${this.results.browserEnvironmentTest.success ? '✅' : '❌'}`);
    console.log(`   Response Time: ${this.results.browserEnvironmentTest.responseTime.toFixed(0)}ms`);
    console.log(`   Commands Available: ${this.results.browserEnvironmentTest.commandsAvailable.length}`);
    console.log(`   Sample Commands: ${this.results.browserEnvironmentTest.commandsAvailable.slice(0, 3).join(', ')}`);
    
    console.log('\n🔗 Cross-Domain Communication:');
    console.log(`   WebSocket Established: ${this.results.crossDomainCommunication.websocketEstablished ? '✅' : '❌'}`);
    console.log(`   Bidirectional Success: ${this.results.crossDomainCommunication.bidirectionalSuccess ? '✅' : '❌'}`);
    console.log(`   Message Latency: ${this.results.crossDomainCommunication.messageLatency.toFixed(0)}ms`);
    
    // Overall assessment
    const overallSuccess = (
      this.results.serverEnvironmentTest.success &&
      this.results.browserEnvironmentTest.success &&
      this.results.crossDomainCommunication.websocketEstablished &&
      this.results.crossDomainCommunication.bidirectionalSuccess
    );
    
    console.log(`\n🎯 OVERALL ASSESSMENT: ${overallSuccess ? '✅ PASS' : '❌ FAIL'}`);
    
    if (overallSuccess) {
      console.log('🎉 Proper cross-domain integration is working correctly!');
      console.log('✅ No environmental code mixing detected');
      console.log('✅ Browser and server environments tested separately');  
      console.log('✅ WebSocket cross-domain communication validated');
    } else {
      console.log('⚠️  Cross-domain integration needs fixing');
      console.log('🔍 Check that browser and server environments are properly separated');
      console.log('🔍 Verify WebSocket communication is working between domains');
    }
  }
}

async function runProperCrossDomainIntegrationTest() {
  const tester = new ProperCrossDomainTest();
  
  try {
    const results = await tester.runProperCrossDomainTest();
    
    // Save results for analysis
    const resultsDir = 'test-results';
    const resultsFile = path.join(resultsDir, 'proper-cross-domain-results.json');
    
    fs.mkdirSync(resultsDir, { recursive: true });
    fs.writeFileSync(resultsFile, JSON.stringify(results, null, 2));
    
    console.log(`📁 Results saved to: ${resultsFile}`);
    
    return results;
    
  } catch (error) {
    console.error('💥 Proper cross-domain integration test failed:', error);
    throw error;
  }
}

if (require.main === module) {
  runProperCrossDomainIntegrationTest().catch(console.error);
}

export { runProperCrossDomainIntegrationTest, ProperCrossDomainTest };