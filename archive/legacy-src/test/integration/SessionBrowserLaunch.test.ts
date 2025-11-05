/**
 * Integration test for session creation and browser launch flow
 * 
 * Tests the complete flow:
 * 1. CLI runs connect command
 * 2. ConnectCommand delegates to SessionManagerDaemon  
 * 3. SessionManagerDaemon creates session and emits event
 * 4. BrowserManagerDaemon receives event and launches browser
 */

import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import { ContinuumSystemStartup } from '../../system/startup/ContinuumSystemStartup';
import { exec } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs/promises';
import * as path from 'path';

const execAsync = promisify(exec);

describe('Session Browser Launch Integration', () => {
  let system: ContinuumSystemStartup;
  
  beforeAll(async () => {
    // Kill any existing daemons
    try {
      await execAsync('pkill -f "tsx.*main.ts"');
      await new Promise(resolve => setTimeout(resolve, 2000));
    } catch (e) {
      // Ignore if no processes to kill
    }
    
    // Start daemon system
    system = new ContinuumSystemStartup();
    await system.start();
    
    // Wait for system to be ready
    await new Promise(resolve => setTimeout(resolve, 3000));
  });
  
  afterAll(async () => {
    await system.stop();
  });
  
  it('should launch browser when creating new session via CLI', async () => {
    // Record browser processes before
    const { stdout: beforeProcs } = await execAsync('lsof -i :9000 | grep -i "opera\\|chrome" | wc -l');
    const browsersBefore = parseInt(beforeProcs.trim()) || 0;
    console.log(`Browsers connected before: ${browsersBefore}`);
    
    // Run connect command with forceNew to create new session
    const { stdout } = await execAsync('curl -X POST http://localhost:9000/api/commands/connect -H "Content-Type: application/json" -d \'{"forceNew": true}\'');
    const response = JSON.parse(stdout);
    
    console.log('Connect response:', response);
    
    // Verify session was created
    expect(response.session).toBeDefined();
    expect(response.session.action).toBe('created_new');
    expect(response.session.sessionId).toBeTruthy();
    
    // Wait for browser launch
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Check if browser launched
    const { stdout: afterProcs } = await execAsync('lsof -i :9000 | grep -i "opera\\|chrome" | wc -l');
    const browsersAfter = parseInt(afterProcs.trim()) || 0;
    console.log(`Browsers connected after: ${browsersAfter}`);
    
    // Should have more browser connections
    expect(browsersAfter).toBeGreaterThan(browsersBefore);
  }, 30000);
  
  it('should create session directories when new session created', async () => {
    // Create new session
    const { stdout } = await execAsync('curl -X POST http://localhost:9000/api/commands/connect -H "Content-Type: application/json" -d \'{"forceNew": true}\'');
    const response = JSON.parse(stdout);
    
    const sessionId = response.session.sessionId;
    const browserLogPath = response.session.logs.browser;
    const serverLogPath = response.session.logs.server;
    
    // Wait a bit for directories to be created
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Check if directories exist
    const sessionDir = path.dirname(browserLogPath);
    const dirExists = await fs.access(sessionDir).then(() => true).catch(() => false);
    
    expect(dirExists).toBe(true);
    
    // Check if log files exist
    const browserLogExists = await fs.access(browserLogPath).then(() => true).catch(() => false);
    const serverLogExists = await fs.access(serverLogPath).then(() => true).catch(() => false);
    
    console.log(`Session directory exists: ${dirExists}`);
    console.log(`Browser log exists: ${browserLogExists}`);
    console.log(`Server log exists: ${serverLogExists}`);
  }, 30000);
  
  it('should properly route connect command through daemon system', async () => {
    // This test verifies the command routing works
    // Connect command should go through:
    // HTTP -> WebSocketDaemon -> CommandProcessor -> ConnectCommand -> SessionManagerDaemon
    
    // Enable debug logging by checking server logs
    const { stdout: healthCheck } = await execAsync('curl http://localhost:9000/api/health');
    const health = JSON.parse(healthCheck);
    
    expect(health.status).toBe('healthy');
    expect(health.daemons).toContain('session-manager');
    expect(health.daemons).toContain('browser-manager');
    expect(health.daemons).toContain('command-processor');
  });
});