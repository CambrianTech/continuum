/**
 * Test Entity Constants
 *
 * Centralized tracking of test entities created during integration tests.
 * These are cleaned up automatically on `npm start` to prevent test pollution.
 *
 * WHY: Integration tests create CRUD test entities. If tests fail before cleanup,
 * these entities persist and pollute the database. This file tracks test entity
 * patterns so seed scripts can automatically clean them up.
 */

/**
 * Test entity unique ID patterns
 * Used to identify and clean up test entities during seeding
 */
export const TEST_ENTITY_PATTERNS = {
  // User entities created by integration tests
  users: {
    // database-chat-integration.test.ts creates users like "crud-test-user-1762399520021"
    // state-system-integration.test.ts creates users like "state-test-user-1762399520021"
    uniqueIdPrefix: ['crud-test-user-', 'state-test-user-'],
    // ID format: timestamp-based like "1762399520021-mpki7plc1"
    idPattern: /^\d{13}-[a-z0-9]+$/,
    // Display name pattern
    displayNamePattern: /^(CRUD Test User|UPDATED Test User|State Test User)$/,
  },

  // Room entities created by integration tests
  rooms: {
    // database-chat-integration.test.ts creates rooms like "crud-test-room-1762399520021"
    uniqueIdPrefix: 'crud-test-room-',
    namePrefix: 'crud-test-room',
    // ID format: timestamp-based
    idPattern: /^\d{13}-[a-z0-9]+$/,
    displayNamePattern: /^(CRUD Test Room|UPDATED Test Room)$/,
  },

  // Message entities created by integration tests
  messages: {
    // Test messages have content like "Test message for CRUD operations"
    contentPattern: /^Test message for CRUD operations/,
    // ID format: timestamp-based
    idPattern: /^\d{13}-[a-z0-9]+$/,
  }
};

/**
 * Explicit test entity IDs that failed to clean up in previous test runs
 * Add specific IDs here if you notice stuck test entities in the database
 *
 * MAINTENANCE: This array should be kept empty in committed code.
 * It's for temporary local cleanup during development.
 */
export const STUCK_TEST_ENTITY_IDS = {
  users: [
    // Example: '1762399520021-mpki7plc1',
  ] as string[],

  rooms: [
    // Example: '1762402034433-snxx4yw7l',
  ] as string[],

  messages: [
    // Example: '1762402765105-mszfs6fpr',
  ] as string[],
};

/**
 * Check if a user entity is a test entity
 */
export function isTestUser(user: { uniqueId?: string; displayName?: string; id?: string }): boolean {
  // Check uniqueId prefix (supports array of prefixes)
  if (user.uniqueId) {
    const prefixes = Array.isArray(TEST_ENTITY_PATTERNS.users.uniqueIdPrefix)
      ? TEST_ENTITY_PATTERNS.users.uniqueIdPrefix
      : [TEST_ENTITY_PATTERNS.users.uniqueIdPrefix];

    for (const prefix of prefixes) {
      if (user.uniqueId.startsWith(prefix)) {
        return true;
      }
    }
  }

  // Check display name pattern
  if (user.displayName && TEST_ENTITY_PATTERNS.users.displayNamePattern.test(user.displayName)) {
    return true;
  }

  // Check explicit stuck IDs
  if (user.id && STUCK_TEST_ENTITY_IDS.users.includes(user.id)) {
    return true;
  }

  return false;
}

/**
 * Check if a room entity is a test entity
 */
export function isTestRoom(room: { uniqueId?: string; name?: string; displayName?: string; id?: string }): boolean {
  // Check uniqueId prefix
  if (room.uniqueId?.startsWith(TEST_ENTITY_PATTERNS.rooms.uniqueIdPrefix)) {
    return true;
  }

  // Check name prefix
  if (room.name?.startsWith(TEST_ENTITY_PATTERNS.rooms.namePrefix)) {
    return true;
  }

  // Check display name pattern
  if (room.displayName && TEST_ENTITY_PATTERNS.rooms.displayNamePattern.test(room.displayName)) {
    return true;
  }

  // Check explicit stuck IDs
  if (room.id && STUCK_TEST_ENTITY_IDS.rooms.includes(room.id)) {
    return true;
  }

  return false;
}

/**
 * Check if a message entity is a test entity
 */
export function isTestMessage(message: { content?: string; text?: string; id?: string }): boolean {
  // Check content pattern
  const content = message.content || message.text || '';
  if (TEST_ENTITY_PATTERNS.messages.contentPattern.test(content)) {
    return true;
  }

  // Check explicit stuck IDs
  if (message.id && STUCK_TEST_ENTITY_IDS.messages.includes(message.id)) {
    return true;
  }

  return false;
}
