#!/usr/bin/env npx tsx

/**
 * Debug validation issue - Rust-like strict typing test
 */

import { DataServiceFactory, DataServiceMode } from './system/data/services/DataServiceFactory';
import type { User, CreateUserData } from './system/data/domains/User';

async function testValidationEnforcement() {
  console.log('🔍 Testing strict validation enforcement...');
  
  const dataService = await DataServiceFactory.create({
    mode: DataServiceMode.JSON_ONLY,
    paths: { jsonDatabase: '.continuum/test-validation' },
    context: { source: 'validation-test' }
  });

  // Test 1: Valid data should succeed
  console.log('\n1. Testing valid data...');
  const validData: CreateUserData = {
    displayName: 'Valid User',
    type: 'human'
  };

  const validResult = await dataService.executeOperation<User>('users/create', validData);
  console.log('Valid result:', validResult.success ? '✅ SUCCESS' : `❌ FAILED: ${validResult.error.message}`);

  // Test 2: Invalid data should fail (empty displayName)
  console.log('\n2. Testing invalid data (empty displayName)...');
  const invalidData: CreateUserData = {
    displayName: '', // Invalid!
    type: 'human'
  };

  const invalidResult = await dataService.executeOperation<User>('users/create', invalidData);
  console.log('Invalid result:', invalidResult.success ? '❌ SHOULD HAVE FAILED' : `✅ CORRECTLY FAILED: ${invalidResult.error.message}`);

  // Test 3: Test ORM method too
  console.log('\n3. Testing ORM method with invalid data...');
  const ormResult = await dataService.create<User>('users', invalidData);
  console.log('ORM result:', ormResult.success ? '❌ SHOULD HAVE FAILED' : `✅ CORRECTLY FAILED: ${ormResult.error.message}`);

  await dataService.close();
}

if (require.main === module) {
  testValidationEnforcement().catch(console.error);
}