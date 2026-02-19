#!/usr/bin/env tsx
/**
 * Executable Data Clearing Script
 * 
 * For clearing all data when needed.
 * Run: npx tsx api/data-seed/clear-data.ts
 */

import DataSeeder from './DataSeeder';

async function main() {
  try {
    await DataSeeder.clearAllData();
    console.log('✅ All data cleared successfully');
  } catch (error: any) {
    console.error('❌ FATAL: Data clearing failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}