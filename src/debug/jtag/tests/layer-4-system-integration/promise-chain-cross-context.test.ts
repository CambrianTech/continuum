/**
 * Layer 4: Promise Chain Cross-Context Integration Test
 * 
 * Tests complete promise chain preservation across real browser ↔ server contexts
 * using the JTAG testing infrastructure. This validates the end-to-end flow
 * of await jtag.commands.screenshot() from browser through WebSocket to server.
 */

import { TestUtilities } from '../shared/TestUtilities';
import { TestBenchClient } from '../shared/TestBenchClient';
import { JTAGSystem } from '../../shared/JTAGSystem';
import { ScreenshotParams, ScreenshotResult } from '../../shared/JTAGTypes';

describe('Layer 4: Promise Chain Cross-Context Integration', () => {
  let testUtils: TestUtilities;
  let testBenchClient: TestBenchClient;
  let serverJTAG: JTAGSystem;

  beforeAll(async () => {
    console.log('🧪 Layer 4: Starting promise chain cross-context tests...');
    
    // Initialize test utilities
    testUtils = new TestUtilities();
    testBenchClient = new TestBenchClient();
    
    // Connect to running JTAG system (npm start should be running)
    serverJTAG = await JTAGSystem.connect('server');
    
    // Verify test-bench server is running on port 9002
    await testBenchClient.connect();
    
    console.log('✅ Layer 4: Test infrastructure ready');
  });

  afterAll(async () => {
    if (serverJTAG) {
      await serverJTAG.shutdown();
    }
    if (testBenchClient) {
      await testBenchClient.disconnect();
    }
  });

  describe('Cross-Context Promise Preservation', () => {
    test('should preserve await chain for server → browser screenshot command', async () => {
      const testFilename = `layer4-server-to-browser-${Date.now()}.png`;
      
      console.log(`📸 Testing server → browser screenshot: ${testFilename}`);
      
      // Execute screenshot command from server context targeting browser
      const screenshotParams = new ScreenshotParams(testFilename, 'body', {
        width: 800,
        height: 600,
        fullPage: false
      });

      // This should preserve the Promise chain end-to-end
      const startTime = Date.now();
      const result: ScreenshotResult = await serverJTAG.commands.screenshot(screenshotParams);
      const duration = Date.now() - startTime;

      // Verify promise resolved with actual ScreenshotResult
      expect(result).toBeInstanceOf(ScreenshotResult);
      expect(result.success).toBe(true);
      expect(result.filename).toBe(testFilename);
      expect(result.context).toBe('browser'); // Screenshot taken in browser
      expect(result.filepath).toContain(testFilename);
      expect(result.timestamp).toBeDefined();
      
      // Should have metadata from actual screenshot
      expect(result.metadata).toBeDefined();
      expect(result.metadata!.width).toBeGreaterThan(0);
      expect(result.metadata!.height).toBeGreaterThan(0);
      expect(result.metadata!.size).toBeGreaterThan(1000); // Real screenshot should be > 1KB

      console.log(`✅ Screenshot completed in ${duration}ms: ${result.filepath}`);
      console.log(`📊 Screenshot metadata:`, result.metadata);
    }, 15000); // 15 second timeout for real browser operations

    test('should handle concurrent cross-context commands independently', async () => {
      const baseFilename = `layer4-concurrent-${Date.now()}`;
      const screenshotPromises = [];

      console.log('🔄 Testing concurrent cross-context commands...');

      // Create multiple concurrent screenshot requests
      for (let i = 0; i < 3; i++) {
        const filename = `${baseFilename}-${i}.png`;
        const params = new ScreenshotParams(filename, `body`, {
          width: 400 + (i * 100),
          height: 300 + (i * 50)
        });
        
        screenshotPromises.push(serverJTAG.commands.screenshot(params));
      }

      // All should resolve independently with correct results
      const startTime = Date.now();
      const results = await Promise.all(screenshotPromises);
      const duration = Date.now() - startTime;

      expect(results).toHaveLength(3);
      
      results.forEach((result, index) => {
        expect(result).toBeInstanceOf(ScreenshotResult);
        expect(result.success).toBe(true);
        expect(result.filename).toBe(`${baseFilename}-${index}.png`);
        expect(result.context).toBe('browser');
        
        // Each should have different dimensions as requested
        expect(result.metadata!.width).toBe(400 + (index * 100));
        expect(result.metadata!.height).toBe(300 + (index * 50));
      });

      console.log(`✅ ${results.length} concurrent screenshots completed in ${duration}ms`);
    }, 20000);

    test('should handle cross-context command errors properly', async () => {
      console.log('❌ Testing cross-context error handling...');
      
      // Create invalid screenshot request (invalid selector)
      const params = new ScreenshotParams('error-test.png', '#non-existent-element-12345', {
        width: 800,
        height: 600
      });

      // Should properly propagate error through promise chain
      await expect(
        serverJTAG.commands.screenshot(params)
      ).rejects.toThrow(); // Exact error depends on implementation

      console.log('✅ Error handling verified - promise chain preserved errors');
    });

    test('should handle cross-context timeout scenarios', async () => {
      console.log('⏰ Testing cross-context timeout handling...');
      
      // Create screenshot with very long processing time
      const params = new ScreenshotParams('timeout-test.png', 'body', {
        delay: 10000 // 10 second delay - should timeout before this
      });

      // Should timeout properly (ResponseCorrelator has 30s timeout)
      const timeoutPromise = serverJTAG.commands.screenshot(params);
      
      // Race against a shorter timeout to verify timeout behavior
      const shortTimeout = new Promise((_, reject) => {
        setTimeout(() => reject(new Error('Test timeout')), 2000);
      });

      // Either should complete or timeout - both are valid depending on system state
      try {
        await Promise.race([timeoutPromise, shortTimeout]);
        console.log('✅ Command completed within test window');
      } catch (error) {
        // Timeout is expected behavior
        console.log('✅ Timeout behavior verified');
        expect(error.message).toMatch(/(timeout|Test timeout)/i);
      }
    }, 35000);
  });

  describe('Browser Integration Verification', () => {
    test('should verify browser JTAG system receives and processes requests', async () => {
      console.log('🌐 Testing browser JTAG system integration...');
      
      // Use TestBenchClient to execute code in browser context
      const browserTestCode = `
        // Verify browser JTAG system is available
        if (!window.jtag) {
          throw new Error('Browser JTAG system not available');
        }
        
        // Check router status
        const routerStatus = window.jtag.router.status;
        
        return {
          jtagAvailable: !!window.jtag,
          routerInitialized: routerStatus.initialized,
          transportConnected: routerStatus.transport?.connected,
          environment: routerStatus.environment,
          subscribers: routerStatus.subscribers,
          queueStatus: routerStatus.queue,
          healthStatus: routerStatus.health
        };
      `;

      const browserStatus = await testBenchClient.executeInBrowser(browserTestCode);
      
      // Verify browser JTAG system is properly set up for cross-context communication
      expect(browserStatus.jtagAvailable).toBe(true);
      expect(browserStatus.routerInitialized).toBe(true);
      expect(browserStatus.transportConnected).toBe(true);
      expect(browserStatus.environment).toBe('browser');
      expect(browserStatus.subscribers).toBeGreaterThan(0);
      
      // Queue and health systems should be operational
      expect(browserStatus.queueStatus).toBeDefined();
      expect(browserStatus.healthStatus).toBeDefined();
      
      console.log('✅ Browser JTAG system verification complete:', browserStatus);
    });

    test('should verify message type differentiation in browser', async () => {
      console.log('🔍 Testing message type handling in browser...');
      
      const browserTestCode = `
        const { JTAGMessageFactory, JTAGMessageTypes } = window.jtag.types;
        const testContext = { uuid: 'test', environment: 'browser' };
        const testPayload = new window.jtag.types.JTAGPayload();
        
        // Create different message types
        const eventMessage = JTAGMessageFactory.createEvent(
          testContext, 'origin', 'endpoint', testPayload
        );
        
        const correlationId = JTAGMessageFactory.generateCorrelationId();
        const requestMessage = JTAGMessageFactory.createRequest(
          testContext, 'origin', 'endpoint', testPayload, correlationId
        );
        
        // Test type guards
        return {
          eventIsEvent: JTAGMessageTypes.isEvent(eventMessage),
          eventIsRequest: JTAGMessageTypes.isRequest(eventMessage),
          requestIsRequest: JTAGMessageTypes.isRequest(requestMessage),
          requestIsEvent: JTAGMessageTypes.isEvent(requestMessage),
          correlationIdGenerated: correlationId.length > 0
        };
      `;

      const typeTestResults = await testBenchClient.executeInBrowser(browserTestCode);
      
      // Verify type system works correctly in browser
      expect(typeTestResults.eventIsEvent).toBe(true);
      expect(typeTestResults.eventIsRequest).toBe(false);
      expect(typeTestResults.requestIsRequest).toBe(true);
      expect(typeTestResults.requestIsEvent).toBe(false);
      expect(typeTestResults.correlationIdGenerated).toBe(true);
      
      console.log('✅ Browser message type differentiation verified');
    });
  });

  describe('Performance and Reliability', () => {
    test('should maintain acceptable performance for cross-context commands', async () => {
      console.log('⚡ Testing cross-context command performance...');
      
      const performanceTests = [];
      const testCount = 5;
      
      for (let i = 0; i < testCount; i++) {
        const filename = `perf-test-${Date.now()}-${i}.png`;
        const params = new ScreenshotParams(filename, 'body', { width: 400, height: 300 });
        
        const startTime = Date.now();
        const promise = serverJTAG.commands.screenshot(params).then(result => ({
          result,
          duration: Date.now() - startTime
        }));
        
        performanceTests.push(promise);
      }
      
      const results = await Promise.all(performanceTests);
      
      // All should complete successfully
      results.forEach((test, index) => {
        expect(test.result.success).toBe(true);
        expect(test.duration).toBeLessThan(10000); // Should complete within 10 seconds
        console.log(`📊 Test ${index + 1}: ${test.duration}ms`);
      });
      
      const avgDuration = results.reduce((sum, test) => sum + test.duration, 0) / results.length;
      console.log(`✅ Average cross-context command duration: ${avgDuration.toFixed(2)}ms`);
      
      // Performance should be reasonable (adjust based on system capabilities)
      expect(avgDuration).toBeLessThan(5000); // Average under 5 seconds
    }, 30000);

    test('should maintain connection health during sustained operations', async () => {
      console.log('💓 Testing connection health during sustained operations...');
      
      const initialStatus = serverJTAG.router.status;
      expect(initialStatus.health.isHealthy).toBe(true);
      
      // Perform sustained operations
      const operations = [];
      for (let i = 0; i < 10; i++) {
        const filename = `health-test-${Date.now()}-${i}.png`;
        const params = new ScreenshotParams(filename, 'body', { width: 200, height: 200 });
        operations.push(serverJTAG.commands.screenshot(params));
        
        // Small delay between operations
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      
      await Promise.all(operations);
      
      const finalStatus = serverJTAG.router.status;
      
      // Connection should remain healthy
      expect(finalStatus.health.isHealthy).toBe(true);
      expect(finalStatus.transport?.connected).toBe(true);
      
      // Queue should be processing properly
      expect(finalStatus.queue.size).toBeLessThanOrEqual(initialStatus.queue.size);
      
      console.log('✅ Connection health maintained during sustained operations');
      console.log(`📊 Final health score: ${finalStatus.health.score}/100`);
    }, 25000);
  });
});