#!/usr/bin/env tsx
/**
 * Manual Test of ORM Export Functionality
 *
 * Let's see what actually works vs what we think works
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';
import { getDatabasePath } from '../system/config/ServerConfig';

async function testOrmExport() {
  console.log('🧪 Testing ORM export functionality manually...');

  try {
    // Create DataService - use single source of truth for db path
    const dataService = await DataServiceFactory.createSQLiteOnly(getDatabasePath());

    console.log('🔧 Initializing DataService...');
    const initResult = await dataService.initialize();
    if (!initResult.success) {
      throw new Error(`DataService initialization failed: ${initResult.error?.message}`);
    }

    // Test 1: Check if export method exists
    console.log('\n📋 Test 1: Check if export method exists on DataService');
    console.log('   dataService.export type:', typeof dataService.export);
    if (typeof dataService.export !== 'function') {
      console.log('❌ dataService.export method does not exist!');
      return;
    }

    // Test 2: Try to export users collection
    console.log('\n📋 Test 2: Export users collection');
    try {
      const usersExport = await dataService.export('users');
      console.log('✅ Export users result:', usersExport.success);
      if (usersExport.success) {
        console.log(`   Exported ${usersExport.data.entities.length} users`);
        console.log('   Sample user:', usersExport.data.entities[0]?.displayName || 'N/A');
      } else {
        console.log('❌ Export failed:', usersExport.error?.message);
      }
    } catch (error: any) {
      console.log('❌ Export threw error:', error.message);
    }

    // Test 3: Check if exportAll method exists
    console.log('\n📋 Test 3: Check if exportAll method exists');
    console.log('   dataService.exportAll type:', typeof dataService.exportAll);
    if (typeof dataService.exportAll !== 'function') {
      console.log('❌ dataService.exportAll method does not exist!');
      return;
    }

    // Test 4: Try exportAll
    console.log('\n📋 Test 4: Export all collections');
    try {
      const allExport = await dataService.exportAll(['users', 'rooms']);
      console.log('✅ Export all result:', allExport.success);
      if (allExport.success) {
        console.log('   Collections exported:', Object.keys(allExport.data.collections));
        for (const [collection, items] of Object.entries(allExport.data.collections)) {
          console.log(`   ${collection}: ${Array.isArray(items) ? items.length : 0} items`);
        }
      } else {
        console.log('❌ ExportAll failed:', allExport.error?.message);
      }
    } catch (error: any) {
      console.log('❌ ExportAll threw error:', error.message);
    }

    await dataService.close();

  } catch (error: any) {
    console.error('❌ TEST FAILED:', error.message);
    console.error(error.stack);
  }
}

// Run test
testOrmExport();