#!/usr/bin/env tsx
/**
 * ResponseCorrelator Unit Tests
 * 
 * Tests the request-response correlation system that preserves Promise chains
 * across cross-context communication.
 */

import { ResponseCorrelator } from '../../shared/ResponseCorrelator';

console.log('🧪 ResponseCorrelator Test Suite');

function testRequestCreationAndResolution() {
  console.log('  📝 Testing request creation and resolution...');
  
  return new Promise<void>((resolve, reject) => {
    const correlator = new ResponseCorrelator(5000);
    const correlationId = correlator.generateCorrelationId();
    const testResponse = { success: true, data: 'test-result' };

    // Create request Promise
    const requestPromise = correlator.createRequest(correlationId);

    // Simulate async response resolution
    setTimeout(() => {
      const resolved = correlator.resolveRequest(correlationId, testResponse);
      if (!resolved) {
        reject(new Error('Failed to resolve request'));
        return;
      }
    }, 10);

    requestPromise.then((response) => {
      if (JSON.stringify(response) !== JSON.stringify(testResponse)) {
        reject(new Error('Response data mismatch'));
        return;
      }
      console.log('  ✅ Request creation and resolution works');
      resolve();
    }).catch(reject);
  });
}

function testRequestTimeout() {
  console.log('  📝 Testing request timeout...');
  
  return new Promise<void>((resolve, reject) => {
    const correlator = new ResponseCorrelator(50); // Very short timeout
    const correlationId = correlator.generateCorrelationId();

    const requestPromise = correlator.createRequest(correlationId);

    requestPromise.then(() => {
      reject(new Error('Request should have timed out'));
    }).catch((error) => {
      if (error.message.includes('timeout')) {
        console.log('  ✅ Request timeout works');
        resolve();
      } else {
        reject(new Error('Wrong error type: ' + error.message));
      }
    });
  });
}

function testMultipleConcurrentRequests() {
  console.log('  📝 Testing multiple concurrent requests...');
  
  return new Promise<void>((resolve, reject) => {
    const correlator = new ResponseCorrelator(5000);
    const requests = [
      { id: correlator.generateCorrelationId(), response: { data: 'response-1' } },
      { id: correlator.generateCorrelationId(), response: { data: 'response-2' } },
      { id: correlator.generateCorrelationId(), response: { data: 'response-3' } }
    ];

    const promises = requests.map(req => correlator.createRequest(req.id));

    // Resolve all requests in random order
    setTimeout(() => correlator.resolveRequest(requests[1].id, requests[1].response), 10);
    setTimeout(() => correlator.resolveRequest(requests[0].id, requests[0].response), 20);
    setTimeout(() => correlator.resolveRequest(requests[2].id, requests[2].response), 15);

    Promise.all(promises).then((responses) => {
      for (let i = 0; i < responses.length; i++) {
        if (JSON.stringify(responses[i]) !== JSON.stringify(requests[i].response)) {
          reject(new Error(`Response ${i} mismatch`));
          return;
        }
      }
      console.log('  ✅ Multiple concurrent requests work');
      resolve();
    }).catch(reject);
  });
}

function testRejectRequest() {
  console.log('  📝 Testing request rejection...');
  
  return new Promise<void>((resolve, reject) => {
    const correlator = new ResponseCorrelator(5000);
    const correlationId = correlator.generateCorrelationId();

    const requestPromise = correlator.createRequest(correlationId);

    setTimeout(() => {
      correlator.rejectRequest(correlationId, new Error('Test rejection'));
    }, 10);

    requestPromise.then(() => {
      reject(new Error('Request should have been rejected'));
    }).catch((error) => {
      if (error.message.includes('Test rejection')) {
        console.log('  ✅ Request rejection works');
        resolve();
      } else {
        reject(new Error('Wrong rejection error: ' + error.message));
      }
    });
  });
}

function testDuplicateResolution() {
  console.log('  📝 Testing duplicate resolution handling...');
  
  return new Promise<void>((resolve, reject) => {
    const correlator = new ResponseCorrelator(5000);
    const correlationId = correlator.generateCorrelationId();
    const testResponse = { data: 'test' };

    const requestPromise = correlator.createRequest(correlationId);

    setTimeout(() => {
      const resolved1 = correlator.resolveRequest(correlationId, testResponse);
      const resolved2 = correlator.resolveRequest(correlationId, testResponse);
      
      if (!resolved1) {
        reject(new Error('First resolution should succeed'));
        return;
      }
      if (resolved2) {
        reject(new Error('Second resolution should fail'));
        return;
      }
    }, 10);

    requestPromise.then((response) => {
      if (JSON.stringify(response) !== JSON.stringify(testResponse)) {
        reject(new Error('Response mismatch'));
        return;
      }
      console.log('  ✅ Duplicate resolution handling works');
      resolve();
    }).catch(reject);
  });
}

// Run all tests
async function runAllTests() {
  try {
    await testRequestCreationAndResolution();
    await testRequestTimeout();
    await testMultipleConcurrentRequests();
    await testRejectRequest();
    await testDuplicateResolution();
    
    console.log('✅ All ResponseCorrelator tests passed!');
    process.exit(0);
  } catch (error) {
    console.error('❌ ResponseCorrelator test failed:', error);
    process.exit(1);
  }
}

runAllTests();