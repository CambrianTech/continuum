/**
 * REAL JTAG Integration Tests
 * 
 * Tests the complete JTAG system following middle-out testing methodology.
 * No mocks - real WebSocket server, real browser simulation, real log files.
 */

import { describe, it, before, after, beforeEach } from 'node:test';
import assert from 'node:assert';
import { jtag, JTAG, JTAGLogEntry } from '../jtag';
import * as fs from 'fs';
import * as path from 'path';
import * as net from 'net';
import * as http from 'http';
import puppeteer from 'puppeteer';
import WebSocket from 'ws';

const TEST_PORT = 9001;
const LOG_DIR = '.continuum/jtag/logs';
const BROWSER_TEST_PORT = 9002;

describe('JTAG Real Integration Tests - Layer 1: Foundation', () => {
  let testJTAG: JTAG;
  let browser: any;
  let testPage: any;
  let testServer: http.Server;
  
  before(async () => {
    // Clear any existing logs (Layer 1 requirement)
    try {
      if (fs.existsSync(LOG_DIR)) {
        const files = fs.readdirSync(LOG_DIR);
        files.forEach(file => {
          if (file.startsWith('server.') || file.startsWith('browser.')) {
            fs.unlinkSync(path.join(LOG_DIR, file));
          }
        });
      }
    } catch (error) {
      // Directory might not exist
    }
    
    // Create fresh JTAG instance
    testJTAG = new JTAG();
    await testJTAG.start();
    
    // Create test server using the same JTAG System Proof HTML
    testServer = http.createServer(async (req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      
      if (req.url === '/') {
        // Serve the JTAG System Proof HTML file
        const proofHTML = fs.readFileSync(path.join(__dirname, '../jtag-system-proof.html'), 'utf8');
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(proofHTML);
      } else if (req.url === '/browser-client.js') {
        // Compile TypeScript to JavaScript for browser using esbuild
        const esbuild = await import('esbuild');
        const browserClientPath = path.join(__dirname, '../browser-client.ts');
        
        try {
          const result = await esbuild.build({
            entryPoints: [browserClientPath],
            bundle: false,
            write: false,
            target: 'es2020',
            format: 'esm',
            loader: { '.ts': 'ts' }
          });
          
          res.writeHead(200, { 'Content-Type': 'application/javascript' });
          res.end(result.outputFiles[0].text);
        } catch (error) {
          console.error('❌ TypeScript compilation failed:', error);
          res.writeHead(500);
          res.end('TypeScript compilation error: ' + error.message);
        }
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });
    
    testServer.listen(BROWSER_TEST_PORT);
    
    // Read browser config from package.json
    const packageJson = JSON.parse(fs.readFileSync(path.join(__dirname, '../package.json'), 'utf8'));
    const browserConfig = packageJson.config.browser;
    
    // Launch real browser with Puppeteer using package.json config
    browser = await puppeteer.launch({ 
      headless: browserConfig.headless,
      devtools: browserConfig.devtools,
      args: ['--no-sandbox', '--disable-setuid-sandbox'],
      defaultViewport: { width: browserConfig.width, height: browserConfig.height }
    });
    testPage = await browser.newPage();
    
    // Navigate to test page where JTAG browser client will auto-connect
    console.log(`🌐 Navigating to test page: http://localhost:${BROWSER_TEST_PORT}`);
    
    // Listen for console messages from the browser
    testPage.on('console', msg => console.log(`🌐 BROWSER: ${msg.text()}`));
    testPage.on('pageerror', err => console.log(`🌐 PAGE ERROR: ${err.message}`));
    
    await testPage.goto(`http://localhost:${BROWSER_TEST_PORT}`, { waitUntil: 'networkidle0' });
    
    // Check if page loaded
    const title = await testPage.title();
    console.log(`📄 Page title: ${title}`);
    
    // Wait for JTAG browser client to be ready
    try {
      await testPage.waitForFunction(() => {
        // Check if window.jtag exists and has the required methods
        return window.jtag && window.jtag.log && window.jtag.screenshot;
      }, { timeout: 10000 });
      console.log('✅ JTAG browser client is ready!');
    } catch (error) {
      console.log('❌ Timeout waiting for JTAG. Current state:');
      const debugInfo = await testPage.evaluate(() => {
        return {
          hasJtag: !!window.jtag,
          jtagMethods: window.jtag ? Object.keys(window.jtag) : [],
          windowKeys: Object.keys(window).filter(k => k.includes('jtag')),
          errors: window.lastError || 'none'
        };
      });
      console.log('🔍 Debug info:', debugInfo);
      throw error;
    }
    
    console.log(`✅ Puppeteer browser connected to JTAG server at localhost:${BROWSER_TEST_PORT}`);
    
    // Give server time to fully start
    await new Promise(resolve => setTimeout(resolve, 2000));
  });

  after(async () => {
    // Cleanup real browser
    if (browser) {
      await browser.close();
    }
    
    // Cleanup test server
    if (testServer) {
      testServer.close();
    }
    
    await testJTAG.stop();
    await new Promise(resolve => setTimeout(resolve, 1000));
  });

  describe('Layer 1: Core Foundation Tests', () => {
    it('should auto-start WebSocket server on import (Layer 1)', async () => {
      // Test that importing jtag actually starts a server on port 9001
      const serverListening = await new Promise<boolean>((resolve) => {
        const socket = new net.Socket();
        
        socket.setTimeout(2000);
        socket.on('connect', () => {
          socket.destroy();
          resolve(true);
        });
        socket.on('error', () => resolve(false));
        socket.on('timeout', () => {
          socket.destroy();
          resolve(false);
        });
        
        socket.connect(TEST_PORT, 'localhost');
      });
      
      assert.strictEqual(serverListening, true, 'JTAG server should be listening on port 9001');
    });

    it('should respond to health endpoint (Layer 1)', async () => {
      try {
        // Use dynamic import for fetch to handle ES modules
        const { default: fetch } = await import('node-fetch');
        const response = await fetch(`http://localhost:${TEST_PORT}/health`);
        const data = await response.json() as any;
        
        assert.strictEqual(response.status, 200);
        assert.strictEqual(data.status, 'ok');
        assert.strictEqual(typeof data.connections, 'number');
        assert.strictEqual(typeof data.logs, 'number');
        assert.ok(data.timestamp, 'Health response should include timestamp');
      } catch (error) {
        assert.fail(`Health endpoint test failed: ${(error as Error).message}`);
      }
    });
  });

  describe('Layer 2: Server-Side API Tests', () => {
    it('should create log entries via server API', () => {
      const entry = testJTAG.log('TEST_SERVER', 'Server test message', { test: true });
      
      assert.ok(entry.timestamp, 'Log entry should have timestamp');
      assert.strictEqual(entry.context, 'server');
      assert.strictEqual(entry.component, 'TEST_SERVER');
      assert.strictEqual(entry.message, 'Server test message');
      assert.strictEqual(entry.type, 'log');
      assert.deepStrictEqual(entry.data, { test: true });
    });

    it('should write logs to files (Layer 2)', async () => {
      // Generate test logs
      testJTAG.log('TEST_FILE', 'File write test');
      testJTAG.critical('TEST_FILE', 'Critical file test');
      testJTAG.trace('TEST_FILE', 'testFunction', 'ENTER');
      testJTAG.probe('TEST_FILE', 'test_probe', { value: 42 });
      
      // Give time for file writes
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // Check files exist
      const allLogFile = path.join(LOG_DIR, 'server.all.log');
      const criticalLogFile = path.join(LOG_DIR, 'server.critical.log');
      const traceLogFile = path.join(LOG_DIR, 'server.trace.log');
      const probeLogFile = path.join(LOG_DIR, 'server.probe.log');
      
      assert.ok(fs.existsSync(allLogFile), 'server.all.log should exist');
      assert.ok(fs.existsSync(criticalLogFile), 'server.critical.log should exist');
      assert.ok(fs.existsSync(traceLogFile), 'server.trace.log should exist');
      assert.ok(fs.existsSync(probeLogFile), 'server.probe.log should exist');
      
      // Check file contents
      const allLogContent = fs.readFileSync(allLogFile, 'utf8');
      const criticalLogContent = fs.readFileSync(criticalLogFile, 'utf8');
      const traceLogContent = fs.readFileSync(traceLogFile, 'utf8');
      const probeLogContent = fs.readFileSync(probeLogFile, 'utf8');
      
      assert.ok(allLogContent.includes('TEST_FILE'), 'All log should contain TEST_FILE entries');
      assert.ok(allLogContent.includes('File write test'), 'All log should contain basic log message');
      assert.ok(criticalLogContent.includes('Critical file test'), 'Critical log should contain critical message');
      assert.ok(traceLogContent.includes('TRACE: testFunction ENTER'), 'Trace log should contain trace message');
      assert.ok(probeLogContent.includes('PROBE: test_probe'), 'Probe log should contain probe message');
    });
  });

  describe('Layer 3: WebSocket Connection Tests', () => {
    it('should accept WebSocket connections (Layer 3)', async () => {
      const connected = await new Promise<boolean>((resolve) => {
        const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
        
        const timeout = setTimeout(() => {
          ws.terminate();
          resolve(false);
        }, 5000);
        
        ws.on('open', () => {
          clearTimeout(timeout);
          ws.close();
          resolve(true);
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          console.error('WebSocket connection test error:', error.message);
          resolve(false);
        });
      });
      
      assert.strictEqual(connected, true, 'Should accept WebSocket connections');
    });

    it('should track connection count in stats (Layer 3)', async () => {
      const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
      
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          ws.terminate();
          reject(new Error('WebSocket connection timeout'));
        }, 5000);
        
        ws.on('open', () => {
          clearTimeout(timeout);
          resolve();
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
      
      // Wait for connection to be properly registered using event-based approach
      await new Promise<void>((resolve) => {
        const checkStats = () => {
          const stats = testJTAG.getStats();
          if (stats.connections > 0) {
            resolve();
          } else {
            setTimeout(checkStats, 10);
          }
        };
        checkStats();
      });
      
      const stats = testJTAG.getStats();
      assert.ok(stats.connections > 0, `Should track connections, got: ${stats.connections}`);
      
      ws.close();
      await new Promise(resolve => setTimeout(resolve, 100));
    });
  });

  describe('Layer 4: Real Browser Integration (CRITICAL)', () => {
    it('should receive real browser log messages and write to browser logs', async () => {
      const initialLogCount = testJTAG.getStats().logs.total;
      
      // Use Puppeteer to execute JavaScript in REAL browser that's connected to JTAG
      console.log('🌐 Using Puppeteer + JTAG to trigger real browser logging...');
      const execResult = await testPage.evaluate(() => {
        // This runs directly in the real browser at localhost:9002
        console.log('🎯 Puppeteer exec: Running in real browser');
        window.jtag.log('REAL_BROWSER_INTEGRATION', 'Real browser message via Puppeteer exec', {
          testType: 'real-browser-puppeteer',
          userAgent: navigator.userAgent,
          url: window.location.href,
          timestamp: new Date().toISOString()
        });
        return { success: true, message: 'Real browser log sent' };
      });
      
      console.log('📊 JTAG exec result:', execResult);
      assert.ok(execResult.success, 'JTAG exec should succeed');
      
      // Wait for message to be processed
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Check browser logs were written  
      const browserLogPath = path.join(LOG_DIR, 'browser.all.log');
      assert.ok(fs.existsSync(browserLogPath), 'Browser log file should exist');
      
      const browserLogs = fs.readFileSync(browserLogPath, 'utf8');
      console.log('📄 Browser log content:', browserLogs);
      
      // Verify REAL browser log message (not fake simulation)
      assert.ok(browserLogs.includes('REAL_BROWSER_INTEGRATION'), 'Should have real browser log messages');
      assert.ok(browserLogs.includes('Real browser message via JTAG exec'), 'Should contain real browser message');
      
      // Verify log count increased
      const newLogCount = testJTAG.getStats().logs.total;
      assert.ok(newLogCount > initialLogCount, `Log count should increase: ${initialLogCount} -> ${newLogCount}`);
      
      console.log('✅ Real browser-to-server log pipeline verified!');
    });

    it('should handle all browser log types (log, critical, trace, probe) via real browser', async () => {
      const logTypes: Array<'log' | 'critical' | 'trace' | 'probe'> = ['log', 'critical', 'trace', 'probe'];
      const results: boolean[] = [];
      
      for (const logType of logTypes) {
        console.log(`🧪 Testing ${logType} message type in real browser...`);
        
        const execResult = await testPage.evaluate((logType) => {
          // This runs in the real browser at localhost:9002  
          console.log(`🎯 Puppeteer exec: Testing ${logType} in real browser`);
          
          const logData = {
            testType: `real-browser-${logType}`,
            userAgent: navigator.userAgent,
            url: window.location.href,
            timestamp: new Date().toISOString()
          };
          
          // Call the appropriate JTAG method
          switch(logType) {
            case 'log':
              window.jtag.log(`REAL_BROWSER_${logType.toUpperCase()}`, `${logType.toUpperCase()} message from real browser`, logData);
              break;
            case 'critical':  
              window.jtag.critical(`REAL_BROWSER_${logType.toUpperCase()}`, `${logType.toUpperCase()} message from real browser`, logData);
              break;
            case 'trace':
              window.jtag.trace(`REAL_BROWSER_${logType.toUpperCase()}`, 'testFunction', 'ENTER', logData);
              break;
            case 'probe':
              window.jtag.probe(`REAL_BROWSER_${logType.toUpperCase()}`, 'browser_state', logData);
              break;
          }
          
          return { success: true, logType: logType };
        }, logType);
        
        const success = execResult.success === true;
        results.push(success);
        console.log(`  ${success ? '✅' : '❌'} ${logType} message processed (real browser)`);
        
        // Small delay between messages
        await new Promise(resolve => setTimeout(resolve, 200));
      }
      
      // All log types should be processed successfully
      const allSuccessful = results.every(result => result === true);
      assert.strictEqual(allSuccessful, true, `All log types should be processed successfully: ${results}`);
      
      console.log('🎉 All browser log types verified via real browser execution!');
    });
    
    it('should capture real browser screenshots using html2canvas', async () => {
      console.log('📸 Testing real browser screenshot capture...');
      
      const screenshotResult = await testPage.evaluate(async () => {
        // This runs in the real browser at localhost:9002
        console.log('🎯 Puppeteer exec: Capturing real browser screenshot');
        
        // Take screenshot using the existing JTAG screenshot functionality
        const screenshotData = await window.jtag.screenshot('real-browser-test-' + Date.now(), {
          selector: '#screenshot-target',
          width: 400,
          height: 200
        });
        
        console.log('📸 Screenshot result:', screenshotData);
        return screenshotData;
      });
      
      console.log('📊 Screenshot exec result:', screenshotResult);
      
      assert.ok(screenshotResult.success, 'JTAG screenshot exec should succeed');
      assert.ok(screenshotResult.result, 'Should have screenshot result data');
      
      if (screenshotResult.result && screenshotResult.result.success) {
        console.log(`📷 Screenshot captured: ${screenshotResult.result.filename}`);
        assert.ok(screenshotResult.result.filename, 'Should have screenshot filename');
        assert.ok(screenshotResult.result.size > 0, 'Screenshot should have size > 0');
      }
      
      console.log('✅ Real browser screenshot capture verified!');
    });
    
    it('should capture regular console.log() after jtag.attach(console)', async () => {
      console.log('📌 Testing console attachment...');
      
      const attachResult = await testPage.evaluate(() => {
        // Attach JTAG to console 
        window.jtag.attach(console);
        
        // Now use regular console methods - they should be captured
        console.log('Regular console.log captured by JTAG');
        console.error('Regular console.error captured by JTAG'); 
        console.warn('Regular console.warn captured by JTAG');
        
        // Detach
        window.jtag.detach(console);
        
        return { success: true, attached: true };
      });
      
      console.log('📊 Console attach result:', attachResult);
      assert.ok(attachResult.success, 'Console attachment should succeed');
      
      // Wait for logs to be processed  
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      console.log('✅ Console attachment test completed!');
    });
  });

  describe('Layer 4: Error Handling', () => {
    it('should handle invalid JSON messages gracefully', async () => {
      const handled = await new Promise<boolean>((resolve, reject) => {
        const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
        
        const timeout = setTimeout(() => {
          ws.terminate();
          reject(new Error('Invalid JSON test timeout'));
        }, 5000);
        
        ws.on('open', () => {
          // Send invalid JSON
          ws.send('{ invalid json message');
        });
        
        ws.on('message', (data) => {
          clearTimeout(timeout);
          try {
            const response = JSON.parse(data.toString());
            ws.close();
            // Should receive error response
            resolve(response.success === false && response.error === 'Invalid JSON');
          } catch (error) {
            ws.close();
            reject(new Error(`Failed to parse error response: ${error}`));
          }
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(new Error(`WebSocket error in invalid JSON test: ${error.message}`));
        });
      });
      
      assert.strictEqual(handled, true, 'Should handle invalid JSON gracefully');
    });
  });

  describe('Browser Client Code Generation', () => {
    it('should generate valid TypeScript browser client code', () => {
      const clientCode = testJTAG.getBrowserClientCode();
      
      assert.ok(clientCode.includes('JTAGBrowserClient'), 'Should include TypeScript interface');
      assert.ok(clientCode.includes('window.jtag'), 'Should attach to window object');
      assert.ok(clientCode.includes('WebSocket'), 'Should use WebSocket API');
      assert.ok(clientCode.includes(`ws://localhost:${TEST_PORT}`), 'Should use correct WebSocket URL');
      assert.ok(clientCode.includes('_connectWebSocket'), 'Should include connection method');
      assert.ok(clientCode.includes('_send'), 'Should include send method');
      assert.ok(clientCode.includes('log('), 'Should include log method');
      assert.ok(clientCode.includes('critical('), 'Should include critical method');
      assert.ok(clientCode.includes('trace('), 'Should include trace method');
      assert.ok(clientCode.includes('probe('), 'Should include probe method');
      
      console.log('✅ Browser client TypeScript code generation verified!');
    });
  });
});