/**
 * Real Console Capture Integration Test
 * 
 * VALIDATION REQUIREMENT: Prove that JavaScript execution produces console.log output
 * that gets captured by DevTools and written to session browser.log files
 * 
 * This is the git hook validation pattern:
 * 1. Execute JavaScript with unique UUID
 * 2. JavaScript calls console.log(uuid)
 * 3. DevTools captures console output
 * 4. SessionConsoleLogger writes to browser.log
 * 5. Verify UUID appears in browser.log file
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SessionManagerDaemon } from '../../daemons/session-manager/SessionManagerDaemon.js';
import { BrowserManagerDaemon } from '../../daemons/browser-manager/BrowserManagerDaemon.js';
import { JSExecuteCommand } from '../../commands/browser/js-execute/JSExecuteCommand.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('Real Console Capture Integration', () => {
  let sessionManager: SessionManagerDaemon;
  let browserManager: BrowserManagerDaemon;
  let jsExecuteCommand: JSExecuteCommand;
  let tempDir: string;
  let testSessionId: string;
  let browserLogPath: string;

  beforeEach(async () => {
    // Create temporary directory for test sessions
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'real-console-capture-'));
    
    // Initialize daemons
    sessionManager = new SessionManagerDaemon(tempDir);
    browserManager = new BrowserManagerDaemon();
    jsExecuteCommand = new JSExecuteCommand();
    
    await sessionManager.start();
    await browserManager.start();
    
    // Use session connect flow like the real system
    const connectResult = await sessionManager.handleConnect({
      source: 'test-client',
      owner: 'console-capture-validation',
      sessionPreference: 'new',
      capabilities: ['javascript-execution', 'console-capture'],
      context: 'real-console-test',
      type: 'test'
    });
    
    expect(connectResult.success).toBe(true);
    testSessionId = connectResult.data?.sessionId;
    
    // Extract browser log path from connect result (same as user gets)
    browserLogPath = connectResult.data?.logs?.browser;
    expect(browserLogPath).toBeDefined();
    
    console.log(`📝 Using session log from connect: ${browserLogPath}`);
  });

  afterEach(async () => {
    await browserManager.stop();
    await sessionManager.stop();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe('UUID Console Tracking (Git Hook Pattern)', () => {
    it('should execute JavaScript with UUID and capture console.log in browser.log', async () => {
      // Generate unique UUID for this test execution
      const testUUID = `validation-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      console.log(`🎯 Testing UUID: ${testUUID}`);
      
      // Create browser instance (should automatically start console logging)
      const browserResult = await browserManager.handleMessage({
        type: 'browser_request',
        data: {
          type: 'create',
          sessionId: testSessionId,
          options: {
            type: 'default',
            devtools: true
          }
        }
      });
      
      if (!browserResult.success) {
        console.log('❌ Browser creation failed:', browserResult.error);
      }
      expect(browserResult.success).toBe(true);
      const browserData = browserResult.data?.browser;
      expect(browserData?.devToolsUrl).toBeDefined();
      
      console.log(`🌐 Browser created: ${browserData?.id} with DevTools: ${browserData?.devToolsUrl}`);
      
      // Wait for console logging to initialize
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Read initial browser log content
      const initialContent = await fs.readFile(browserLogPath, 'utf-8');
      console.log('📄 Initial browser.log content:', initialContent.split('\n').length, 'lines');
      
      // Execute JavaScript that outputs our UUID to console
      const jsResult = await jsExecuteCommand.execute({
        script: `
          console.log('🎯 VALIDATION_UUID_${testUUID}_START');
          console.log('🔥 Real console output from JavaScript execution!');
          console.log('📊 Timestamp: ' + new Date().toISOString());
          console.log('🎯 VALIDATION_UUID_${testUUID}_COMPLETE');
        `,
        sessionId: testSessionId,
        logExecution: true,
        generateUUID: true
      });
      
      expect(jsResult.success).toBe(true);
      expect(jsResult.data?.executionUUID).toBeDefined();
      
      console.log(`✅ JavaScript executed: ${jsResult.data?.executionUUID}`);
      
      // Wait for console capture to process
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Read final browser log content
      const finalContent = await fs.readFile(browserLogPath, 'utf-8');
      console.log('📄 Final browser.log content:', finalContent.split('\n').length, 'lines');
      console.log('📝 Final content preview:');
      console.log(finalContent.slice(-500)); // Show last 500 chars
      
      // CRITICAL VALIDATION: UUID must appear in browser.log from real console output
      const hasUUIDStart = finalContent.includes(`VALIDATION_UUID_${testUUID}_START`);
      const hasUUIDComplete = finalContent.includes(`VALIDATION_UUID_${testUUID}_COMPLETE`);
      const hasRealConsoleOutput = finalContent.includes('Real console output from JavaScript execution');
      const hasTimestamp = finalContent.includes('Timestamp:');
      
      console.log('\n🧪 Console Capture Validation:');
      console.log(`✅ UUID Start Marker: ${hasUUIDStart}`);
      console.log(`✅ UUID Complete Marker: ${hasUUIDComplete}`);
      console.log(`✅ Real Console Output: ${hasRealConsoleOutput}`);
      console.log(`✅ Timestamp Capture: ${hasTimestamp}`);
      
      // All markers must be present for real console capture to be proven
      expect(hasUUIDStart).toBe(true);
      expect(hasUUIDComplete).toBe(true);
      expect(hasRealConsoleOutput).toBe(true);
      expect(hasTimestamp).toBe(true);
      
      // Additional validation: Content should be formatted like DevTools capture
      const hasDevToolsFormat = finalContent.includes('🌐 [') && finalContent.includes('] LOG:');
      expect(hasDevToolsFormat).toBe(true);
      
      console.log('🎉 REAL CONSOLE CAPTURE VALIDATED - Git hook pattern proven!');
    });

    it('should handle multiple JavaScript executions with unique UUIDs', async () => {
      // Create browser first
      const browserResult = await browserManager.handleMessage({
        type: 'browser_request',
        data: {
          type: 'create',
          sessionId: testSessionId,
          options: { type: 'default', devtools: true }
        }
      });
      
      expect(browserResult.success).toBe(true);
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Execute multiple JavaScript commands with different UUIDs
      const testUUIDs: string[] = [];
      
      for (let i = 0; i < 3; i++) {
        const uuid = `multi-test-${Date.now()}-${i}-${Math.random().toString(36).substr(2, 5)}`;
        testUUIDs.push(uuid);
        
        const result = await jsExecuteCommand.execute({
          script: `console.log('🎯 MULTI_UUID_${uuid}'); console.log('Execution ${i + 1} of 3');`,
          sessionId: testSessionId,
          logExecution: true
        });
        
        expect(result.success).toBe(true);
        
        // Small delay between executions
        await new Promise(resolve => setTimeout(resolve, 500));
      }
      
      // Wait for all console output to be captured
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Verify all UUIDs appear in browser log
      const finalContent = await fs.readFile(browserLogPath, 'utf-8');
      
      for (const uuid of testUUIDs) {
        const hasUUID = finalContent.includes(`MULTI_UUID_${uuid}`);
        expect(hasUUID).toBe(true);
        console.log(`✅ UUID ${uuid} captured in browser.log`);
      }
      
      console.log('🎉 Multiple UUID tracking validated - session logging is accumulative!');
    });

    it('should prove DevTools WebSocket connection is real (not simulated)', async () => {
      // This test specifically validates that we're not using fake/simulated console output
      
      const browserResult = await browserManager.handleMessage({
        type: 'browser_request',
        data: {
          type: 'create',
          sessionId: testSessionId,
          options: { type: 'default', devtools: true }
        }
      });
      
      expect(browserResult.success).toBe(true);
      const devToolsUrl = browserResult.data?.browser?.devToolsUrl;
      expect(devToolsUrl).toBeDefined();
      
      console.log(`🔗 DevTools URL for validation: ${devToolsUrl}`);
      
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Execute JavaScript with timing-sensitive content that would be hard to fake
      const timestamp = Date.now();
      const result = await jsExecuteCommand.execute({
        script: `
          const now = Date.now();
          console.log('🕐 REAL_TIMESTAMP_CHECK: ' + now);
          console.log('🔢 MATH_RANDOM: ' + Math.random());
          console.log('🌍 USER_AGENT: ' + navigator.userAgent);
        `,
        sessionId: testSessionId
      });
      
      expect(result.success).toBe(true);
      
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      const finalContent = await fs.readFile(browserLogPath, 'utf-8');
      
      // These would be nearly impossible to simulate correctly
      const hasTimestampCheck = finalContent.includes('REAL_TIMESTAMP_CHECK:');
      const hasMathRandom = finalContent.includes('MATH_RANDOM:');
      const hasUserAgent = finalContent.includes('USER_AGENT:');
      
      expect(hasTimestampCheck).toBe(true);
      expect(hasMathRandom).toBe(true);
      expect(hasUserAgent).toBe(true);
      
      console.log('✅ Real browser environment data captured - DevTools integration is authentic!');
    });
  });
});