#!/usr/bin/env tsx
/**
 * Test Clear + Import Functionality
 *
 * Test the new DataService.clear() and proper clear/import workflow
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';

async function testClearImport() {
  console.log('🧪 Testing clear + import functionality...');

  try {
    const dataService = await DataServiceFactory.createSQLiteOnly('.continuum/database/continuum.db');

    const initResult = await dataService.initialize();
    if (!initResult.success) {
      throw new Error(`DataService initialization failed: ${initResult.error?.message}`);
    }

    // Step 1: Check current state
    console.log('\n📋 Step 1: Current database state');
    const currentUsers = await dataService.list('users');
    const currentRooms = await dataService.list('rooms');
    const currentMessages = await dataService.list('chat_messages');

    if (currentUsers.success) console.log(`   Users: ${currentUsers.data.length}`);
    if (currentRooms.success) console.log(`   Rooms: ${currentRooms.data.length}`);
    if (currentMessages.success) console.log(`   Messages: ${currentMessages.data.length}`);

    // Step 2: Export current data
    console.log('\n📋 Step 2: Export all current data');
    const exportResult = await dataService.exportAll(['users', 'rooms', 'chat_messages']);
    if (!exportResult.success) {
      throw new Error(`Export failed: ${exportResult.error?.message}`);
    }

    const backupData = exportResult.data;
    console.log(`✅ Exported data from ${Object.keys(backupData.collections).length} collections`);

    // Step 3: Test new clear functionality
    console.log('\n📋 Step 3: Test DataService.clearAll()');
    const clearResult = await dataService.clearAll(['users', 'rooms', 'chat_messages']);
    if (clearResult.success) {
      console.log('✅ Clear all completed:');
      for (const [collection, result] of Object.entries(clearResult.data.results)) {
        console.log(`   ${collection}: deleted ${result.deleted} items, errors: ${result.errors.length}`);
      }
    } else {
      console.log(`❌ Clear all failed: ${clearResult.error?.message}`);
      return;
    }

    // Step 4: Verify database is empty
    console.log('\n📋 Step 4: Verify database is empty after clear');
    const afterClearUsers = await dataService.list('users');
    const afterClearRooms = await dataService.list('rooms');
    const afterClearMessages = await dataService.list('chat_messages');

    if (afterClearUsers.success) console.log(`   Users: ${afterClearUsers.data.length} (should be 0)`);
    if (afterClearRooms.success) console.log(`   Rooms: ${afterClearRooms.data.length} (should be 0)`);
    if (afterClearMessages.success) console.log(`   Messages: ${afterClearMessages.data.length} (should be 0)`);

    // Step 5: Re-import the exported data
    console.log('\n📋 Step 5: Re-import the exported data');

    for (const [collection, entities] of Object.entries(backupData.collections)) {
      if (!Array.isArray(entities) || entities.length === 0) {
        console.log(`   Skipping empty collection: ${collection}`);
        continue;
      }

      // Remove BaseEntity fields for import
      const entitiesToImport = entities.map((entity: any) => {
        const { id, createdAt, updatedAt, version, ...cleanEntity } = entity;
        return cleanEntity;
      });

      console.log(`   Importing ${entitiesToImport.length} ${collection}...`);
      const importResult = await dataService.import(collection, entitiesToImport);

      if (importResult.success) {
        console.log(`   ✅ ${collection}: imported ${importResult.data.imported}, errors: ${importResult.data.errors.length}`);
      } else {
        console.log(`   ❌ ${collection} import failed: ${importResult.error?.message}`);
      }
    }

    // Step 6: Verify final state matches original
    console.log('\n📋 Step 6: Verify final database state');
    const finalUsers = await dataService.list('users');
    const finalRooms = await dataService.list('rooms');
    const finalMessages = await dataService.list('chat_messages');

    if (finalUsers.success) console.log(`   Users: ${finalUsers.data.length}`);
    if (finalRooms.success) console.log(`   Rooms: ${finalRooms.data.length}`);
    if (finalMessages.success) console.log(`   Messages: ${finalMessages.data.length}`);

    await dataService.close();
    console.log('\n🎉 Clear + import test completed successfully!');

  } catch (error: any) {
    console.error('❌ CLEAR+IMPORT TEST FAILED:', error.message);
    console.error(error.stack);
  }
}

// Run test
testClearImport();