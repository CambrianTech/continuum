#!/usr/bin/env tsx
/**
 * Export Current Database Data
 *
 * Uses DataService.exportAll() to export current entities
 * Generates TypeScript seed files that can be used for import
 */

import { writeFileSync } from 'fs';
import { DataServiceFactory } from '../system/data/services/DataServiceFactory';
import { getDatabasePath } from '../system/config/ServerConfig';

async function exportCurrentData() {
  console.log('📦 Exporting current database data using ORM...');

  try {
    // Create DataService - use single source of truth for db path
    const dataService = await DataServiceFactory.createSQLiteOnly(getDatabasePath());

    console.log('🔧 Initializing DataService...');
    const initResult = await dataService.initialize();
    if (!initResult.success) {
      throw new Error(`DataService initialization failed: ${initResult.error?.message}`);
    }

    // Export all collections
    console.log('📤 Exporting all collections...');
    const collections = ['users', 'rooms', 'chat_messages'];
    const exportResult = await dataService.exportAll(collections);

    if (!exportResult.success) {
      throw new Error(`Export failed: ${exportResult.error?.message}`);
    }

    const exportData = exportResult.data;
    console.log(`✅ Exported data from ${Object.keys(exportData.collections).length} collections at ${exportData.exportedAt}`);

    // Generate TypeScript seed file
    const tsContent = generateSeedFile(exportData);

    // Write to seed file
    const outputPath = 'data/seed/currentData.ts';
    writeFileSync(outputPath, tsContent, 'utf-8');
    console.log(`📄 Saved export to: ${outputPath}`);

    // Also save JSON for easy inspection
    const jsonPath = 'data/seed/currentData.json';
    writeFileSync(jsonPath, JSON.stringify(exportData, null, 2), 'utf-8');
    console.log(`📄 Saved JSON to: ${jsonPath}`);

    // Print summary
    console.log('\n📊 Export Summary:');
    for (const [collection, items] of Object.entries(exportData.collections)) {
      console.log(`   ${collection}: ${Array.isArray(items) ? items.length : 0} records`);
    }

    await dataService.close();
    console.log('\n🎉 Data export completed successfully!');
    console.log('💡 To import: dataService.import(collection, exportedData.collections[collection])');

  } catch (error: any) {
    console.error('❌ EXPORT FAILED:', error.message);
    console.error(error.stack);
    process.exit(1);
  }
}

function generateSeedFile(exportData: any): string {
  return `/**
 * Current Database Export
 *
 * Generated at: ${exportData.exportedAt}
 * Via: dataService.exportAll(['users', 'rooms', 'chat_messages'])
 *
 * This file contains the current state of all entities and can be imported
 * using dataService.import() methods
 */

export const currentSeedData = ${JSON.stringify(exportData, null, 2)};

export default currentSeedData;

// Individual collections for selective import
export const users = currentSeedData.collections.users;
export const rooms = currentSeedData.collections.rooms;
export const chat_messages = currentSeedData.collections.chat_messages;

/**
 * Usage Examples:
 *
 * // Import all data
 * await dataService.import('users', users);
 * await dataService.import('rooms', rooms);
 * await dataService.import('chat_messages', chat_messages);
 *
 * // Or import selectively
 * await dataService.import('users', users.slice(0, 1)); // Just first user
 */
`;
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  exportCurrentData();
}

export default exportCurrentData;