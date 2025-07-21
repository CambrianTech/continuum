/**
 * JTAG WebSocket Server Integration Test
 * 
 * Tests the ACTUAL WebSocket server that gets auto-started by importing the main JTAG module.
 * This should have caught the infinite retry loop bug.
 */

import { jtag } from '../index'; // This should auto-start WebSocket server
import * as net from 'net';

const JTAG_PORT = 9001;

describe('JTAG WebSocket Server Auto-Initialization', () => {
  
  beforeAll(async () => {
    // Give auto-initialization time to start server
    await new Promise(resolve => setTimeout(resolve, 1000));
  });

  test('should auto-start WebSocket server on import', async () => {
    // Test that port 9001 is actually listening
    const isListening = await new Promise<boolean>((resolve) => {
      const socket = new net.Socket();
      
      socket.setTimeout(2000);
      
      socket.on('connect', () => {
        socket.destroy();
        resolve(true);
      });
      
      socket.on('error', () => {
        resolve(false);
      });
      
      socket.on('timeout', () => {
        socket.destroy();
        resolve(false);
      });
      
      socket.connect(JTAG_PORT, 'localhost');
    });

    expect(isListening).toBe(true);
  });

  test('should handle WebSocket connection failure with retry limits', async () => {
    // This test should simulate what happens in browser when server is down
    const WebSocket = require('ws');
    
    let connectionAttempts = 0;
    let lastError: Error | null = null;
    
    // Try connecting to a port that definitely doesn't exist
    const badPort = 9999;
    
    const tryConnect = () => {
      return new Promise<void>((resolve, reject) => {
        connectionAttempts++;
        
        const ws = new WebSocket(`ws://localhost:${badPort}`);
        
        const timeout = setTimeout(() => {
          ws.terminate();
          lastError = new Error('Connection timeout');
          reject(lastError);
        }, 1000);
        
        ws.on('open', () => {
          clearTimeout(timeout);
          resolve();
        });
        
        ws.on('error', (error: Error) => {
          clearTimeout(timeout);
          lastError = error;
          reject(error);
        });
      });
    };

    // Try connecting with retry limits (like the fixed browser code)
    const maxRetries = 3;
    let retryCount = 0;
    
    while (retryCount < maxRetries) {
      try {
        await tryConnect();
        break; // Success
      } catch (error) {
        retryCount++;
        if (retryCount >= maxRetries) {
          break; // Give up
        }
        await new Promise(resolve => setTimeout(resolve, 100)); // Brief delay
      }
    }

    // Should have tried exactly maxRetries times
    expect(connectionAttempts).toBe(maxRetries);
    expect(lastError).toBeTruthy();
    expect(retryCount).toBe(maxRetries);
  });

  test('should successfully connect to real JTAG WebSocket server', async () => {
    const WebSocket = require('ws');
    
    const connected = await new Promise<boolean>((resolve) => {
      const ws = new WebSocket(`ws://localhost:${JTAG_PORT}`);
      
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

  test('should handle log messages via WebSocket', async () => {
    const WebSocket = require('ws');
    
    const messageHandled = await new Promise<boolean>((resolve) => {
      const ws = new WebSocket(`ws://localhost:${JTAG_PORT}`);
      
      const timeout = setTimeout(() => {
        ws.terminate();
        resolve(false);
      }, 5000);
      
      ws.on('open', () => {
        // Send a log message
        const logMessage = {
          type: 'log',
          payload: {
            timestamp: new Date().toISOString(),
            context: 'browser',
            component: 'WEBSOCKET_TEST',
            message: 'Test log message via WebSocket',
            data: { test: true },
            type: 'log'
          }
        };
        
        ws.send(JSON.stringify(logMessage));
      });
      
      ws.on('message', (data: Buffer) => {
        clearTimeout(timeout);
        const response = JSON.parse(data.toString());
        ws.close();
        resolve(response.success === true);
      });
      
      ws.on('error', () => {
        clearTimeout(timeout);
        resolve(false);
      });
    });

    expect(messageHandled).toBe(true);
  });
});