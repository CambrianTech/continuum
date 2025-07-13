/**
 * Default ChatRoom Configuration
 * 
 * Types and utilities for loading default room configuration from JSON files.
 * This allows easy editing of default rooms without touching code.
 */

import { promises as fs } from 'fs';
import { resolve, dirname } from 'path';
import { ChatRoomType } from '../../types/shared/ChatRoomTypes';

/**
 * Interface describing a default room specification from JSON
 */
export interface DefaultRoomSpec {
  id: string;
  name: string;
  type: string; // Will be validated against ChatRoomType enum
  description?: string;
  autoCreated?: boolean;
  metadata?: Record<string, any>;
}

/**
 * Interface for the default rooms JSON file structure
 */
export interface DefaultRoomsConfig {
  defaultRooms: DefaultRoomSpec[];
}

/**
 * Load default rooms configuration from JSON file
 */
export async function loadDefaultRoomsConfig(): Promise<DefaultRoomSpec[]> {
  try {
    // Get the directory of this file and construct path to config
    const configPath = resolve(dirname(import.meta.url.replace('file://', '')), 'config/default-rooms.json');
    
    const configContent = await fs.readFile(configPath, 'utf8');
    const config: DefaultRoomsConfig = JSON.parse(configContent);
    
    // Validate the configuration
    if (!config.defaultRooms || !Array.isArray(config.defaultRooms)) {
      throw new Error('Invalid default rooms configuration: missing defaultRooms array');
    }
    
    // Validate each room spec
    for (const room of config.defaultRooms) {
      if (!room.id || !room.name || !room.type) {
        throw new Error(`Invalid room specification: ${JSON.stringify(room)} - missing required fields`);
      }
      
      // Validate room type
      const validTypes = Object.values(ChatRoomType);
      if (!validTypes.includes(room.type as ChatRoomType)) {
        throw new Error(`Invalid room type "${room.type}" for room "${room.id}". Valid types: ${validTypes.join(', ')}`);
      }
    }
    
    return config.defaultRooms;
    
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.error('❌ Failed to load default rooms configuration:', errorMessage);
    
    // Return minimal fallback configuration
    return [
      {
        id: 'general',
        name: 'General Chat',
        type: 'chat',
        description: 'Fallback general chat room',
        autoCreated: true,
        metadata: { fallback: true }
      }
    ];
  }
}

/**
 * Get default room by ID from loaded configuration
 */
export function getDefaultRoom(rooms: DefaultRoomSpec[], id: string): DefaultRoomSpec | undefined {
  return rooms.find(room => room.id === id);
}

/**
 * Check if a room ID is a default room
 */
export function isDefaultRoom(rooms: DefaultRoomSpec[], id: string): boolean {
  return rooms.some(room => room.id === id);
}

/**
 * Get all default room IDs
 */
export function getDefaultRoomIds(rooms: DefaultRoomSpec[]): string[] {
  return rooms.map(room => room.id);
}