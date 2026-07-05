/**
 * REAL JTAG Integration Tests
 * 
 * These tests will prove whether the full browser-to-server WebSocket pipeline actually works.
 * No mocks, no fake connections - real integration testing.
 */

import fetch from 'node-fetch';
import { jtag, JTAG, JTAGLogEntry, JTAGStats } from '@tests/jtag';
import * as WebSocket from 'ws';
import * as fs from 'fs';
import * as path from 'path';

const TEST_PORT = 9001;
const LOG_DIR = '.continuum/jtag/logs';

describe('JTAG Real Integration Tests', () => {
  let testJTAG: JTAG;
  
  beforeAll(async () => {
    // Clear any existing logs
    try {
      const files = fs.readdirSync(LOG_DIR);
      files.forEach(file => {
        if (file.startsWith('server.')) {
          fs.unlinkSync(path.join(LOG_DIR, file));
        }
      });
    } catch (error) {
      // Directory might not exist
    }
    
    // Create fresh JTAG instance
    testJTAG = new JTAG();
    await testJTAG.start();
    
    // Give server time to fully start
    await new Promise(resolve => setTimeout(resolve, 2000));
  });

  afterAll(async () => {
    await testJTAG.stop();
    await new Promise(resolve => setTimeout(resolve, 1000));
  });

  describe('Server Auto-Start', () => {
    test('should start WebSocket server on port 9001', async () => {
      // Test raw socket connection to verify server is actually listening
      const connected = await new Promise<boolean>((resolve) => {
        const net = require('net');
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
      
      expect(connected).toBe(true);
    });

    test('should respond to health endpoint', async () => {
      const response = await fetch(`http://localhost:${TEST_PORT}/health`);
      const data = await response.json();
      
      expect(response.status).toBe(200);
      expect(data.status).toBe('ok');
      expect(typeof data.connections).toBe('number');
      expect(typeof data.logs).toBe('number');
      expect(data.timestamp).toBeDefined();
    });
  });

  describe('Server-Side Logging', () => {
    test('should create log entries via server API', () => {
      const entry = testJTAG.log('TEST_SERVER', 'Server test message', { test: true });
      
      expect(entry.timestamp).toBeDefined();
      expect(entry.context).toBe('server');
      expect(entry.component).toBe('TEST_SERVER');
      expect(entry.message).toBe('Server test message');
      expect(entry.type).toBe('log');
      expect(entry.data).toEqual({ test: true });
    });

    test('should write logs to files', async () => {
      testJTAG.log('TEST_FILE', 'File write test');
      testJTAG.critical('TEST_FILE', 'Critical file test');
      
      // Give time for file writes
      await new Promise(resolve => setTimeout(resolve, 100));
      
      const allLogFile = path.join(LOG_DIR, 'server.all.log');
      const criticalLogFile = path.join(LOG_DIR, 'server.critical.log');
      
      expect(fs.existsSync(allLogFile)).toBe(true);
      expect(fs.existsSync(criticalLogFile)).toBe(true);
      
      const allLogContent = fs.readFileSync(allLogFile, 'utf8');
      const criticalLogContent = fs.readFileSync(criticalLogFile, 'utf8');
      
      expect(allLogContent).toContain('TEST_FILE');
      expect(allLogContent).toContain('File write test');
      expect(criticalLogContent).toContain('Critical file test');
    });
  });

  describe('WebSocket Connection', () => {
    test('should accept WebSocket connections', async () => {
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
        
        ws.on('error', () => {
          clearTimeout(timeout);
          resolve(false);
        });
      });
      
      expect(connected).toBe(true);
    });

    test('should track connection count in stats', async () => {
      const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
      
      await new Promise<void>((resolve) => {
        ws.on('open', resolve);
      });
      
      const stats = testJTAG.getStats();
      expect(stats.connections).toBeGreaterThan(0);
      
      ws.close();
      
      // Give time for connection to close
      await new Promise(resolve => setTimeout(resolve, 100));
    });
  });

  describe('Browser-to-Server Log Pipeline (CRITICAL TEST)', () => {
    test('should receive browser log messages via WebSocket and write to server logs', async () => {
      const initialLogCount = testJTAG.getStats().logs.total;
      
      const browserLogSent = await new Promise<boolean>((resolve) => {
        const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
        
        const timeout = setTimeout(() => {
          ws.terminate();
          resolve(false);
        }, 10000);
        
        ws.on('open', () => {
          // Send a browser log message (simulating browser client)
          const browserLogMessage = {
            type: 'log',
            payload: {
              timestamp: new Date().toISOString(),
              context: 'browser',
              component: 'BROWSER_TEST',
              message: 'This message was sent from simulated browser client',
              data: { 
                userAgent: 'Test Browser',
                url: 'http://localhost:9002/test'
              },
              type: 'log'
            }
          };
          
          ws.send(JSON.stringify(browserLogMessage));
        });
        
        ws.on('message', (data) => {
          clearTimeout(timeout);
          try {
            const response = JSON.parse(data.toString());
            ws.close();
            resolve(response.success === true);
          } catch (error) {
            ws.close();
            resolve(false);
          }
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          console.error('WebSocket error in test:', error);
          resolve(false);
        });
      });
      
      expect(browserLogSent).toBe(true);
      
      // Give time for log processing
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // Verify log count increased
      const newLogCount = testJTAG.getStats().logs.total;
      expect(newLogCount).toBeGreaterThan(initialLogCount);
      
      // Verify the browser log was written to file
      const allLogFile = path.join(LOG_DIR, 'server.all.log');
      const logContent = fs.readFileSync(allLogFile, 'utf8');
      
      expect(logContent).toContain('BROWSER_TEST');
      expect(logContent).toContain('This message was sent from simulated browser client');
      expect(logContent).toContain('Test Browser');
    });

    test('should handle different log types from browser', async () => {
      const logTypes = ['log', 'critical', 'trace', 'probe'];
      const results: boolean[] = [];
      
      for (const logType of logTypes) {
        const result = await new Promise<boolean>((resolve) => {
          const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
          
          const timeout = setTimeout(() => {
            ws.terminate();
            resolve(false);
          }, 5000);
          
          ws.on('open', () => {
            const message = {
              type: 'log',
              payload: {
                timestamp: new Date().toISOString(),
                context: 'browser',
                component: 'BROWSER_TYPE_TEST',
                message: `${logType.toUpperCase()} message from browser`,
                data: { logType },
                type: logType
              }
            };
            
            ws.send(JSON.stringify(message));
          });
          
          ws.on('message', (data) => {
            clearTimeout(timeout);
            try {
              const response = JSON.parse(data.toString());
              ws.close();
              resolve(response.success === true);
            } catch (error) {
              ws.close();
              resolve(false);
            }
          });
          
          ws.on('error', () => {
            clearTimeout(timeout);
            resolve(false);
          });
        });
        
        results.push(result);
        
        // Small delay between messages
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      
      // All log types should be handled successfully
      expect(results.every(result => result === true)).toBe(true);
      
      // Give time for all logs to be written
      await new Promise(resolve => setTimeout(resolve, 500));
      
      // Verify all types were written to appropriate files
      const allLogContent = fs.readFileSync(path.join(LOG_DIR, 'server.all.log'), 'utf8');
      const criticalLogContent = fs.readFileSync(path.join(LOG_DIR, 'server.critical.log'), 'utf8');
      const traceLogContent = fs.readFileSync(path.join(LOG_DIR, 'server.trace.log'), 'utf8');
      const probeLogContent = fs.readFileSync(path.join(LOG_DIR, 'server.probe.log'), 'utf8');
      
      expect(allLogContent).toContain('BROWSER_TYPE_TEST');
      expect(criticalLogContent).toContain('CRITICAL message from browser');
      expect(traceLogContent).toContain('TRACE message from browser');
      expect(probeLogContent).toContain('PROBE message from browser');
    });
  });

  describe('Error Handling', () => {
    test('should handle invalid WebSocket messages gracefully', async () => {
      const handled = await new Promise<boolean>((resolve) => {
        const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
        
        const timeout = setTimeout(() => {
          ws.terminate();
          resolve(false);
        }, 5000);
        
        ws.on('open', () => {
          // Send invalid JSON
          ws.send('invalid json message');
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
            resolve(false);
          }
        });
        
        ws.on('error', () => {
          clearTimeout(timeout);
          resolve(false);
        });
      });
      
      expect(handled).toBe(true);
    });

    test('should handle unknown message types', async () => {
      const handled = await new Promise<boolean>((resolve) => {
        const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
        
        const timeout = setTimeout(() => {
          ws.terminate();
          resolve(false);
        }, 5000);
        
        ws.on('open', () => {
          ws.send(JSON.stringify({ type: 'unknown_type', payload: {} }));
        });
        
        ws.on('message', (data) => {
          clearTimeout(timeout);
          try {
            const response = JSON.parse(data.toString());
            ws.close();
            resolve(response.success === false && response.error === 'Unknown message type');
          } catch (error) {
            ws.close();
            resolve(false);
          }
        });
        
        ws.on('error', () => {
          clearTimeout(timeout);
          resolve(false);
        });
      });
      
      expect(handled).toBe(true);
    });
  });

  describe('HTTP Log Endpoint', () => {
    test('should accept log messages via HTTP POST', async () => {
      const logEntry: JTAGLogEntry = {
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'HTTP_TEST',
        message: 'HTTP log message',
        data: { method: 'POST' },
        type: 'log'
      };
      
      const response = await fetch(`http://localhost:${TEST_PORT}/jtag`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(logEntry)
      });
      
      const result = await response.json();
      
      expect(response.status).toBe(200);
      expect(result.success).toBe(true);
      expect(result.timestamp).toBeDefined();
      
      // Verify log was written
      await new Promise(resolve => setTimeout(resolve, 100));
      const logContent = fs.readFileSync(path.join(LOG_DIR, 'server.all.log'), 'utf8');
      expect(logContent).toContain('HTTP_TEST');
      expect(logContent).toContain('HTTP log message');
    });
  });
});

describe('JTAG Browser Client Code Generation', () => {
  test('should generate valid TypeScript browser client code', () => {
    const clientCode = jtag.getBrowserClientCode();
    
    expect(clientCode).toContain('JTAGBrowserClient');
    expect(clientCode).toContain('window.jtag');
    expect(clientCode).toContain('WebSocket');
    expect(clientCode).toContain('ws://localhost:9001');
    expect(clientCode).toContain('_connectWebSocket');
    expect(clientCode).toContain('_send');
    expect(clientCode).toContain('log(');
    expect(clientCode).toContain('critical(');
    expect(clientCode).toContain('trace(');
    expect(clientCode).toContain('probe(');
  });
});