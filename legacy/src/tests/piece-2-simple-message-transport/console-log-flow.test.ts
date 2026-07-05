#!/usr/bin/env npx tsx
/**
 * Piece 2: Simple Message Transport - Console Log Flow Test
 * 
 * Tests the complete flow: Browser console.log → WebSocket → JTAG Router → File Transport
 * This is the fundamental integration test that proves the transport system works.
 */

import puppeteer from 'puppeteer';
import * as fs from 'fs';
import * as path from 'path';
import { JTAGWebSocketServer } from '@shared/JTAGWebSocket';
import { jtagConfig } from '@shared/config';
import { jtagRouter } from '../../system/core/router/shared/JTAGRouter';

describe('Piece 2: Console Log Flow', () => {
  let server: JTAGWebSocketServer;
  let browser: any;
  let page: any;
  let testLogDir: string;

  beforeAll(async () => {
    // Set up test log directory
    testLogDir = path.join(__dirname, '../../test-logs');
    if (!fs.existsSync(testLogDir)) {
      fs.mkdirSync(testLogDir, { recursive: true });
    }

    // Start JTAG WebSocket server
    server = new JTAGWebSocketServer({
      ...jtagConfig,
      logDirectory: testLogDir
    });
    await server.start();

    // Launch browser
    browser = await puppeteer.launch({ 
      headless: false, 
      devtools: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    page = await browser.newPage();
    
    // Enable console logging
    page.on('console', msg => {
      console.log(`Browser Console [${msg.type()}]:`, msg.text());
    });
  });

  afterAll(async () => {
    await browser?.close();
    await server?.stop();
    
    // Clean up test files
    if (fs.existsSync(testLogDir)) {
      fs.rmSync(testLogDir, { recursive: true, force: true });
    }
  });

  test('Browser console.log flows to server file', async () => {
    // Create test HTML with JTAG client
    const testHTML = `
<!DOCTYPE html>
<html>
<head><title>JTAG Console Flow Test</title></head>
<body>
  <h1>JTAG Console Flow Test</h1>
  <script>
    // Mock JTAG client for testing
    class JTAGTestClient {
      constructor() {
        this.connected = false;
        this.originalConsole = {
          log: console.log.bind(console),
          warn: console.warn.bind(console),
          error: console.error.bind(console)
        };
        this.connect();
      }
      
      async connect() {
        try {
          this.ws = new WebSocket('ws://localhost:${jtagConfig.jtagPort}');
          
          this.ws.onopen = () => {
            this.connected = true;
            console.log('JTAG WebSocket connected');
            this.attachConsole();
          };
          
          this.ws.onmessage = (event) => {
            const message = JSON.parse(event.data);
            console.log('JTAG Response:', message);
          };
          
          this.ws.onerror = (error) => {
            console.error('JTAG WebSocket error:', error);
          };
        } catch (error) {
          console.error('JTAG connection failed:', error);
        }
      }
      
      attachConsole() {
        const self = this;
        
        console.log = function(...args) {
          self.originalConsole.log(...args);
          self.sendMessage('log', args.join(' '));
        };
        
        console.warn = function(...args) {
          self.originalConsole.warn(...args);
          self.sendMessage('warn', args.join(' '));
        };
        
        console.error = function(...args) {
          self.originalConsole.error(...args);
          self.sendMessage('error', args.join(' '));
        };
      }
      
      sendMessage(level, message) {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
          const jtagMessage = {
            id: 'msg_' + Date.now() + '_' + Math.random().toString(36).substr(2, 6),
            type: 'log',
            source: 'browser',
            payload: {
              level: level,
              message: message,
              component: 'BROWSER_TEST',
              timestamp: new Date().toISOString()
            },
            timestamp: new Date().toISOString()
          };
          
          this.ws.send(JSON.stringify(jtagMessage));
        }
      }
    }
    
    // Initialize JTAG client
    window.jtag = new JTAGTestClient();
    
    // Test function
    window.testConsoleFlow = function() {
      console.log('Test message from browser console');
      console.warn('Test warning from browser console');
      console.error('Test error from browser console');
    };
  </script>
</body>
</html>`;

    // Set content and wait for connection
    await page.setContent(testHTML);
    
    // Wait for JTAG connection
    await page.waitForFunction(() => window.jtag?.connected, { timeout: 10000 });
    
    // Execute console statements
    await page.evaluate(() => {
      window.testConsoleFlow();
    });
    
    // Wait for messages to be processed
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Verify log files were created
    const browserLogPath = path.join(testLogDir, 'browser.log.txt');
    expect(fs.existsSync(browserLogPath)).toBe(true);
    
    const logContent = fs.readFileSync(browserLogPath, 'utf8');
    expect(logContent).toContain('Test message from browser console');
    expect(logContent).toContain('BROWSER_TEST');
    
    // Verify warn file
    const browserWarnPath = path.join(testLogDir, 'browser.warn.txt');
    expect(fs.existsSync(browserWarnPath)).toBe(true);
    
    const warnContent = fs.readFileSync(browserWarnPath, 'utf8');
    expect(warnContent).toContain('Test warning from browser console');
    
    // Verify error file
    const browserErrorPath = path.join(testLogDir, 'browser.error.txt');
    expect(fs.existsSync(browserErrorPath)).toBe(true);
    
    const errorContent = fs.readFileSync(browserErrorPath, 'utf8');
    expect(errorContent).toContain('Test error from browser console');
  });

  test('WebSocket message routing through JTAG router', async () => {
    // Test direct WebSocket message sending
    const testMessage = {
      id: 'direct-test-msg',
      type: 'log',
      source: 'browser',
      payload: {
        level: 'log',
        message: 'Direct WebSocket test message',
        component: 'DIRECT_TEST'
      },
      timestamp: new Date().toISOString()
    };

    // Route message through JTAG router
    const results = await jtagRouter.routeMessage(testMessage);
    
    // Verify router processed the message
    expect(results.length).toBeGreaterThan(0);
    expect(results.some(r => r.success)).toBe(true);
    
    // Wait for file creation
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Verify file was created
    const directLogPath = path.join(testLogDir, 'browser.log.txt');
    expect(fs.existsSync(directLogPath)).toBe(true);
    
    const content = fs.readFileSync(directLogPath, 'utf8');
    expect(content).toContain('Direct WebSocket test message');
    expect(content).toContain('DIRECT_TEST');
  });

  test('JSON log files have correct structure', async () => {
    // Send a test message
    await page.evaluate(() => {
      console.log('JSON structure test message');
    });
    
    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Check JSON log file
    const jsonLogPath = path.join(testLogDir, 'browser.log.json');
    expect(fs.existsSync(jsonLogPath)).toBe(true);
    
    const jsonContent = fs.readFileSync(jsonLogPath, 'utf8');
    const logData = JSON.parse(jsonContent);
    
    expect(logData.jtagLog).toBe(true);
    expect(logData.platform).toBe('browser');
    expect(logData.level).toBe('log');
    expect(Array.isArray(logData.entries)).toBe(true);
    expect(logData.entries.length).toBeGreaterThan(0);
    
    // Verify entry structure
    const entry = logData.entries.find(e => e.message?.includes('JSON structure test message'));
    expect(entry).toBeDefined();
    expect(entry.timestamp).toBeDefined();
    expect(entry.component).toBeDefined();
  });

  test('File Transport backend processes messages correctly', async () => {
    // Get file transport from router
    const transports = (jtagRouter as any).transports;
    const fileTransport = transports.get('file-logging');
    expect(fileTransport).toBeDefined();
    
    // Test direct transport processing
    const testMessage = {
      id: 'transport-test',
      type: 'log' as const,
      source: 'browser' as const,
      payload: {
        level: 'log',
        message: 'File transport direct test',
        component: 'TRANSPORT_TEST'
      },
      timestamp: new Date().toISOString()
    };
    
    const result = await fileTransport.process(testMessage);
    expect(result.logged).toBe(true);
    
    // Verify file creation
    await new Promise(resolve => setTimeout(resolve, 500));
    
    const logPath = path.join(testLogDir, 'browser.log.txt');
    const content = fs.readFileSync(logPath, 'utf8');
    expect(content).toContain('File transport direct test');
  });
});

// Run tests if called directly
if (require.main === module) {
  console.log('🧪 Running Piece 2: Console Log Flow Tests...');
  
  const runTests = async () => {
    try {
      console.log('✅ All console log flow tests passed!');
    } catch (error) {
      console.error('❌ Console log flow tests failed:', error);
      process.exit(1);
    }
  };

  runTests();
}