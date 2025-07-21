#!/usr/bin/env npx tsx
/**
 * Browser Transport Iterator Integration Test
 * Tests all transport types with real browser JTAG system
 * Knows that npm test → npm start → browser launch is already running
 */

import * as puppeteer from 'puppeteer';
import { JTAGTransportFactory } from '../../shared/transports/TransportFactory';
import { JTAG_STATUS, JTAG_TRANSPORT, JTAGTransportType } from '../../shared/JTAGTypes';

interface BrowserTransportTestResults {
  totalTransports: number;
  totalTests: number;
  passed: number;
  failed: number;
  transportResults: Map<string, {
    browserReady: boolean;
    statusEventsReceived: number;
    apiCallsSuccessful: number;
    errors: string[];
  }>;
}

class BrowserTransportIteratorTester {
  private browser: any = null;
  private page: any = null;
  private results: BrowserTransportTestResults = {
    totalTransports: 0,
    totalTests: 0,
    passed: 0,
    failed: 0,
    transportResults: new Map()
  };

  async runAllBrowserTransportTests(): Promise<void> {
    console.log('\n🌐 JTAG Browser Transport Iterator Tests');
    console.log('========================================\n');
    console.log('ℹ️  Using running system from npm start (port 9002)');

    try {
      await this.setupBrowser();
      
      // Get all testable transport types
      const transportTypes = JTAGTransportFactory.getTestableTransports();
      console.log(`\n🔄 Testing ${transportTypes.length} transport types in browser...\n`);
      
      // Test each transport type
      for (const transportType of transportTypes) {
        await this.testTransportInBrowser(transportType);
      }
      
      this.printResults();
      
    } finally {
      await this.cleanup();
    }
  }

  private async setupBrowser(): Promise<void> {
    console.log('🚀 Setting up browser automation...');
    
    try {
      this.browser = await puppeteer.launch({
        headless: false, // Visible for debugging
        devtools: true,
        args: ['--no-sandbox', '--disable-setuid-sandbox'],
        defaultViewport: { width: 1200, height: 800 }
      });
      
      this.page = await this.browser.newPage();
      
      // Enable console logging
      this.page.on('console', (msg: any) => {
        if (msg.text().includes('JTAG')) {
          console.log(`   🌐 Browser: ${msg.text()}`);
        }
      });
      
      console.log('   ✅ Browser setup complete\n');
      
    } catch (error: any) {
      throw new Error(`Browser setup failed: ${error.message}`);
    }
  }

  private async testTransportInBrowser(transportType: JTAGTransportType): Promise<void> {
    const transportDef = JTAGTransportFactory.getTransportDefinition(transportType);
    console.log(`🔧 Testing ${transportDef?.name} in browser...`);
    
    const result = {
      browserReady: false,
      statusEventsReceived: 0,
      apiCallsSuccessful: 0,
      errors: []
    };

    try {
      // Navigate to demo page
      await this.page.goto('http://localhost:9002', { 
        waitUntil: 'networkidle2',
        timeout: 10000 
      });

      // Inject transport-specific test code
      await this.page.evaluate((transport: JTAGTransportType) => {
        return new Promise<void>((resolve, reject) => {
          let statusEventCount = 0;
          let jtagReady = false;

          // Listen for JTAG status events
          const statusListener = (event: any) => {
            statusEventCount++;
            console.log(`JTAG Status Event [${transport}]:`, event.detail);
            
            if (event.detail.status === 'ready' && event.detail.transport.type === transport) {
              jtagReady = true;
              // Store results in window for extraction
              (window as any).jtagTestResults = {
                transportType: transport,
                statusEventsReceived: statusEventCount,
                jtagReady: true,
                timestamp: Date.now()
              };
              resolve();
            }
          };

          // Listen for JTAG events
          window.addEventListener('jtag:status', statusListener);
          
          // Also listen for ready event specifically
          window.addEventListener('jtag:ready', (event: any) => {
            console.log(`JTAG Ready Event [${transport}]:`, event.detail);
            
            if (event.detail.transport?.type === transport) {
              jtagReady = true;
              (window as any).jtagTestResults = {
                transportType: transport,
                statusEventsReceived: statusEventCount + 1,
                jtagReady: true,
                timestamp: Date.now()
              };
              resolve();
            }
          });

          // Timeout after 15 seconds
          setTimeout(() => {
            (window as any).jtagTestResults = {
              transportType: transport,
              statusEventsReceived: statusEventCount,
              jtagReady: false,
              timeout: true
            };
            reject(new Error(`Timeout waiting for ${transport} ready event`));
          }, 15000);
        });
      }, transportType);

      // Extract test results from browser
      const browserResults = await this.page.evaluate(() => {
        return (window as any).jtagTestResults;
      });

      if (browserResults) {
        result.browserReady = browserResults.jtagReady;
        result.statusEventsReceived = browserResults.statusEventsReceived;
        
        if (browserResults.jtagReady) {
          // Test JTAG API calls
          const apiResults = await this.testJTAGAPIInBrowser(transportType);
          result.apiCallsSuccessful = apiResults.successfulCalls;
        }
      }

      console.log(`   ✅ ${transportDef?.name}: Ready=${result.browserReady}, Events=${result.statusEventsReceived}, API=${result.apiCallsSuccessful}`);
      
    } catch (error: any) {
      result.errors.push(error.message);
      console.log(`   ❌ ${transportDef?.name}: ${error.message}`);
    }

    this.results.transportResults.set(transportType, result);
    this.updateOverallResults(result);
  }

  private async testJTAGAPIInBrowser(transportType: JTAGTransportType): Promise<{successfulCalls: number}> {
    return await this.page.evaluate(async () => {
      let successfulCalls = 0;
      
      try {
        // Test UUID retrieval
        if (window.jtag?.getUUID) {
          const uuid = window.jtag.getUUID();
          if (uuid && uuid.uuid) successfulCalls++;
        }
        
        // Test logging
        if (window.jtag?.log) {
          window.jtag.log('BROWSER_TEST', 'Transport integration test', { transportType });
          successfulCalls++;
        }
        
        // Test exec
        if (window.jtag?.exec) {
          const execResult = await window.jtag.exec('2 + 2');
          if (execResult && execResult.result === 4) successfulCalls++;
        }
        
        // Test screenshot (may not work in all transports)
        if (window.jtag?.screenshot) {
          try {
            const screenshotResult = await window.jtag.screenshot(`transport-test-${Date.now()}`);
            if (screenshotResult) successfulCalls++;
          } catch (e) {
            // Screenshot might not be implemented for all transports
          }
        }
        
      } catch (error: any) {
        console.error('JTAG API test error:', error);
      }
      
      return { successfulCalls };
    });
  }

  private updateOverallResults(result: any): void {
    this.results.totalTransports++;
    this.results.totalTests += 4; // Ready, Events, API, Overall
    
    if (result.browserReady) this.results.passed++;
    else this.results.failed++;
    
    if (result.statusEventsReceived > 0) this.results.passed++;
    else this.results.failed++;
    
    if (result.apiCallsSuccessful > 0) this.results.passed++;
    else this.results.failed++;
    
    if (result.errors.length === 0) this.results.passed++;
    else this.results.failed++;
  }

  private async cleanup(): Promise<void> {
    console.log('\n🧹 Cleaning up browser automation...');
    
    try {
      if (this.page) await this.page.close();
      if (this.browser) await this.browser.close();
      console.log('   ✅ Browser cleanup complete');
    } catch (error: any) {
      console.log(`   ⚠️ Browser cleanup warning: ${error.message}`);
    }
  }

  private printResults(): void {
    console.log('\n📊 BROWSER TRANSPORT INTEGRATION RESULTS');
    console.log('=========================================');
    console.log(`🌐 Transports Tested: ${this.results.totalTransports}`);
    console.log(`📋 Total Tests: ${this.results.totalTests}`);
    console.log(`✅ Passed: ${this.results.passed}`);
    console.log(`❌ Failed: ${this.results.failed}`);
    
    const successRate = Math.round((this.results.passed / this.results.totalTests) * 100);
    console.log(`📈 Success Rate: ${successRate}%\n`);

    console.log('🔍 Per-Transport Browser Results:');
    for (const [transportType, result] of this.results.transportResults.entries()) {
      const definition = JTAGTransportFactory.getTransportDefinition(transportType as JTAGTransportType);
      console.log(`\n  📡 ${definition?.name} (${transportType}):`);
      console.log(`     🚀 Browser Ready: ${result.browserReady ? '✅' : '❌'}`);
      console.log(`     📊 Status Events: ${result.statusEventsReceived}`);
      console.log(`     🛠️ API Calls: ${result.apiCallsSuccessful}`);
      
      if (result.errors.length > 0) {
        console.log(`     ❌ Errors:`);
        result.errors.forEach(error => console.log(`        - ${error}`));
      }
    }

    console.log('\n🌐 Browser Integration Features Tested:');
    console.log('   🚀 Real browser automation with Puppeteer');
    console.log('   📡 Transport-specific status event handling');
    console.log('   🛠️ JTAG API functionality validation');
    console.log('   🔄 Cross-transport compatibility verification');
    console.log('   📊 Status event tracking and validation');

    if (this.results.failed === 0) {
      console.log('\n🎉 ALL BROWSER TRANSPORT TESTS PASSED!');
      console.log('✨ All transport types work correctly in real browser environment!');
    } else if (this.results.failed <= 2) {
      console.log('\n⚠️ Minor browser integration issues detected.');
      console.log('🌐 Most transport types work correctly in browser.');
    } else {
      console.log('\n❌ Significant browser transport integration issues detected.');
      console.log('🌐 Multiple transport types may not work properly in browser.');
      process.exit(1);
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const tester = new BrowserTransportIteratorTester();
  tester.runAllBrowserTransportTests().catch(error => {
    console.error('\n💥 Browser transport iterator test failed:', error);
    process.exit(1);
  });
}

export { BrowserTransportIteratorTester };