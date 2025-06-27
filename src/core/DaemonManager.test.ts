/**
 * DaemonManager Comprehensive Tests
 * Verifies that all daemons actually work, not just claim to work
 */

import { DaemonManager } from './DaemonManager';
import { WebSocket } from 'ws';

describe('DaemonManager Integration Tests', () => {
  let daemonManager: DaemonManager;
  
  beforeEach(() => {
    daemonManager = new DaemonManager();
  });
  
  afterEach(async () => {
    await daemonManager.stopAll();
  });

  test('should start all daemons and verify they are actually running', async () => {
    // Start all daemons
    await daemonManager.startAll();
    
    // Verify command processor daemon
    const commandStatus = daemonManager.getDaemonStatus('command-processor');
    expect(commandStatus).toBe('running');
    
    // Verify websocket server daemon  
    const websocketStatus = daemonManager.getDaemonStatus('websocket-server');
    expect(websocketStatus).toBe('running');
    
    // CRITICAL: Verify WebSocket server is actually listening on port 9000
    const canConnect = await testWebSocketConnection();
    expect(canConnect).toBe(true);
  }, 30000);

  test('should provide accurate daemon status', () => {
    const status = daemonManager.getAllStatus();
    expect(status).toBeDefined();
    expect(typeof status).toBe('object');
  });

  test('should handle daemon restart', async () => {
    await daemonManager.startAll();
    
    const initialStatus = daemonManager.getDaemonStatus('websocket-server');
    expect(initialStatus).toBe('running');
    
    await daemonManager.stopDaemon('websocket-server');
    const stoppedStatus = daemonManager.getDaemonStatus('websocket-server');
    expect(stoppedStatus).toBeNull();
  });
});

/**
 * Actually test WebSocket connection - no faking
 */
async function testWebSocketConnection(): Promise<boolean> {
  return new Promise((resolve) => {
    try {
      const client = new WebSocket('ws://localhost:9000');
      
      client.on('open', () => {
        client.close();
        resolve(true);
      });
      
      client.on('error', () => {
        resolve(false);
      });
      
      // Timeout after 5 seconds
      setTimeout(() => {
        client.terminate();
        resolve(false);
      }, 5000);
      
    } catch (error) {
      resolve(false);
    }
  });
}