/**
 * Context Migration Utilities - Helper functions for migrating to secure contexts
 * 
 * These utilities help migrate old-style context creation to secure context patterns.
 * Use these temporarily while migrating the codebase to secure configuration.
 */

import { generateUUID, type UUID } from '../types/CrossPlatformUUID';
import type { JTAGContext, JTAGEnvironment } from '../types/JTAGTypes';
import { createServerContext, createClientContext, createTestContext } from './SecureJTAGContext';
import { isTestEnvironment } from '../../shared/BrowserSafeConfig';

/**
 * Create a secure context from environment - replaces inline context creation
 * Use this instead of: { uuid: generateUUID(), environment: 'server' }
 */
export function createContextForEnvironment(environment: JTAGEnvironment, uuid?: UUID): JTAGContext {
  switch (environment) {
    case 'server':
      return isTestEnvironment() ? createTestContext(uuid) : createServerContext(uuid);
    case 'browser':
      return createClientContext(uuid);
    case 'remote':
      // For remote contexts, use server context (they connect to servers)
      return createServerContext(uuid);
    default:
      throw new Error(`Unknown environment: ${environment}`);
  }
}

/**
 * Migrate an old-style context object to secure context
 * TEMPORARY: Use this for gradual migration
 */
export function migrateToSecureContext(oldContext: { uuid: UUID; environment: JTAGEnvironment }): JTAGContext {
  return createContextForEnvironment(oldContext.environment, oldContext.uuid);
}

/**
 * Create test context with known UUID (for consistent testing)
 */
export function createTestContextWithUUID(uuid: UUID): JTAGContext {
  const context = createTestContext();
  return {
    ...context,
    uuid
  };
}

/**
 * Create server context with known UUID (for system components)
 */
export function createServerContextWithUUID(uuid: UUID): JTAGContext {
  const context = createServerContext();
  return {
    ...context,
    uuid
  };
}