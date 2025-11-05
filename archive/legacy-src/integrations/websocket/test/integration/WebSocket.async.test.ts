/**
 * WebSocket Async Integration Tests
 * 
 * CRITICAL ASYNC DEPENDENCY TESTING:
 * ==================================
 * BLOCKING ISSUE: WebSocket upgrade timeout preventing browser connection
 * ROOT CAUSE: Async dependency chains in connection handling
 * 
 * CONNECTION SEQUENCE DEPENDENCIES:
 * - HTTP request → WebSocket upgrade handshake → Client connection
 * - Daemon registration → Route handlers → Message routing
 * - Browser connection → Widget initialization → Command execution
 * 
 * ASYNC CHAIN TESTING REQUIREMENTS:
 * - Test WebSocket upgrade timeout handling
 * - Test connection establishment race conditions  
 * - Test concurrent connection attempts
 * - Test daemon-to-daemon async communication
 * - Test browser reconnection after daemon restart
 * - Test command execution async dependency chains
 * 
 * VERSION MISMATCH TESTING:
 * - Test static file serving with version cache busting
 * - Test version coordination between client/server
 * - Test graceful degradation when versions mismatch
 */

import { WebSocketDaemon } from '../../WebSocketDaemon';
import { ConnectionManager } from '../../core/ConnectionManager';
import { DynamicMessageRouter } from '../../core/DynamicMessageRouter';
import WebSocket from 'ws';
import { createServer } from 'http';

describe('WebSocket Async Integration Tests', () => {
  let daemon: WebSocketDaemon;
  let testPort: number;

  beforeAll(() => {
    // Use random port to avoid conflicts
    testPort = 9000 + Math.floor(Math.random() * 1000);
  });

  beforeEach(() => {
    daemon = new WebSocketDaemon({ port: testPort, host: 'localhost' });
  });

  afterEach(async () => {
    if (daemon.isRunning()) {
      await daemon.stop();
    }
    // Wait for port cleanup
    await new Promise(resolve => setTimeout(resolve, 100));
  });

  describe('WebSocket Upgrade Sequence Testing', () => {
    
    test('should handle WebSocket upgrade within timeout', async () => {
      await daemon.start();
      
      return new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('WebSocket connection timed out - this is the blocking issue!'));
        }, 5000);

        const ws = new WebSocket(`ws://localhost:${testPort}`);
        
        ws.on('open', () => {
          clearTimeout(timeout);
          ws.close();
          resolve();
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
    });

    test('should handle multiple concurrent connection attempts', async () => {
      await daemon.start();
      
      const connectionPromises = [];
      for (let i = 0; i < 5; i++) {
        connectionPromises.push(
          new Promise<boolean>((resolve, reject) => {
            const timeout = setTimeout(() => {
              reject(new Error(`Connection ${i} timed out`));
            }, 3000);

            const ws = new WebSocket(`ws://localhost:${testPort}`);
            
            ws.on('open', () => {
              clearTimeout(timeout);
              ws.close();
              resolve(true);
            });
            
            ws.on('error', () => {
              clearTimeout(timeout);
              resolve(false);
            });
          })
        );
      }
      
      const results = await Promise.all(connectionPromises);
      expect(results.filter(r => r === true).length).toBeGreaterThan(0);
    });

    test('should detect and prevent upgrade hanging', async () => {
      await daemon.start();
      
      // Test rapid connection/disconnection cycles
      for (let i = 0; i < 3; i++) {
        await new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => {
            reject(new Error('Connection cycle timed out'));
          }, 2000);

          const ws = new WebSocket(`ws://localhost:${testPort}`);
          
          ws.on('open', () => {
            // Immediately close and test next
            ws.close();
            clearTimeout(timeout);
            resolve();
          });
          
          ws.on('error', (error) => {
            clearTimeout(timeout);
            reject(error);
          });
        });
      }
    });
  });

  describe('Async Daemon Communication Testing', () => {
    
    test('should handle daemon registration async dependencies', async () => {
      await daemon.start();
      
      // Test daemon registration doesn't block
      const mockDaemon = {
        name: 'test-daemon',
        handleMessage: jest.fn().mockResolvedValue({ success: true })
      };
      
      // This should complete quickly without blocking
      const startTime = Date.now();
      daemon.registerDaemon('test-daemon', mockDaemon);
      const duration = Date.now() - startTime;
      
      expect(duration).toBeLessThan(100);
    });

    test('should handle message routing async chains', async () => {
      await daemon.start();
      
      const ws = new WebSocket(`ws://localhost:${testPort}`);
      
      return new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('Message routing timed out'));
        }, 3000);

        ws.on('open', () => {
          // Test message routing async chain
          const testMessage = {
            type: 'ping',
            data: { test: true },
            requestId: 'test-123'
          };
          
          ws.send(JSON.stringify(testMessage));
        });
        
        ws.on('message', (data) => {
          const response = JSON.parse(data.toString());
          expect(response.type).toBe('ping_response');
          clearTimeout(timeout);
          ws.close();
          resolve();
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
    });
  });

  describe('Version Mismatch and Static File Testing', () => {
    
    test('should serve static files without version conflicts', async () => {
      await daemon.start();
      
      // Test static file serving doesn't block
      const response = await fetch(`http://localhost:${testPort}/src/ui/continuum.js`);
      
      // Should either succeed or fail quickly, not timeout
      expect([200, 404]).toContain(response.status);
    });

    test('should handle cache busting parameters', async () => {
      await daemon.start();
      
      const urls = [
        `/src/ui/continuum.js?v=0.2.2177&bust=123`,
        `/src/ui/continuum.js?v=0.2.2193&bust=456`,
        `/src/ui/continuum.js`
      ];
      
      // All requests should complete quickly
      const promises = urls.map(async (url) => {
        const startTime = Date.now();
        const response = await fetch(`http://localhost:${testPort}${url}`);
        const duration = Date.now() - startTime;
        
        expect(duration).toBeLessThan(1000);
        return response.status;
      });
      
      const statuses = await Promise.all(promises);
      // Should get consistent responses, not timeouts
      expect(statuses.every(status => [200, 404].includes(status))).toBe(true);
    });
  });

  describe('Connection Recovery and Resilience', () => {
    
    test('should handle daemon restart without client hanging', async () => {
      await daemon.start();
      
      // Establish connection
      const ws = new WebSocket(`ws://localhost:${testPort}`);
      await new Promise<void>((resolve) => {
        ws.on('open', resolve);
      });
      
      // Restart daemon
      await daemon.stop();
      await daemon.start();
      
      // New connection should work
      const ws2 = new WebSocket(`ws://localhost:${testPort}`);
      
      return new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('Reconnection after restart failed'));
        }, 5000);

        ws2.on('open', () => {
          clearTimeout(timeout);
          ws2.close();
          resolve();
        });
        
        ws2.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
    });

    test('should handle browser reconnection gracefully', async () => {
      await daemon.start();
      
      // Simulate browser reconnection pattern
      for (let attempt = 0; attempt < 3; attempt++) {
        await new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => {
            reject(new Error(`Reconnection attempt ${attempt} failed`));
          }, 2000);

          const ws = new WebSocket(`ws://localhost:${testPort}`);
          
          ws.on('open', () => {
            // Send init message like browser does
            ws.send(JSON.stringify({
              type: 'client_init',
              data: { userAgent: 'test-browser' }
            }));
          });
          
          ws.on('message', () => {
            // Got response, close and try next
            ws.close();
            clearTimeout(timeout);
            resolve();
          });
          
          ws.on('error', (error) => {
            clearTimeout(timeout);
            reject(error);
          });
        });
      }
    });
  });

  describe('Command Execution Async Chains', () => {
    
    test('should handle command execution without blocking', async () => {
      await daemon.start();
      
      const ws = new WebSocket(`ws://localhost:${testPort}`);
      
      return new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('Command execution timed out'));
        }, 5000);

        ws.on('open', () => {
          // Test command execution chain
          const commandMessage = {
            type: 'execute_command',
            data: {
              command: 'selftest',
              params: {},
              requestId: 'cmd-test-123'
            }
          };
          
          ws.send(JSON.stringify(commandMessage));
        });
        
        ws.on('message', (data) => {
          const response = JSON.parse(data.toString());
          if (response.type === 'execute_command_response') {
            expect(response.data).toBeDefined();
            clearTimeout(timeout);
            ws.close();
            resolve();
          }
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
    });
  });

});