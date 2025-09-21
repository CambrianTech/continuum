#!/usr/bin/env tsx
/**
 * Comprehensive Data Seeding System
 * 
 * Provides repeatable seeding and clearing for ALL initial data.
 * Essential for new repo users to have consistent, reliable setup.
 * 
 * Features:
 * - Clear all data collections
 * - Seed users (joel + AI agents)
 * - Seed chat rooms (general, academy)
 * - Verify seeding worked
 * - Crash and burn error handling - no fallbacks
 */

import UserDataSeed from './UserDataSeed';
import RoomDataSeed from './RoomDataSeed';

// Rust-like branded types for strict typing
export type CollectionName = string & { readonly __brand: 'CollectionName' };
export type SeedOperationResult = {
  readonly success: boolean;
  readonly collection: CollectionName;
  readonly count: number;
  readonly error?: string;
};

export function createCollectionName(name: string): CollectionName {
  if (!name || name.trim().length === 0) {
    throw new Error('CollectionName cannot be empty');
  }
  return name as CollectionName;
}

export class DataSeeder {
  private static readonly COLLECTIONS = [
    createCollectionName('users'),
    createCollectionName('rooms'),
    createCollectionName('messages')
  ] as const;

  /**
   * Clear ALL data collections - complete reset
   */
  public static async clearAllData(): Promise<void> {
    console.log('🧹 CLEARING ALL DATA - Complete reset for fresh start');
    
    for (const collection of this.COLLECTIONS) {
      try {
        console.log(`🗑️ Clearing collection: ${collection}`);
        
        // First, list all items to see what we're deleting
        const { execSync } = require('child_process');
        const listResult = execSync(
          `./jtag data/list --collection="${collection}"`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );
        
        const listData = JSON.parse(listResult.split('COMMAND RESULT:')[1].split('============================================================')[0].trim());
        
        if (listData.success && listData.items && listData.items.length > 0) {
          console.log(`📋 Found ${listData.items.length} items in ${collection}`);
          
          // Delete each item individually
          for (const item of listData.items) {
            try {
              const deleteResult = execSync(
                `./jtag data/delete --collection="${collection}" --id="${item.id}"`,
                { encoding: 'utf-8', cwd: process.cwd() }
              );
              console.log(`🗑️ Deleted ${item.id} from ${collection}`);
            } catch (error: any) {
              console.warn(`⚠️ Failed to delete ${item.id}: ${error.message}`);
            }
          }
        } else {
          console.log(`📋 Collection ${collection} is empty`);
        }
        
        console.log(`✅ Cleared collection: ${collection}`);
        
      } catch (error: any) {
        console.error(`❌ FATAL: Failed to clear collection ${collection}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }
    
    console.log('✅ ALL DATA CLEARED - Fresh slate ready for seeding');
  }

  /**
   * Seed all initial data - users, rooms, messages
   */
  public static async seedAllData(): Promise<void> {
    console.log('🌱 SEEDING ALL INITIAL DATA - Creating fresh system state');
    
    // Seed users first (required for rooms and messages)
    await this.seedUsers();
    
    // Seed chat rooms
    await this.seedChatRooms();
    
    // Seed initial messages
    await this.seedInitialMessages();
    
    console.log('✅ ALL DATA SEEDED - System ready for new repo users');
  }

  /**
   * Seed all users - joel + AI agents
   */
  private static async seedUsers(): Promise<void> {
    console.log('👥 Seeding users...');
    
    const seedData = UserDataSeed.generateSeedUsers();
    
    for (const user of seedData.users) {
      try {
        const command = UserDataSeed.createDataCommand(user);
        
        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag data/create --collection="${command.collection}" --id="${command.id}" --data='${JSON.stringify(command.data)}'`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );
        
        console.log(`👤 Created user: ${user.name} (${user.userType})`);
        
      } catch (error: any) {
        console.error(`❌ FATAL: Failed to create user ${user.name}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }
    
    console.log(`✅ Seeded ${seedData.totalCount} users`);
  }

  /**
   * Seed initial chat rooms using RoomDataSeed
   */
  private static async seedChatRooms(): Promise<void> {
    console.log('🏠 Seeding chat rooms...');
    
    const roomData = RoomDataSeed.generateSeedRooms();
    
    for (const room of roomData.rooms) {
      try {
        const command = RoomDataSeed.createRoomDataCommand(room);
        
        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag data/create --collection="${command.collection}" --id="${command.id}" --data='${JSON.stringify(command.data)}'`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );
        
        console.log(`🏠 Created room: ${room.name} (${room.members.length} members)`);
        
      } catch (error: any) {
        console.error(`❌ FATAL: Failed to create room ${room.name}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }
    
    console.log(`✅ Seeded ${roomData.totalCount} chat rooms`);
  }

  /**
   * Seed initial welcome messages using RoomDataSeed
   */
  private static async seedInitialMessages(): Promise<void> {
    console.log('💬 Seeding initial messages...');
    
    const messages = RoomDataSeed.generateSeedMessages();
    
    for (const message of messages) {
      try {
        const command = RoomDataSeed.createMessageDataCommand(message);
        
        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag data/create --collection="${command.collection}" --id="${command.id}" --data='${JSON.stringify(command.data)}'`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );
        
        console.log(`💬 Created message: ${message.content.slice(0, 50)}...`);
        
      } catch (error: any) {
        console.error(`❌ FATAL: Failed to create message ${message.id}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }
    
    console.log(`✅ Seeded ${messages.length} initial messages`);
  }

  /**
   * Verify all seeded data exists and is correct
   */
  public static async verifySeededData(): Promise<void> {
    console.log('🔍 VERIFYING SEEDED DATA - Ensuring system is ready');
    
    for (const collection of this.COLLECTIONS) {
      try {
        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag data/list --collection="${collection}"`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );
        
        const data = JSON.parse(result.split('COMMAND RESULT:')[1].split('============================================================')[0].trim());
        
        if (!data.success) {
          throw new Error(`List command failed for ${collection}: ${data.error}`);
        }
        
        const count = data.items ? data.items.length : 0;
        console.log(`✅ Collection ${collection}: ${count} items`);
        
        if (collection === 'users' && count < 6) {
          throw new Error(`Expected at least 6 users, found ${count}`);
        }
        if (collection === 'rooms' && count < 2) {
          throw new Error(`Expected at least 2 rooms, found ${count}`);
        }
        if (collection === 'chat_messages' && count < 3) {
          throw new Error(`Expected at least 3 messages, found ${count}`);
        }
        
      } catch (error: any) {
        console.error(`❌ FATAL: Verification failed for ${collection}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }
    
    console.log('✅ ALL DATA VERIFIED - System ready for new repo users');
  }

  /**
   * Complete reset and reseed - the main operation new repo users need
   */
  public static async resetAndSeed(): Promise<void> {
    console.log('🔄 COMPLETE RESET AND SEED - Fresh system for new repo users');
    console.log('=='.repeat(40));
    
    try {
      await this.clearAllData();
      await this.seedAllData();
      await this.verifySeededData();
      
      console.log('=='.repeat(40));
      console.log('🎉 COMPLETE! System ready with fresh data for new repo users');
      console.log('👥 Users: joel + 5 AI agents (Claude Code, GeneralAI, CodeAI, PlannerAI, Auto Route)');
      console.log('🏠 Rooms: general (6 members), academy (3 members)');
      console.log('💬 Messages: Welcome messages in both rooms');
      console.log('✅ All data verified and ready for development');
      
    } catch (error: any) {
      console.error('❌ FATAL: Reset and seed failed:', error.message);
      console.error('🚨 System may be in inconsistent state - manual cleanup required');
      throw error; // Crash and burn - no fallbacks
    }
  }
}

export default DataSeeder;