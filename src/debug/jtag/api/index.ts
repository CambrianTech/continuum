/**
 * JTAG API - Main Public Interface
 * 
 * Single entry point for external consumers.
 * This is the consumer-first API that follows clean architectural boundaries.
 * 
 * Usage:
 * ```typescript
 * import { JTAGClient, PersonaUser, FileLoadParams } from '@continuum/jtag/api';
 * ```
 */

// Client interface - main client functionality
export * from './client';

// Command types - parameter and result interfaces  
export * from './commands';

// User types - domain user hierarchy
export * from './types/User';

// API version and metadata
export const API_VERSION = '1.0.0';
export const API_NAME = 'JTAG Consumer API';

// Re-export convenience types
export type {
  // Client types
  IJTAGClient,
  ClientConnectionOptions,
  ClientConnectionResult,
  ClientFactory
} from './client/JTAGClient';

export type {
  // Common command types
  CommandParams,
  CommandResult,
  CommandCategory
} from './commands';

export type {
  // User hierarchy
  BaseUser,
  HumanUser,
  PersonaUser,
  AgentUser,
  AIUser,
  UserType,
  Permission,
  UserCapability
} from './types/User';