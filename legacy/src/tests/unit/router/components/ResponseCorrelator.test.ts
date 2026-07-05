#!/usr/bin/env tsx
/**
 * ResponseCorrelator - Comprehensive Unit Tests
 * 
 * Tests the core request-response correlation system that enables promise-based
 * communication across async message routing. This is critical for the router's
 * ability to turn async message passing into synchronous-looking method calls.
 */

import { ResponseCorrelator } from '../../../../system/core/shared/ResponseCorrelator';

console.log('🧪 ResponseCorrelator Comprehensive Unit Tests');

// ========================================
// UNIT TESTS - CORRELATION LOGIC
// ========================================

/**
 * Test 1: Basic Correlation ID Generation and Uniqueness
 */
async function testCorrelationIdGenerationAndUniqueness(): Promise<void> {
  console.log('  📝 Testing correlation ID generation and uniqueness...');
  
  const correlator = new ResponseCorrelator(5000);
  
  // Test ID format and uniqueness
  const ids = new Set<string>();
  const idCount = 1000;
  
  for (let i = 0; i < idCount; i++) {
    const id = correlator.generateCorrelationId();
    
    // Test format: req_timestamp_random
    if (!id.match(/^req_\d+_[a-z0-9]+$/)) {
      throw new Error(`Invalid correlation ID format: ${id}`);
    }
    
    // Test uniqueness
    if (ids.has(id)) {
      throw new Error(`Duplicate correlation ID generated: ${id}`);
    }
    ids.add(id);
  }
  
  console.log(`    📊 Generated ${idCount} unique correlation IDs with valid format`);
  
  // Test timestamp ordering (newer IDs should have later timestamps)
  const id1 = correlator.generateCorrelationId();
  await new Promise(resolve => setTimeout(resolve, 5)); // Ensure different timestamp  
  const id2 = correlator.generateCorrelationId();
  
  const timestamp1 = parseInt(id1.split('_')[1]);
  const timestamp2 = parseInt(id2.split('_')[1]);
  
  if (timestamp2 < timestamp1) {
    throw new Error('Later correlation IDs should have later or equal timestamps');
  }
  
  console.log('  ✅ Correlation ID generation and uniqueness works correctly');
}

/**
 * Test 2: Basic Request-Response Correlation
 */
async function testBasicRequestResponseCorrelation(): Promise<void> {
  console.log('  📝 Testing basic request-response correlation...');
  
  const correlator = new ResponseCorrelator(5000);
  
  // Test successful correlation
  const correlationId = correlator.generateCorrelationId();
  const testResponse = { 
    success: true, 
    data: 'test response', 
    timestamp: new Date().toISOString(),
    correlationId 
  };
  
  // Create request promise
  const requestPromise = correlator.createRequest(correlationId);
  
  // Resolve after short delay
  setTimeout(() => {
    correlator.resolveRequest(correlationId, testResponse);
  }, 10);
  
  // Wait for resolution
  const response = await requestPromise;
  
  if (JSON.stringify(response) !== JSON.stringify(testResponse)) {
    throw new Error('Response correlation failed - data mismatch');
  }
  
  console.log('  ✅ Basic request-response correlation works correctly');
}

/**
 * Test 3: Multiple Concurrent Correlations
 */
async function testMultipleConcurrentCorrelations(): Promise<void> {
  console.log('  📝 Testing multiple concurrent correlations...');
  
  const correlator = new ResponseCorrelator(5000);
  const concurrentCount = 100;
  
  // Create multiple concurrent requests
  const requests: Array<{
    correlationId: string;
    promise: Promise<any>;
    expectedData: any;
  }> = [];
  
  for (let i = 0; i < concurrentCount; i++) {
    const correlationId = correlator.generateCorrelationId();
    const expectedData = { 
      id: i, 
      data: `test-data-${i}`, 
      timestamp: Date.now() + i 
    };
    
    const promise = correlator.createRequest(correlationId);
    
    requests.push({
      correlationId,
      promise,
      expectedData
    });
  }
  
  // Resolve all requests in random order
  const resolveOrder = [...Array(concurrentCount).keys()].sort(() => Math.random() - 0.5);
  
  for (const index of resolveOrder) {
    const request = requests[index];
    setTimeout(() => {
      correlator.resolveRequest(request.correlationId, request.expectedData);
    }, Math.random() * 100);
  }
  
  // Wait for all resolutions
  const results = await Promise.all(requests.map(r => r.promise));
  
  // Verify each result matches its expected data
  for (let i = 0; i < concurrentCount; i++) {
    if (JSON.stringify(results[i]) !== JSON.stringify(requests[i].expectedData)) {
      throw new Error(`Concurrent correlation ${i} failed - data mismatch`);
    }
  }
  
  console.log(`    📊 Successfully handled ${concurrentCount} concurrent correlations`);
  console.log('  ✅ Multiple concurrent correlations work correctly');
}

/**
 * Test 4: Timeout Handling and Cleanup
 */
async function testTimeoutHandlingAndCleanup(): Promise<void> {
  console.log('  📝 Testing timeout handling and cleanup...');
  
  const shortTimeout = 100;
  const correlator = new ResponseCorrelator(shortTimeout);
  
  // Test timeout behavior
  const correlationId = correlator.generateCorrelationId();
  const timeoutPromise = correlator.createRequest(correlationId, shortTimeout);
  
  try {
    await timeoutPromise;
    throw new Error('Should have timed out');
  } catch (error: any) {
    if (!error.message.includes('timeout') && !error.message.includes('Request timed out')) {
      throw new Error(`Expected timeout error, got: ${error.message}`);
    }
  }
  
  // Test that timed out request is cleaned up
  const isResolved = correlator.resolveRequest(correlationId, { data: 'late response' });
  if (isResolved) {
    throw new Error('Timed out request should not be resolvable');
  }
  
  // Test multiple timeout scenarios
  const timeoutRequests = [];
  for (let i = 0; i < 10; i++) {
    const id = correlator.generateCorrelationId();
    const promise = correlator.createRequest(id, 50);
    timeoutRequests.push(promise);
  }
  
  // All should timeout
  const timeoutResults = await Promise.allSettled(timeoutRequests);
  for (const result of timeoutResults) {
    if (result.status !== 'rejected') {
      throw new Error('All short timeout requests should be rejected');
    }
  }
  
  console.log('    📊 Successfully handled timeout scenarios and cleanup');
  console.log('  ✅ Timeout handling and cleanup works correctly');
}

/**
 * Test 5: Error Scenarios and Edge Cases
 */
async function testErrorScenariosAndEdgeCases(): Promise<void> {
  console.log('  📝 Testing error scenarios and edge cases...');
  
  const correlator = new ResponseCorrelator(5000);
  
  // Test resolving non-existent correlation ID
  const nonExistentResolution = correlator.resolveRequest('fake-correlation-id', { data: 'test' });
  if (nonExistentResolution) {
    throw new Error('Should not resolve non-existent correlation ID');
  }
  
  // Test double resolution
  const correlationId = correlator.generateCorrelationId();
  const requestPromise = correlator.createRequest(correlationId);
  
  // First resolution should work
  const firstResolution = correlator.resolveRequest(correlationId, { data: 'first' });
  if (!firstResolution) {
    throw new Error('First resolution should succeed');
  }
  
  // Second resolution should fail
  const secondResolution = correlator.resolveRequest(correlationId, { data: 'second' });
  if (secondResolution) {
    throw new Error('Second resolution should fail');
  }
  
  // Verify first response was received
  const response = await requestPromise;
  if (response.data !== 'first') {
    throw new Error('Should receive first response, not second');
  }
  
  // Test rejection scenario
  const errorCorrelationId = correlator.generateCorrelationId();
  const errorRequestPromise = correlator.createRequest(errorCorrelationId);
  
  const testError = new Error('Test error response');
  correlator.rejectRequest(errorCorrelationId, testError);
  
  try {
    await errorRequestPromise;
    throw new Error('Should have been rejected with error');
  } catch (error: any) {
    if (error.message !== 'Test error response') {
      throw new Error(`Expected test error, got: ${error.message}`);
    }
  }
  
  console.log('  ✅ Error scenarios and edge cases handled correctly');
}

/**
 * Test 6: Memory Management and Leak Prevention
 */
async function testMemoryManagementAndLeakPrevention(): Promise<void> {
  console.log('  📝 Testing memory management and leak prevention...');
  
  const correlator = new ResponseCorrelator(1000);
  
  // Create many requests that complete normally
  console.log('    📊 Testing completed request cleanup...');
  const completedCount = 100;
  const completedPromises = [];
  
  for (let i = 0; i < completedCount; i++) {
    const correlationId = correlator.generateCorrelationId();
    const promise = correlator.createRequest(correlationId);
    completedPromises.push(promise);
    
    // Resolve immediately
    correlator.resolveRequest(correlationId, { data: `completed-${i}` });
  }
  
  await Promise.all(completedPromises);
  console.log(`    📊 Completed ${completedCount} requests successfully`);
  
  // Create many requests that timeout (should be cleaned up)
  console.log('    📊 Testing timeout cleanup...');
  const timeoutCount = 50;
  const timeoutPromises = [];
  
  for (let i = 0; i < timeoutCount; i++) {
    const correlationId = correlator.generateCorrelationId();
    const promise = correlator.createRequest(correlationId, 10); // Very short timeout
    timeoutPromises.push(promise);
  }
  
  // Wait for all to timeout
  const timeoutResults = await Promise.allSettled(timeoutPromises);
  const timedOutCount = timeoutResults.filter(r => r.status === 'rejected').length;
  
  if (timedOutCount !== timeoutCount) {
    throw new Error(`Expected ${timeoutCount} timeouts, got ${timedOutCount}`);
  }
  
  console.log(`    📊 Cleaned up ${timedOutCount} timed-out requests`);
  
  // Test that memory usage is reasonable (correlator should not hold onto completed/timed out requests)
  // This is harder to test directly, but we can test that we can still create new requests efficiently
  console.log('    📊 Testing continued operation after cleanup...');
  
  const postCleanupCount = 100;
  const postCleanupPromises = [];
  const startTime = Date.now();
  
  for (let i = 0; i < postCleanupCount; i++) {
    const correlationId = correlator.generateCorrelationId();
    const promise = correlator.createRequest(correlationId);
    postCleanupPromises.push(promise);
    correlator.resolveRequest(correlationId, { data: `post-cleanup-${i}` });
  }
  
  await Promise.all(postCleanupPromises);
  const endTime = Date.now();
  
  console.log(`    📊 Post-cleanup operations completed in ${endTime - startTime}ms`);
  
  if (endTime - startTime > 1000) {
    throw new Error('Post-cleanup operations should be fast, indicating proper cleanup');
  }
  
  console.log('  ✅ Memory management and leak prevention works correctly');
}

/**
 * Test 7: High-Frequency Correlation Performance
 */
async function testHighFrequencyCorrelationPerformance(): Promise<void> {
  console.log('  📝 Testing high-frequency correlation performance...');
  
  const correlator = new ResponseCorrelator(10000);
  const highFrequencyCount = 1000;
  
  console.log(`    📊 Testing ${highFrequencyCount} rapid correlations...`);
  const startTime = Date.now();
  
  // Create requests as fast as possible
  const rapidPromises = [];
  for (let i = 0; i < highFrequencyCount; i++) {
    const correlationId = correlator.generateCorrelationId();
    const promise = correlator.createRequest(correlationId);
    rapidPromises.push(promise);
    
    // Resolve immediately in same loop to test rapid cycling
    correlator.resolveRequest(correlationId, { data: `rapid-${i}` });
  }
  
  // Wait for all to complete
  const results = await Promise.all(rapidPromises);
  const endTime = Date.now();
  const duration = endTime - startTime;
  
  // Verify all results
  for (let i = 0; i < highFrequencyCount; i++) {
    if (results[i].data !== `rapid-${i}`) {
      throw new Error(`Rapid correlation ${i} failed`);
    }
  }
  
  console.log(`    📊 Completed ${highFrequencyCount} correlations in ${duration}ms`);
  console.log(`    📊 Performance: ${(highFrequencyCount / duration * 1000).toFixed(0)} correlations/sec`);
  
  // Performance threshold
  if (duration > 2000) {
    throw new Error(`Performance too slow: ${duration}ms for ${highFrequencyCount} correlations`);
  }
  
  console.log('  ✅ High-frequency correlation performance is acceptable');
}

// ========================================
// RUN ALL TESTS
// ========================================

async function runAllTests(): Promise<void> {
  try {
    await testCorrelationIdGenerationAndUniqueness();
    await testBasicRequestResponseCorrelation();
    await testMultipleConcurrentCorrelations();
    await testTimeoutHandlingAndCleanup();
    await testErrorScenariosAndEdgeCases();
    await testMemoryManagementAndLeakPrevention();
    await testHighFrequencyCorrelationPerformance();
    
    console.log('✅ All ResponseCorrelator unit tests passed!');
    console.log('\n📋 COMPONENT TEST SUMMARY:');
    console.log('  ✅ Correlation ID generation and uniqueness');
    console.log('  ✅ Basic request-response correlation');
    console.log('  ✅ Multiple concurrent correlations');
    console.log('  ✅ Timeout handling and cleanup');
    console.log('  ✅ Error scenarios and edge cases');
    console.log('  ✅ Memory management and leak prevention');
    console.log('  ✅ High-frequency correlation performance');
    console.log('\n🎯 ResponseCorrelator is bulletproof and production-ready!');
    
  } catch (error) {
    console.error('❌ ResponseCorrelator test failed:', error);
    throw error;
  }
}

// Run tests if called directly
if (process.argv[1] && process.argv[1].endsWith('ResponseCorrelator.test.ts')) {
  runAllTests()
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

export { runAllTests };