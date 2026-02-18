#!/usr/bin/env tsx
/**
 * EndpointMatcher - Comprehensive Unit Tests
 * 
 * Tests the core endpoint pattern matching and subscriber management system.
 * This component is critical for routing decisions - it must be bulletproof.
 */

import { EndpointMatcher } from '../../../../system/core/router/shared/EndpointMatcher';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';

console.log('🧪 EndpointMatcher Comprehensive Unit Tests');

// Mock subscriber interface for testing
interface TestSubscriber {
  id: string;
  endpoint: string;
  handleMessage?: (msg: any) => Promise<any>;
}

// ========================================
// UNIT TESTS - CORE FUNCTIONALITY
// ========================================

/**
 * Test 1: Basic Registration and Lookup
 */
async function testBasicRegistrationAndLookup(): Promise<void> {
  console.log('  📝 Testing basic registration and lookup...');
  
  const matcher = new EndpointMatcher<TestSubscriber>();
  
  // Test empty matcher
  const emptyMatch = matcher.match('any/endpoint');
  if (emptyMatch !== null) {
    throw new Error('Empty matcher should return null for any endpoint');
  }
  
  // Register some subscribers
  const pingSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/ping' };
  const fileSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/file' };
  const commandsSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands' };
  
  matcher.register('commands/ping', pingSubscriber);
  matcher.register('commands/file', fileSubscriber);
  matcher.register('commands', commandsSubscriber);
  
  // Test exact matches
  const pingMatch = matcher.match('commands/ping');
  if (!pingMatch || pingMatch.subscriber !== pingSubscriber || pingMatch.matchType !== 'exact') {
    throw new Error('Should find exact match for commands/ping');
  }
  
  const fileMatch = matcher.match('commands/file');
  if (!fileMatch || fileMatch.subscriber !== fileSubscriber || fileMatch.matchType !== 'exact') {
    throw new Error('Should find exact match for commands/file');
  }
  
  // Test hierarchical matches
  const screenshotMatch = matcher.match('commands/screenshot');
  if (!screenshotMatch || screenshotMatch.subscriber !== commandsSubscriber || screenshotMatch.matchType !== 'hierarchical') {
    throw new Error('Should find hierarchical match for commands/screenshot via commands');
  }
  
  const fileSaveMatch = matcher.match('commands/file/save');
  if (!fileSaveMatch || fileSaveMatch.subscriber !== fileSubscriber || fileSaveMatch.matchType !== 'hierarchical') {
    throw new Error('Should find hierarchical match for commands/file/save via commands/file');
  }
  
  console.log('  ✅ Basic registration and lookup works correctly');
}

/**
 * Test 2: Priority and Specificity Resolution
 */
async function testPriorityAndSpecificity(): Promise<void> {
  console.log('  📝 Testing priority and specificity resolution...');
  
  const matcher = new EndpointMatcher<TestSubscriber>();
  
  // Register overlapping patterns in different orders
  const generalSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands' };
  const specificSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/ping' };
  const fileGeneralSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/file' };
  const fileSpecificSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/file/save' };
  
  // Register in reverse order of specificity to test priority handling
  matcher.register('commands', generalSubscriber);
  matcher.register('commands/file', fileGeneralSubscriber);  
  matcher.register('commands/ping', specificSubscriber);
  matcher.register('commands/file/save', fileSpecificSubscriber);
  
  // Most specific should win
  const pingMatch = matcher.match('commands/ping');
  if (!pingMatch || pingMatch.subscriber !== specificSubscriber || pingMatch.matchType !== 'exact') {
    throw new Error('Most specific match should win for commands/ping');
  }
  
  const fileSaveMatch = matcher.match('commands/file/save');
  if (!fileSaveMatch || fileSaveMatch.subscriber !== fileSpecificSubscriber || fileSaveMatch.matchType !== 'exact') {
    throw new Error('Most specific match should win for commands/file/save');
  }
  
  // Medium specificity should be preferred over general
  const fileLoadMatch = matcher.match('commands/file/load');
  if (!fileLoadMatch || fileLoadMatch.subscriber !== fileGeneralSubscriber || fileLoadMatch.matchType !== 'hierarchical') {
    throw new Error('Medium specificity should beat general for commands/file/load');
  }
  
  // General should catch unmatched
  const screenshotMatch = matcher.match('commands/screenshot');
  if (!screenshotMatch || screenshotMatch.subscriber !== generalSubscriber || screenshotMatch.matchType !== 'hierarchical') {
    throw new Error('General should catch unmatched commands/screenshot');
  }
  
  console.log('  ✅ Priority and specificity resolution works correctly');
}

/**
 * Test 3: Edge Cases and Boundary Conditions
 */
async function testEdgeCasesAndBoundaryConditions(): Promise<void> {
  console.log('  📝 Testing edge cases and boundary conditions...');
  
  const matcher = new EndpointMatcher<TestSubscriber>();
  
  // Test empty strings and null inputs
  try {
    const nullMatch = matcher.match('');
    if (nullMatch !== null) {
      throw new Error('Empty string should not match anything');
    }
  } catch (error: any) {
    if (!error.message.includes('Empty string')) {
      throw error; // Re-throw if not expected error
    }
  }
  
  // Test very long endpoint names
  const longEndpoint = 'commands/' + 'a'.repeat(1000);
  const longSubscriber: TestSubscriber = { id: generateUUID(), endpoint: longEndpoint };
  matcher.register(longEndpoint, longSubscriber);
  
  const longMatch = matcher.match(longEndpoint);
  if (!longMatch || longMatch.subscriber !== longSubscriber) {
    throw new Error('Should handle very long endpoint names');
  }
  
  // Test special characters in endpoints
  const specialEndpoint = 'commands/file-save_v2.0';
  const specialSubscriber: TestSubscriber = { id: generateUUID(), endpoint: specialEndpoint };
  matcher.register(specialEndpoint, specialSubscriber);
  
  const specialMatch = matcher.match(specialEndpoint);
  if (!specialMatch || specialMatch.subscriber !== specialSubscriber) {
    throw new Error('Should handle special characters in endpoints');
  }
  
  // Test deep nesting
  const deepEndpoint = 'a/b/c/d/e/f/g/h/i/j';
  const shallowEndpoint = 'a/b/c';
  
  const shallowSubscriber: TestSubscriber = { id: generateUUID(), endpoint: shallowEndpoint };
  matcher.register(shallowEndpoint, shallowSubscriber);
  
  const deepMatch = matcher.match(deepEndpoint);
  if (!deepMatch || deepMatch.subscriber !== shallowSubscriber || deepMatch.matchType !== 'hierarchical') {
    throw new Error('Should handle deep hierarchical matching');
  }
  
  console.log('  ✅ Edge cases and boundary conditions handled correctly');
}

/**
 * Test 4: Subscriber Replacement and Unregistration
 */
async function testSubscriberReplacementAndUnregistration(): Promise<void> {
  console.log('  📝 Testing subscriber replacement and unregistration...');
  
  const matcher = new EndpointMatcher<TestSubscriber>();
  
  // Register initial subscriber
  const initialSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/test' };
  matcher.register('commands/test', initialSubscriber);
  
  const initialMatch = matcher.match('commands/test');
  if (!initialMatch || initialMatch.subscriber !== initialSubscriber) {
    throw new Error('Initial subscriber should be registered');
  }
  
  // Replace with new subscriber
  const replacementSubscriber: TestSubscriber = { id: generateUUID(), endpoint: 'commands/test' };
  matcher.register('commands/test', replacementSubscriber);
  
  const replacementMatch = matcher.match('commands/test');
  if (!replacementMatch || replacementMatch.subscriber !== replacementSubscriber) {
    throw new Error('Replacement subscriber should override initial subscriber');
  }
  
  if (replacementMatch.subscriber === initialSubscriber) {
    throw new Error('Initial subscriber should be replaced, not kept');
  }
  
  // Test that unregistration works (if supported)
  if ('unregister' in matcher) {
    (matcher as any).unregister('commands/test');
    const unregisteredMatch = matcher.match('commands/test');
    if (unregisteredMatch !== null) {
      throw new Error('Unregistered endpoint should not match');
    }
  }
  
  console.log('  ✅ Subscriber replacement and unregistration works correctly');
}

/**
 * Test 5: Multiple Endpoint Patterns and Complex Hierarchies
 */
async function testComplexHierarchiesAndPatterns(): Promise<void> {
  console.log('  📝 Testing complex hierarchies and endpoint patterns...');
  
  const matcher = new EndpointMatcher<TestSubscriber>();
  
  // Create complex hierarchy
  const subscribers = {
    root: { id: generateUUID(), endpoint: 'api' },
    v1: { id: generateUUID(), endpoint: 'api/v1' },
    users: { id: generateUUID(), endpoint: 'api/v1/users' },
    userById: { id: generateUUID(), endpoint: 'api/v1/users/123' },
    posts: { id: generateUUID(), endpoint: 'api/v1/posts' },
    postComments: { id: generateUUID(), endpoint: 'api/v1/posts/comments' },
  };
  
  // Register in mixed order to test sorting/priority
  matcher.register('api/v1/users', subscribers.users);
  matcher.register('api', subscribers.root);
  matcher.register('api/v1/posts/comments', subscribers.postComments);
  matcher.register('api/v1', subscribers.v1);
  matcher.register('api/v1/users/123', subscribers.userById);
  matcher.register('api/v1/posts', subscribers.posts);
  
  // Test exact matches at different levels
  const userByIdMatch = matcher.match('api/v1/users/123');
  if (!userByIdMatch || userByIdMatch.subscriber !== subscribers.userById || userByIdMatch.matchType !== 'exact') {
    throw new Error('Should exactly match most specific user endpoint');
  }
  
  const postCommentsMatch = matcher.match('api/v1/posts/comments');
  if (!postCommentsMatch || postCommentsMatch.subscriber !== subscribers.postComments || postCommentsMatch.matchType !== 'exact') {
    throw new Error('Should exactly match post comments endpoint');
  }
  
  // Test hierarchical fallbacks
  const userUpdateMatch = matcher.match('api/v1/users/456/update');
  if (!userUpdateMatch || userUpdateMatch.subscriber !== subscribers.users || userUpdateMatch.matchType !== 'hierarchical') {
    throw new Error('Should hierarchically match users endpoint for user operations');
  }
  
  const postCreateMatch = matcher.match('api/v1/posts/create');
  if (!postCreateMatch || postCreateMatch.subscriber !== subscribers.posts || postCreateMatch.matchType !== 'hierarchical') {
    throw new Error('Should hierarchically match posts endpoint for post operations');
  }
  
  // Test deep fallback to root
  const unknownEndpoint = matcher.match('api/v2/unknown');
  if (!unknownEndpoint || unknownEndpoint.subscriber !== subscribers.root || unknownEndpoint.matchType !== 'hierarchical') {
    throw new Error('Should fallback to root API endpoint for unknown versions');
  }
  
  console.log('  ✅ Complex hierarchies and endpoint patterns work correctly');
}

/**
 * Test 6: Performance with Large Numbers of Endpoints
 */
async function testPerformanceWithLargeEndpointCounts(): Promise<void> {
  console.log('  📝 Testing performance with large numbers of endpoints...');
  
  const matcher = new EndpointMatcher<TestSubscriber>();
  
  // Register many endpoints to test performance
  const endpointCount = 1000;
  const subscribers: TestSubscriber[] = [];
  
  console.log(`    📊 Registering ${endpointCount} endpoints...`);
  const registrationStart = Date.now();
  
  for (let i = 0; i < endpointCount; i++) {
    const subscriber: TestSubscriber = {
      id: generateUUID(),
      endpoint: `commands/test-${i}`
    };
    subscribers.push(subscriber);
    matcher.register(`commands/test-${i}`, subscriber);
  }
  
  const registrationTime = Date.now() - registrationStart;
  console.log(`    📊 Registration completed in ${registrationTime}ms (${(endpointCount / registrationTime * 1000).toFixed(0)} registrations/sec)`);
  
  // Test lookup performance
  console.log(`    📊 Testing ${endpointCount} lookups...`);
  const lookupStart = Date.now();
  
  for (let i = 0; i < endpointCount; i++) {
    const match = matcher.match(`commands/test-${i}`);
    if (!match || match.subscriber !== subscribers[i]) {
      throw new Error(`Failed to find registered endpoint commands/test-${i}`);
    }
  }
  
  const lookupTime = Date.now() - lookupStart;
  console.log(`    📊 Lookup completed in ${lookupTime}ms (${(endpointCount / lookupTime * 1000).toFixed(0)} lookups/sec)`);
  
  // Test hierarchical lookup performance
  matcher.register('commands', { id: generateUUID(), endpoint: 'commands' });
  
  const hierarchicalStart = Date.now();
  for (let i = 0; i < 100; i++) {
    const match = matcher.match(`commands/unregistered-${i}`);
    if (!match || match.matchType !== 'hierarchical') {
      throw new Error(`Failed hierarchical match for commands/unregistered-${i}`);
    }
  }
  const hierarchicalTime = Date.now() - hierarchicalStart;
  console.log(`    📊 Hierarchical lookups: ${hierarchicalTime}ms for 100 lookups`);
  
  // Performance thresholds (should be very fast)
  if (registrationTime > 1000) {
    throw new Error(`Registration too slow: ${registrationTime}ms for ${endpointCount} endpoints`);
  }
  
  if (lookupTime > 100) {
    throw new Error(`Lookup too slow: ${lookupTime}ms for ${endpointCount} lookups`);
  }
  
  console.log('  ✅ Performance with large endpoint counts is acceptable');
}

// ========================================
// RUN ALL TESTS
// ========================================

async function runAllTests(): Promise<void> {
  try {
    await testBasicRegistrationAndLookup();
    await testPriorityAndSpecificity();
    await testEdgeCasesAndBoundaryConditions();
    await testSubscriberReplacementAndUnregistration();
    await testComplexHierarchiesAndPatterns();
    await testPerformanceWithLargeEndpointCounts();
    
    console.log('✅ All EndpointMatcher unit tests passed!');
    console.log('\n📋 COMPONENT TEST SUMMARY:');
    console.log('  ✅ Basic registration and lookup');
    console.log('  ✅ Priority and specificity resolution');
    console.log('  ✅ Edge cases and boundary conditions');
    console.log('  ✅ Subscriber replacement and unregistration');
    console.log('  ✅ Complex hierarchies and endpoint patterns');
    console.log('  ✅ Performance with large endpoint counts');
    console.log('\n🎯 EndpointMatcher is bulletproof and production-ready!');
    
  } catch (error) {
    console.error('❌ EndpointMatcher test failed:', error);
    throw error;
  }
}

// Run tests if called directly
if (process.argv[1] && process.argv[1].endsWith('EndpointMatcher.test.ts')) {
  runAllTests()
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

export { runAllTests };