#!/usr/bin/env tsx
/**
 * Executable Data Seeding Script
 * 
 * For new repo users to set up initial system data.
 * Run: npx tsx api/data-seed/seed-data.ts
 */

import DataSeeder from './DataSeeder';

async function main() {
  try {
    await DataSeeder.resetAndSeed();
  } catch (error: any) {
    console.error('❌ FATAL: Data seeding failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}