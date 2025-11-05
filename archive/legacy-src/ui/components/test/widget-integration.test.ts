#!/usr/bin/env tsx
/**
 * Widget Integration Test - Tests widget loading and functionality
 * This test validates that widgets load, render, and work correctly
 * Part of Layer 5 (UI Components) testing in MIDDLE-OUT methodology
 */

import { spawn } from 'child_process';
import { WebSocket } from 'ws';

class WidgetIntegrationTest {
  private daemonProcess: any = null;
  private testResults: any[] = [];

  log(message: string, data?: any) {
    const timestamp = new Date().toISOString();
    console.log(`[${timestamp}] 🧪 WIDGET TEST: ${message}`);
    if (data) console.log(`[${timestamp}] 📊`, JSON.stringify(data, null, 2));
  }

  async startDaemon(): Promise<void> {
    this.log("Starting daemon for widget testing...");
    
    return new Promise((resolve, reject) => {
      this.daemonProcess = spawn('npm', ['run', 'start:full'], { 
        stdio: ['pipe', 'pipe', 'pipe'],
        env: { ...process.env, NODE_ENV: 'test' }
      });

      let output = '';
      let readyTimeout: NodeJS.Timeout;

      this.daemonProcess.stdout.on('data', (data: Buffer) => {
        output += data.toString();
        if (output.includes('System ready and operational')) {
          clearTimeout(readyTimeout);
          this.log("Daemon started successfully for widget testing");
          resolve();
        }
      });

      this.daemonProcess.stderr.on('data', (data: Buffer) => {
        console.error('Daemon stderr:', data.toString());
      });

      this.daemonProcess.on('exit', (code: number) => {
        this.log(`Daemon exited with code ${code}`);
        if (code !== 0) {
          reject(new Error(`Daemon failed with code ${code}`));
        }
      });

      readyTimeout = setTimeout(() => {
        reject(new Error('Daemon startup timeout after 30s'));
      }, 30000);
    });
  }

  async testWidgetFiles(): Promise<boolean> {
    this.log("Testing widget file availability...");
    
    const widgetPaths = [
      '/dist/ui/components/Chat/ChatWidget',
      '/dist/ui/components/Sidebar/SidebarWidget',
      '/dist/ui/components/shared/BaseWidget'
    ];

    let allPassed = true;
    
    for (const path of widgetPaths) {
      try {
        const response = await fetch(`http://localhost:9000${path}`);
        const content = await response.text();
        
        const passed = response.status === 200 && content.length > 100;
        
        this.testResults.push({
          test: `Widget file: ${path}`,
          passed,
          status: response.status,
          contentLength: content.length,
          hasExport: content.includes('export') || content.includes('class'),
          hasCustomElement: content.includes('customElements.define')
        });
        
        if (!passed) allPassed = false;
        this.log(`Widget file test: ${path} - ${passed ? 'PASS' : 'FAIL'} (${response.status}, ${content.length} chars)`);
        
        // Check for CSS import errors
        if (content.includes('import') && content.includes('.css')) {
          const hasCSSError = content.includes('Failed to resolve') || content.includes('Cannot find module');
          if (hasCSSError) {
            this.testResults.push({
              test: `CSS import error in ${path}`,
              passed: false,
              error: 'CSS imports failing in TypeScript compilation'
            });
            allPassed = false;
          }
        }
        
      } catch (error) {
        this.testResults.push({
          test: `Widget file: ${path}`,
          passed: false,
          error: error.message
        });
        allPassed = false;
        this.log(`Widget file test: ${path} - FAIL (${error.message})`);
      }
    }
    
    return allPassed;
  }

  async testWidgetDiscoveryCommand(): Promise<boolean> {
    this.log("Testing widget discovery command...");
    
    try {
      // Test via WebSocket command (the way widgets actually discover themselves)
      const ws = new WebSocket('ws://localhost:9000');
      
      return new Promise((resolve) => {
        let commandSent = false;
        const timeout = setTimeout(() => {
          if (!commandSent) {
            ws.close();
            this.testResults.push({
              test: 'Widget discovery WebSocket command',
              passed: false,
              error: 'WebSocket connection timeout'
            });
            this.log("Widget discovery test - FAIL (WebSocket timeout)");
            resolve(false);
          }
        }, 10000);
        
        ws.on('open', () => {
          // Send widget discovery command
          const command = {
            type: 'execute_command',
            data: {
              command: 'discover_widgets',
              params: '{}',
              requestId: `widget_test_${Date.now()}`
            }
          };
          
          ws.send(JSON.stringify(command));
          commandSent = true;
          this.log("Widget discovery command sent");
        });
        
        ws.on('message', (data) => {
          try {
            const response = JSON.parse(data.toString());
            
            if (response.type === 'command_response' || response.type === 'execute_command_response') {
              clearTimeout(timeout);
              ws.close();
              
              const success = response.success !== false && !response.error;
              const widgetCount = response.data?.widgets?.length || 0;
              
              this.testResults.push({
                test: 'Widget discovery WebSocket command',
                passed: success,
                widgetCount,
                widgets: response.data?.widgets || [],
                response: response
              });
              
              this.log(`Widget discovery test - ${success ? 'PASS' : 'FAIL'} (found ${widgetCount} widgets)`);
              resolve(success);
            }
          } catch (error) {
            clearTimeout(timeout);
            ws.close();
            this.testResults.push({
              test: 'Widget discovery WebSocket command',
              passed: false,
              error: `Response parse error: ${error.message}`
            });
            this.log(`Widget discovery test - FAIL (parse error: ${error.message})`);
            resolve(false);
          }
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          this.testResults.push({
            test: 'Widget discovery WebSocket command',
            passed: false,
            error: error.message
          });
          this.log(`Widget discovery test - FAIL (${error.message})`);
          resolve(false);
        });
      });
      
    } catch (error) {
      this.testResults.push({
        test: 'Widget discovery command',
        passed: false,
        error: error.message
      });
      this.log(`Widget discovery test - FAIL (${error.message})`);
      return false;
    }
  }

  async testMainPageWithWidgets(): Promise<boolean> {
    this.log("Testing main page widget integration...");
    
    try {
      const response = await fetch('http://localhost:9000/');
      const html = await response.text();
      
      const hasWidgetElements = html.includes('<chat-widget>') && html.includes('<continuum-sidebar>');
      const hasWidgetScript = html.includes('continuum-browser');
      const hasWidgetCSS = html.includes('type="module"'); // Module scripts should be present
      
      const passed = response.status === 200 && hasWidgetElements && hasWidgetScript;
      
      this.testResults.push({
        test: 'Main page widget integration',
        passed,
        status: response.status,
        hasWidgetElements,
        hasWidgetScript,
        hasModuleScripts: hasWidgetCSS,
        htmlLength: html.length
      });
      
      this.log(`Main page widget test - ${passed ? 'PASS' : 'FAIL'}`);
      this.log(`  - Widget elements: ${hasWidgetElements ? 'YES' : 'NO'}`);
      this.log(`  - Widget script: ${hasWidgetScript ? 'YES' : 'NO'}`);
      
      return passed;
    } catch (error) {
      this.testResults.push({
        test: 'Main page widget integration',
        passed: false,
        error: error.message
      });
      this.log(`Main page widget test - FAIL (${error.message})`);
      return false;
    }
  }

  async testCSSFileHandling(): Promise<boolean> {
    this.log("Testing CSS file handling for widgets...");
    
    const cssFiles = [
      '/dist/ui/components/Chat/ChatWidget.css',
      '/dist/ui/components/Sidebar/SidebarWidget.css',
      '/dist/ui/components/shared/BaseWidget.css'
    ];

    let passedCount = 0;
    
    for (const cssPath of cssFiles) {
      try {
        const response = await fetch(`http://localhost:9000${cssPath}`);
        const content = await response.text();
        
        // CSS files should either exist or the server should handle the missing files gracefully
        const passed = response.status === 200 || response.status === 404; // 404 is acceptable, but 500 is not
        
        this.testResults.push({
          test: `CSS file: ${cssPath}`,
          passed,
          status: response.status,
          contentLength: content.length,
          isValidCSS: content.includes('{') || content.includes('/*') || response.status === 404
        });
        
        if (passed) passedCount++;
        this.log(`CSS file test: ${cssPath} - ${passed ? 'PASS' : 'FAIL'} (${response.status})`);
        
      } catch (error) {
        this.testResults.push({
          test: `CSS file: ${cssPath}`,
          passed: false,
          error: error.message
        });
        this.log(`CSS file test: ${cssPath} - FAIL (${error.message})`);
      }
    }
    
    return passedCount === cssFiles.length;
  }

  async checkSystemRunning(): Promise<boolean> {
    try {
      const response = await fetch('http://localhost:9000/api/health');
      return response.status === 200;
    } catch (error) {
      return false;
    }
  }

  async stopDaemon(): Promise<void> {
    if (this.daemonProcess) {
      this.log("Stopping daemon...");
      this.daemonProcess.kill('SIGTERM');
      
      return new Promise((resolve) => {
        setTimeout(() => {
          if (this.daemonProcess && !this.daemonProcess.killed) {
            this.daemonProcess.kill('SIGKILL');
          }
          resolve();
        }, 5000);
      });
    }
  }

  async runCompleteWidgetTest(): Promise<boolean> {
    this.log("=== STARTING COMPLETE WIDGET INTEGRATION TEST ===");
    
    try {
      // Check if system is already running
      const isRunning = await this.checkSystemRunning();
      
      if (!isRunning) {
        await this.startDaemon();
        // Wait for system to stabilize
        await new Promise(resolve => setTimeout(resolve, 8000));
      } else {
        this.log("System already running, using existing instance");
      }
      
      const tests = [
        await this.testWidgetFiles(),
        await this.testWidgetDiscoveryCommand(),
        await this.testMainPageWithWidgets(),
        await this.testCSSFileHandling()
      ];
      
      const allPassed = tests.every(test => test);
      
      this.log("=== WIDGET TEST RESULTS SUMMARY ===");
      this.testResults.forEach(result => {
        const status = result.passed ? '✅ PASS' : '❌ FAIL';
        console.log(`${status} ${result.test}`);
        if (result.error) console.log(`   Error: ${result.error}`);
        if (result.widgetCount !== undefined) console.log(`   Widgets found: ${result.widgetCount}`);
        if (result.hasWidgetElements !== undefined) console.log(`   Widget elements in HTML: ${result.hasWidgetElements}`);
      });
      
      this.log(`=== OVERALL WIDGET TEST RESULT: ${allPassed ? 'ALL TESTS PASSED' : 'SOME TESTS FAILED'} ===`);
      
      return allPassed;
      
    } catch (error) {
      this.log(`Widget integration test failed: ${error.message}`);
      return false;
    } finally {
      // Only stop daemon if we started it
      if (this.daemonProcess) {
        await this.stopDaemon();
      }
    }
  }
}

// Run the widget test
const test = new WidgetIntegrationTest();
test.runCompleteWidgetTest().then(success => {
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('Widget test execution failed:', error);
  process.exit(1);
});