/**
 * Layer 4: System Integration Tests
 * Tests full daemon system integration with HTTP endpoints
 * According to middle-out methodology, this requires Layers 1-3 working
 * 
 * REQUIRES: Daemons running on localhost:9000
 * Run with: npm run test:layer4 (after starting system)
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';
import { fileURLToPath } from 'node:url';

describe('Layer 4: System Integration Tests', () => {
  let systemProcess: ChildProcess | null = null;
  const __filename = fileURLToPath(import.meta.url);
  const __dirname = path.dirname(__filename);
  const projectRoot = path.resolve(__dirname, '../../../..');
  
  before(async function() {
    this.timeout(30000); // Give system time to start
    
    console.log('🚀 Starting Continuum system for Layer 4 tests...');
    
    // Start the system using npm run start
    systemProcess = spawn('npm', ['run', 'start'], {
      cwd: projectRoot,
      env: { ...process.env, NODE_ENV: 'test' },
      stdio: ['ignore', 'pipe', 'pipe']
    });
    
    // Wait for system to be ready
    await new Promise<void>((resolve, reject) => {
      let output = '';
      const timeout = setTimeout(() => {
        reject(new Error('System failed to start within 30 seconds'));
      }, 30000);
      
      systemProcess?.stdout?.on('data', (data) => {
        output += data.toString();
        console.log('System:', data.toString().trim());
        
        // Look for indicators that system is ready
        if (output.includes('Renderer daemon started on port 9000') ||
            output.includes('WebSocket server listening') ||
            output.includes('All daemons started successfully')) {
          clearTimeout(timeout);
          // Give it another second to stabilize
          setTimeout(resolve, 1000);
        }
      });
      
      systemProcess?.stderr?.on('data', (data) => {
        console.error('System Error:', data.toString());
      });
      
      systemProcess?.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });
    });
    
    console.log('✅ System started, beginning Layer 4 tests');
  });
  
  after(async () => {
    if (systemProcess) {
      console.log('🛑 Stopping Continuum system...');
      systemProcess.kill('SIGTERM');
      
      // Wait for graceful shutdown
      await new Promise<void>((resolve) => {
        systemProcess?.on('exit', () => {
          console.log('✅ System stopped');
          resolve();
        });
        
        // Force kill after 5 seconds
        setTimeout(() => {
          systemProcess?.kill('SIGKILL');
          resolve();
        }, 5000);
      });
    }
  });
  
  describe('HTTP API Endpoints', () => {
    it('should respond to health check endpoint', async () => {
      const response = await fetch('http://localhost:9000/api/commands/health', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ args: [] })
      });
      
      assert.strictEqual(response.status, 200, 'Health endpoint should return 200');
      const data = await response.json();
      assert(data.healthy !== undefined || data.status === 'healthy', 'Should return health status');
    });
    
    it('should handle connect command via HTTP', async () => {
      const response = await fetch('http://localhost:9000/api/commands/connect', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          args: [],
          sessionType: 'test',
          owner: 'layer4-test'
        })
      });
      
      assert.strictEqual(response.status, 200, 'Connect endpoint should return 200');
      const data = await response.json();
      assert(data.sessionId, 'Should return sessionId');
      assert(data.version, 'Should return version');
    });
    
    it('should handle unknown commands gracefully', async () => {
      const response = await fetch('http://localhost:9000/api/commands/nonexistent', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ args: [] })
      });
      
      // Could be 404 or 200 with error in response
      assert([200, 404, 500].includes(response.status), 'Should return appropriate status');
      
      if (response.status === 200) {
        const data = await response.json();
        assert(!data.success || data.error, 'Should indicate failure');
      }
    });
    
    it('should reject malformed JSON', async () => {
      const response = await fetch('http://localhost:9000/api/commands/health', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: 'invalid json'
      });
      
      assert([400, 500].includes(response.status), 'Should return error status for malformed JSON');
    });
  });
  
  describe('WebSocket Integration', () => {
    it('should establish WebSocket connection', async () => {
      // This would require ws package, skipping for now
      // Layer 4 focuses on HTTP endpoints
    });
  });
  
  describe('Static Asset Serving', () => {
    it('should serve HTML at root', async () => {
      const response = await fetch('http://localhost:9000/');
      assert.strictEqual(response.status, 200, 'Root should return 200');
      
      const contentType = response.headers.get('content-type');
      assert(contentType?.includes('text/html'), 'Should serve HTML');
    });
    
    it('should serve continuum-browser.js', async () => {
      const response = await fetch('http://localhost:9000/src/ui/continuum-browser.js');
      assert.strictEqual(response.status, 200, 'Browser JS should be served');
      
      const contentType = response.headers.get('content-type');
      assert(contentType?.includes('javascript'), 'Should serve JavaScript');
    });
  });
});