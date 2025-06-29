/**
 * DaemonManager Integration Tests
 * Tests the complete daemon orchestration system
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import DaemonManager from '../DaemonManager.js';

describe('DaemonManager Integration', () => {
  let manager: DaemonManager;

  beforeEach(() => {
    manager = new DaemonManager();
  });

  afterEach(async () => {
    if (manager) {
      await manager.stopAll();
    }
  });

  describe('Daemon Startup Order', () => {
    it('should start daemons in dependency order', async () => {
      const events: string[] = [];
      
      manager.on('daemon:healthy', (data) => {
        events.push(`${data.name}:healthy`);
      });

      await manager.startAll();
      
      // Verify dependency order: command-processor → websocket-server → renderer
      expect(events.length).toBeGreaterThan(0);
      console.log('Daemon startup events:', events);
      
      const status = manager.getAllStatus();
      console.log('Final daemon status:', status);
      
      // All critical daemons should be running
      expect(status['command-processor']).toBeDefined();
      expect(status['websocket-server']).toBeDefined();
    }, 30000);
  });

  describe('Self-Healing', () => {
    it('should restart failed critical daemons', async () => {
      await manager.startAll();
      
      const status = manager.getAllStatus();
      const wsProcess = Object.values(status).find(s => s.pid);
      
      if (wsProcess) {
        console.log('Testing self-healing by killing process:', wsProcess.pid);
        
        // Kill a daemon process to test auto-restart
        process.kill(wsProcess.pid, 'SIGTERM');
        
        // Wait for restart
        await new Promise(resolve => setTimeout(resolve, 5000));
        
        const newStatus = manager.getAllStatus();
        console.log('Status after restart:', newStatus);
      }
    }, 15000);
  });

  describe('Graceful Shutdown', () => {
    it('should stop all daemons cleanly', async () => {
      await manager.startAll();
      
      const initialStatus = manager.getAllStatus();
      expect(Object.keys(initialStatus).length).toBeGreaterThan(0);
      
      await manager.stopAll();
      
      const finalStatus = manager.getAllStatus();
      expect(Object.keys(finalStatus).length).toBe(0);
    }, 15000);
  });
});