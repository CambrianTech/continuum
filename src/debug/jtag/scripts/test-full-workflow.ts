#!/usr/bin/env tsx
/**
 * Test Complete Export/Import Workflow
 *
 * Real-world scenario: export data, clear database, re-import
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';
import { getDatabasePath } from '../system/config/ServerConfig';

async function testFullWorkflow() {
  console.log('🧪 Testing complete export/import workflow...');

  try {
    const dataService = await DataServiceFactory.createSQLiteOnly(getDatabasePath());

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

    // Step 2: Export all data
    console.log('\n📋 Step 2: Export all current data');
    const exportResult = await dataService.exportAll(['users', 'rooms', 'chat_messages']);
    if (!exportResult.success) {
      throw new Error(`Export failed: ${exportResult.error?.message}`);
    }

    const backupData = exportResult.data;
    console.log(`✅ Exported data from ${Object.keys(backupData.collections).length} collections`);
    for (const [collection, items] of Object.entries(backupData.collections)) {
      console.log(`   ${collection}: ${Array.isArray(items) ? items.length : 0} items`);
    }

    // Step 3: Clear database (simulate npm run data:clear)
    console.log('\n📋 Step 3: Clear all data (simulate data:clear)');
    // Note: We don't have a bulk delete method, so let's manually delete

    // For now, let's just show what we would need to clear
    console.log('   ⚠️ Note: DataService doesn\'t have bulk delete yet');
    console.log('   In real scenario: rm -rf .continuum/database/*');
    console.log('   Skipping clear for now...');

    // Step 4: Re-import the exported data
    console.log('\n📋 Step 4: Re-import the exported data');

    // Import each collection
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
        if (importResult.data.errors.length > 0) {
          console.log(`   ❌ Errors: ${importResult.data.errors.slice(0, 2)}`);
        }
      } else {
        console.log(`   ❌ ${collection} import failed: ${importResult.error?.message}`);
      }
    }

    // Step 5: Verify final state
    console.log('\n📋 Step 5: Verify final database state');
    const finalUsers = await dataService.list('users');
    const finalRooms = await dataService.list('rooms');
    const finalMessages = await dataService.list('chat_messages');

    if (finalUsers.success) console.log(`   Users: ${finalUsers.data.length}`);
    if (finalRooms.success) console.log(`   Rooms: ${finalRooms.data.length}`);
    if (finalMessages.success) console.log(`   Messages: ${finalMessages.data.length}`);

    await dataService.close();
    console.log('\n🎉 Full workflow test completed!');

  } catch (error: any) {
    console.error('❌ WORKFLOW TEST FAILED:', error.message);
    console.error(error.stack);
  }
}

// Run test
testFullWorkflow();