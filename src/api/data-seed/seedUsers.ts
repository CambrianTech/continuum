#!/usr/bin/env tsx
/**
 * User Seeding Command - API-Driven Data Creation
 * 
 * Uses JTAG data/create commands instead of manual filesystem operations.
 * Strict typing, crash and burn on failure - no fallbacks.
 */

import UserDataSeed from './UserDataSeed';

export async function seedUsers(): Promise<void> {
  console.log('🌱 Seeding users via JTAG API (no manual fs calls)...');
  
  const seedData = UserDataSeed.generateSeedUsers();
  
  console.log(`👥 Generated ${seedData.totalCount} users at ${seedData.createdAt}`);
  
  // Create each user via JTAG data/create command
  for (const user of seedData.users) {
    try {
      const command = UserDataSeed.createDataCommand(user);
      
      console.log(`👤 Creating user: ${user.name} (${user.userType})`);
      console.log(`   ID: ${user.id}`);
      console.log(`   Capabilities: ${user.capabilities.map(c => c.name).join(', ')}`);
      
      // TODO: Execute actual JTAG data/create command here
      // This would be: await jtagClient.commands.dataCreate(command)
      console.log(`   📝 Command prepared: ${JSON.stringify(command, null, 2)}`);
      console.log('   ✅ User data prepared for JTAG creation');
      
    } catch (error) {
      // Crash and burn - no fallbacks
      console.error(`❌ FATAL: Failed to create user ${user.name}:`, error);
      throw error;
    }
  }
  
  console.log(`✅ Successfully prepared ${seedData.totalCount} users for database`);
  console.log('💡 Next step: Execute via JTAG client when system is ready');
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  seedUsers().catch((error) => {
    console.error('❌ FATAL: User seeding failed:', error);
    process.exit(1); // Crash and burn - no fallbacks
  });
}

export default seedUsers;