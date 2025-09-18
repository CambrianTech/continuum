#!/usr/bin/env tsx
/**
 * QueryBuilder Debug Test - Figure out what's broken
 */

import { QueryBuilder } from './daemons/data-daemon/shared/QueryBuilder';

console.log('🧪 Testing QueryBuilder...');

// Test 1: Basic query construction
const query1 = new QueryBuilder()
  .select('id', 'name')  // Pass as individual args, not array
  .from('users')
  .build();

console.log('Test 1 - Basic query:', JSON.stringify(query1, null, 2));

// Test 2: Query with where clause
const query2 = new QueryBuilder()
  .from('users')
  .where('userType', 'human')
  .build();

console.log('Test 2 - Where clause:', JSON.stringify(query2, null, 2));

// Test 3: List all collections (this might be what's failing)
const query3 = new QueryBuilder()
  .from('users')
  .build();

console.log('Test 3 - Simple collection query:', JSON.stringify(query3, null, 2));

// Test 4: Check if any of our type utilities are causing issues
try {
  const query4 = new QueryBuilder()
    .select('*')
    .from('test-collection')
    .limit(10)
    .build();

  console.log('Test 4 - Full query:', JSON.stringify(query4, null, 2));
} catch (error) {
  console.error('❌ Test 4 failed:', error);
}

console.log('✅ QueryBuilder tests complete');