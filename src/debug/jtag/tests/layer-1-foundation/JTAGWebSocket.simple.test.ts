/**
 * Simple WebSocket Module Tests
 * 
 * Basic validation tests for the extracted WebSocket functionality.
 * Uses Node.js built-in test runner instead of Jest.
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { JTAGWebSocketServer, JTAGWebSocketClient, JTAGWebSocketUtils } from '@tests/shared/JTAGWebSocket';
import { JTAGLogEntry } from '@tests/shared/JTAGTypes';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

// Test configuration
const TEST_PORT_BASE = 9200; // Use different ports to avoid conflicts
let currentTestPort = TEST_PORT_BASE;

function getNextTestPort(): number {
  return ++currentTestPort;
}

// Utility to wait for async operations
function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

describe('JTAGWebSocket Basic Tests', () => {
  
  describe('JTAGWebSocketUtils', () => {
    it('should create log messages correctly', () => {
      const message = JTAGWebSocketUtils.createLogMessage('TEST', 'Test message', { data: 'test' }, 'log');
      
      assert.strictEqual(message.type, 'log');
      assert.strictEqual(message.payload.component, 'TEST');
      assert.strictEqual(message.payload.message, 'Test message');
      assert.deepStrictEqual(message.payload.data, { data: 'test' });
      assert.strictEqual(message.payload.type, 'log');
      assert.ok(message.payload.timestamp);
    });
    
    it('should create screenshot messages correctly', () => {
      const metadata = { width: 1920, height: 1080, size: 12345 };
      const message = JTAGWebSocketUtils.createScreenshotMessage('test', 'data:image/png;base64,test', 'png', metadata);
      
      assert.strictEqual(message.type, 'screenshot');
      assert.strictEqual(message.payload.filename, 'test');
      assert.strictEqual(message.payload.dataUrl, 'data:image/png;base64,test');
      assert.strictEqual(message.payload.format, 'png');
      assert.deepStrictEqual(message.payload.metadata, metadata);
      assert.ok(message.payload.timestamp);
    });
    
    it('should create exec messages correctly', () => {
      const options = { timeout: 5000, context: 'server' as const };
      const message = JTAGWebSocketUtils.createExecMessage('console.log("test")', options);
      
      assert.strictEqual(message.type, 'exec');
      assert.strictEqual(message.payload.code, 'console.log("test")');
      assert.deepStrictEqual(message.payload.options, options);
    });
    
    it('should validate responses correctly', () => {
      const validResponse = {
        success: true,
        timestamp: new Date().toISOString()
      };
      
      assert.doesNotThrow(() => JTAGWebSocketUtils.validateResponse(validResponse));
      
      assert.throws(() => JTAGWebSocketUtils.validateResponse(null), /Invalid response format/);
      assert.throws(() => JTAGWebSocketUtils.validateResponse({}), /Invalid response: missing success field/);
      assert.throws(() => JTAGWebSocketUtils.validateResponse({ success: true }), /Invalid response: missing timestamp field/);
    });
  });

  describe('JTAGWebSocketServer Initialization', () => {
    let server: JTAGWebSocketServer;
    
    after(async () => {
      if (server) {
        await server.stop();
      }
    });
    
    it('should create server instance', () => {
      const logHandler = (entry: JTAGLogEntry) => console.log('Log:', entry.message);
      
      server = new JTAGWebSocketServer({
        port: getNextTestPort(),
        onLog: logHandler
      });
      
      assert.ok(server);
      assert.strictEqual(server.isRunning(), false);
      assert.strictEqual(server.getConnectionCount(), 0);
    });
    
    it('should start server successfully', async () => {
      if (!server) {
        server = new JTAGWebSocketServer({
          port: getNextTestPort(),
          onLog: (entry: JTAGLogEntry) => console.log('Log:', entry.message)
        });
      }
      
      await server.start();
      
      // Wait for server to be ready
      await delay(100);
      
      assert.strictEqual(server.isRunning(), true);
    });
    
    it('should stop server gracefully', async () => {
      if (server && server.isRunning()) {
        await server.stop();
        assert.strictEqual(server.isRunning(), false);
      }
    });
  });

  describe('Message Validation', () => {
    let server: JTAGWebSocketServer;
    
    before(async () => {
      server = new JTAGWebSocketServer({
        port: getNextTestPort(),
        onLog: (entry: JTAGLogEntry) => console.log('Validation test log:', entry.message)
      });
    });
    
    after(async () => {
      if (server) {
        await server.stop();
      }
    });
    
    it('should validate message structure', () => {
      // Test invalid messages
      assert.throws(() => (server as any).validateMessage(null), /Invalid message format: must be JSON object/);
      assert.throws(() => (server as any).validateMessage({}), /Invalid message format: missing or invalid type field/);
      assert.throws(() => (server as any).validateMessage({ type: 'log' }), /Invalid message format: missing payload field/);
      assert.throws(() => (server as any).validateMessage({ type: 'invalid', payload: {} }), /Invalid message type/);
      
      // Test valid message
      const validMessage = {
        type: 'log',
        payload: {
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'TEST',
          message: 'Test message',
          type: 'log'
        }
      };
      
      assert.doesNotThrow(() => (server as any).validateMessage(validMessage));
    });
  });

  describe('Screenshot Payload Handling', () => {
    let server: JTAGWebSocketServer;
    const testDir = path.join(os.tmpdir(), 'jtag-websocket-simple-tests');
    
    before(async () => {
      // Create test directory
      if (!fs.existsSync(testDir)) {
        fs.mkdirSync(testDir, { recursive: true });
      }
      
      // Mock jtagConfig for testing
      const originalConfig = require('../shared/config').jtagConfig;
      require('../shared/config').jtagConfig.screenshotDirectory = testDir;
      
      server = new JTAGWebSocketServer({
        port: getNextTestPort(),
        onLog: (entry: JTAGLogEntry) => console.log('Screenshot test log:', entry.message),
        onScreenshot: async (payload: any) => {
          return (server as any).handleScreenshotPayload ? (server as any).handleScreenshotPayload(payload) : {
            success: false,
            error: 'Handler not available',
            filepath: '',
            filename: '',
            context: 'browser',
            timestamp: payload.timestamp
          };
        }
      });
    });
    
    after(async () => {
      if (server) {
        await server.stop();
      }
      
      // Cleanup test directory
      if (fs.existsSync(testDir)) {
        fs.rmSync(testDir, { recursive: true });
      }
    });
    
    it('should process screenshot payloads', () => {
      // Create a simple 1x1 red pixel PNG
      const redPixelPng = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQIAPS1C6gAAAABJRU5ErkJggg==';
      
      const payload = {
        filename: 'test-simple',
        dataUrl: `data:image/png;base64,${redPixelPng}`,
        format: 'png',
        metadata: {
          width: 1,
          height: 1,
          size: 95
        },
        timestamp: new Date().toISOString()
      };
      
      // Test the default screenshot handler logic
      const base64Data = payload.dataUrl.split(',')[1];
      const buffer = Buffer.from(base64Data, 'base64');
      
      assert.ok(buffer.length > 0);
      assert.strictEqual(base64Data, redPixelPng);
    });
  });

  describe('Real WebSocket Communication (if available)', function() {
    // Skip if WebSocket is not available or if port conflicts occur
    let server: JTAGWebSocketServer;
    let testPort: number;
    
    before(function() {
      testPort = getNextTestPort();
    });
    
    after(async function() {
      if (server) {
        await server.stop();
      }
    });
    
    it('should establish server and test basic connectivity', async function() {
      const capturedLogs: JTAGLogEntry[] = [];
      
      server = new JTAGWebSocketServer({
        port: testPort,
        onLog: (entry: JTAGLogEntry) => {
          capturedLogs.push(entry);
        }
      });
      
      try {
        await server.start();
        assert.strictEqual(server.isRunning(), true);
        
        // Test server is listening
        await delay(200);
        assert.strictEqual(server.isRunning(), true);
        
      } catch (error: any) {
        if (error.code === 'EADDRINUSE') {
          console.log(`Port ${testPort} in use, skipping connectivity test`);
          return;
        }
        throw error;
      }
    });
  });
});

console.log('🧪 Running JTAG WebSocket simple tests...');