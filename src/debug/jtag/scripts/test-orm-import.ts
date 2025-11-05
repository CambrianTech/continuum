#!/usr/bin/env tsx
/**
 * Manual Test of ORM Import Functionality
 *
 * Let's see if import actually works
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';

async function testOrmImport() {
  console.log('🧪 Testing ORM import functionality manually...');

  try {
    // Create DataService
    const dataService = await DataServiceFactory.createSQLiteOnly('.continuum/database/continuum.db');

    console.log('🔧 Initializing DataService...');
    const initResult = await dataService.initialize();
    if (!initResult.success) {
      throw new Error(`DataService initialization failed: ${initResult.error?.message}`);
    }

    // Test 1: Check if import method exists
    console.log('\n📋 Test 1: Check if import method exists on DataService');
    console.log('   dataService.import type:', typeof dataService.import);
    if (typeof dataService.import !== 'function') {
      console.log('❌ dataService.import method does not exist!');
      return;
    }

    // Test 2: First export some data to have something to import
    console.log('\n📋 Test 2: Export current users for testing import');
    const usersExport = await dataService.export('users');
    if (!usersExport.success) {
      console.log('❌ Could not export users for testing:', usersExport.error?.message);
      return;
    }
    console.log(`✅ Exported ${usersExport.data.entities.length} users for testing`);

    // Test 3: Try to import the exported data (should create duplicates or fail)
    console.log('\n📋 Test 3: Import the exported user data');
    try {
      // Extract just the data without BaseEntity fields for import
      const usersToImport = usersExport.data.entities.map(user => {
        const { id, createdAt, updatedAt, version, ...userData } = user;
        return userData;
      });

      console.log('   Trying to import users without BaseEntity fields...');
      const importResult = await dataService.import('users', usersToImport);
      console.log('✅ Import result:', importResult.success);
      if (importResult.success) {
        console.log(`   Imported: ${importResult.data.imported} users`);
        console.log(`   Errors: ${importResult.data.errors.length}`);
        if (importResult.data.errors.length > 0) {
          console.log('   Import errors:', importResult.data.errors.slice(0, 2));
        }
      } else {
        console.log('❌ Import failed:', importResult.error?.message);
      }
    } catch (error: any) {
      console.log('❌ Import threw error:', error.message);
    }

    // Test 4: Check current user count
    console.log('\n📋 Test 4: Check final user count');
    const finalUsers = await dataService.list('users');
    if (finalUsers.success) {
      console.log(`   Current user count: ${finalUsers.data.length}`);
    }

    await dataService.close();

  } catch (error: any) {
    console.error('❌ TEST FAILED:', error.message);
    console.error(error.stack);
  }
}

// Run test
testOrmImport();