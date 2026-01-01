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
import ActivityDataSeed from './ActivityDataSeed';
import { SystemIdentity } from './SystemIdentity';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';

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
    createCollectionName('messages'),
    createCollectionName('activities')
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
          `./jtag ${DATA_COMMANDS.LIST} --collection="${collection}"`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );
        
        const listData = JSON.parse(listResult.split('COMMAND RESULT:')[1].split('============================================================')[0].trim());
        
        if (listData.success && listData.items && listData.items.length > 0) {
          console.log(`📋 Found ${listData.items.length} items in ${collection}`);
          
          // Delete each item individually
          for (const item of listData.items) {
            try {
              const deleteResult = execSync(
                `./jtag ${DATA_COMMANDS.DELETE} --collection="${collection}" --id="${item.id}"`,
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
   * Seed all initial data - users, rooms, activities, messages
   */
  public static async seedAllData(): Promise<void> {
    console.log('🌱 SEEDING ALL INITIAL DATA - Creating fresh system state');

    // Seed users first (required for rooms, activities, and messages)
    await this.seedUsers();

    // Seed chat rooms (returns room ID map for messages)
    const roomIdMap = await this.seedChatRooms();

    // Seed collaborative activities (canvas, browser, etc.)
    await this.seedActivities();

    // Seed initial messages (uses room ID map)
    await this.seedInitialMessages(roomIdMap);

    console.log('✅ ALL DATA SEEDED - System ready for new repo users');
  }

  /**
   * Seed all users - system owner + AI agents
   */
  private static async seedUsers(): Promise<void> {
    console.log('👥 Seeding users...');
    
    const seedData = UserDataSeed.generateSeedUsers();
    
    for (const user of seedData.users) {
      try {
        const command = UserDataSeed.createDataCommand(user);
        
        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag ${DATA_COMMANDS.CREATE} --collection="${command.collection}" --id="${command.id}" --data='${JSON.stringify(command.data)}'`,
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
   * Returns map of uniqueId -> roomId for message seeding
   */
  private static async seedChatRooms(): Promise<Map<string, string>> {
    console.log('🏠 Seeding chat rooms...');

    const identity = SystemIdentity.getIdentity();
    const roomData = RoomDataSeed.generateSeedRooms(identity.userId as any);
    const roomIdMap = new Map<string, string>();

    for (const room of roomData.rooms) {
      try {
        const validatedRoom = RoomDataSeed.createRoomStoreData(room);

        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag ${DATA_COMMANDS.CREATE} --collection="rooms" --data='${JSON.stringify(validatedRoom)}'`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );

        // Parse result to get created room ID
        const resultData = JSON.parse(result.split('COMMAND RESULT:')[1].split('============================================================')[0].trim());
        if (resultData.success && resultData.data?.id) {
          roomIdMap.set(validatedRoom.uniqueId, resultData.data.id);
          console.log(`🏠 Created room: ${room.displayName} (${room.members.length} members, ID: ${resultData.data.id})`);
        }

      } catch (error: any) {
        console.error(`❌ FATAL: Failed to create room ${room.name}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }

    console.log(`✅ Seeded ${roomData.totalCount} chat rooms`);
    return roomIdMap;
  }

  /**
   * Seed collaborative activities using ActivityDataSeed
   * Activities are content instances (canvas, browser, etc.) that participants can join
   */
  private static async seedActivities(): Promise<void> {
    console.log('🎨 Seeding collaborative activities...');

    const identity = SystemIdentity.getIdentity();
    const activityData = ActivityDataSeed.generateSeedActivities(identity.userId as any);

    for (const activity of activityData.activities) {
      try {
        const validatedActivity = ActivityDataSeed.createActivityStoreData(activity);

        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag ${DATA_COMMANDS.CREATE} --collection="activities" --data='${JSON.stringify(validatedActivity)}'`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );

        // Parse result to get created activity ID
        const resultData = JSON.parse(result.split('COMMAND RESULT:')[1].split('============================================================')[0].trim());
        if (resultData.success && resultData.data?.id) {
          console.log(`🎨 Created activity: ${activity.displayName} (${activity.participants.length} participants, ID: ${resultData.data.id})`);
        }

      } catch (error: any) {
        console.error(`❌ FATAL: Failed to create activity ${activity.uniqueId}:`, error.message);
        throw error; // Crash and burn - no fallbacks
      }
    }

    console.log(`✅ Seeded ${activityData.totalCount} collaborative activities`);
  }

  /**
   * Seed initial welcome messages using RoomDataSeed
   */
  private static async seedInitialMessages(roomIdMap: Map<string, string>): Promise<void> {
    console.log('💬 Seeding initial messages...');

    const identity = SystemIdentity.getIdentity();
    const messages = RoomDataSeed.generateSeedMessages(roomIdMap, identity.userId as any, identity.displayName);

    for (const message of messages) {
      try {
        const validatedMessage = RoomDataSeed.createMessageStoreData(message);

        const { execSync } = require('child_process');
        const result = execSync(
          `./jtag ${DATA_COMMANDS.CREATE} --collection="chat_messages" --data='${JSON.stringify(validatedMessage)}'`,
          { encoding: 'utf-8', cwd: process.cwd() }
        );

        console.log(`💬 Created message: ${message.content.text.slice(0, 50)}...`);

      } catch (error: any) {
        console.error(`❌ FATAL: Failed to create message:`, error.message);
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
          `./jtag ${DATA_COMMANDS.LIST} --collection="${collection}"`,
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
        if (collection === 'activities' && count < 2) {
          throw new Error(`Expected at least 2 activities (canvas, browser), found ${count}`);
        }
        // Messages are optional - no welcome messages required
        
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
      console.log('👥 Users: system owner + 5 AI agents (Claude Code, GeneralAI, CodeAI, PlannerAI, Auto Route)');
      console.log('🏠 Rooms: general, academy, pantheon, canvas (chat rooms)');
      console.log('🎨 Activities: canvas-main, browser-main (collaborative content)');
      console.log('✅ All data verified and ready for development');
      
    } catch (error: any) {
      console.error('❌ FATAL: Reset and seed failed:', error.message);
      console.error('🚨 System may be in inconsistent state - manual cleanup required');
      throw error; // Crash and burn - no fallbacks
    }
  }
}

export default DataSeeder;