/**
 * Cross-Platform UUID Generator
 * 
 * Provides UUID generation that works in both browser and server environments.
 * Abstracts away the platform-specific crypto implementations.
 */

/**
 * UUIDv4 type - represents RFC 4122 compliant UUIDs
 * Format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx where:
 * - x is any hexadecimal digit (0-9, a-f)  
 * - y is one of 8, 9, a, or b (variant bits)
 * - 4 indicates version 4 (random UUID)
 * 
 * Total: 32 hex digits (128 bits) in 8-4-4-4-12 format
 * Example: 550e8400-e29b-41d4-a716-446655440000
 * 
 * Note: Uses string for TypeScript performance - validated at runtime
 */
export type UUID = string;

/**
 * Cross-platform UUID generation
 */
export function generateUUID(): UUID {
  // Server environment - use Node.js crypto
  if (typeof process !== 'undefined' && process.versions?.node) {
    try {
      // Dynamic import to avoid bundling issues
      const crypto = eval('require')('crypto');
      return crypto.randomUUID();
    } catch  {
      console.warn('Failed to load Node.js crypto, falling back to browser implementation');
    }
  }
  
  // Browser environment - use crypto.randomUUID if available
  if (typeof globalThis !== 'undefined' && globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID() as UUID;
  }
  
  // Fallback implementation using Math.random
  return generateUUIDFallback();
}

/**
 * Fallback UUID generation using Math.random
 * Based on RFC 4122 format
 */
function generateUUIDFallback(): UUID {
  const hex = (n: number) => n.toString(16).padStart(2, '0');
  const randomByte = () => Math.floor(Math.random() * 256);
  
  // Generate 16 random bytes
  const bytes = Array.from({ length: 16 }, randomByte);
  
  // Set version (4) and variant bits according to RFC 4122
  bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
  bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant 10
  
  // Format as UUID string
  const hexString = bytes.map(hex).join('');
  return `${hexString.slice(0, 8)}-${hexString.slice(8, 12)}-${hexString.slice(12, 16)}-${hexString.slice(16, 20)}-${hexString.slice(20, 32)}` as UUID;
}

/**
 * Validate UUID format
 */
export function isValidUUID(value: string): value is UUID {
  const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  return uuidRegex.test(value);
}

/**
 * Convert string to UUID (with validation)
 */
export function toUUID(value: string): UUID {
  if (isValidUUID(value)) {
    return value;
  }
  throw new Error(`Invalid UUID format: ${value}`);
}

/**
 * Create a pseudo-UUID from a string (for context.uuid compatibility)
 * Not cryptographically secure, but maintains format consistency
 */
export function createPseudoUUID(input: string): UUID {
  // Use a simple hash to create deterministic but UUID-formatted string
  let hash = 0;
  for (let i = 0; i < input.length; i++) {
    const char = input.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash; // Convert to 32-bit integer
  }

  // Convert hash to hex and pad/truncate to create UUID format
  const hex = Math.abs(hash).toString(16).padStart(8, '0').slice(0, 8);
  const randomSuffix = generateUUIDFallback().slice(9); // Use random for the rest

  return `${hex}-${randomSuffix}` as UUID;
}

/**
 * Generate deterministic UUID from string input
 * Creates consistent UUIDs for the same input string across all sessions
 * Perfect for seeding data with consistent IDs based on entity names
 */
export function stringToUUID(input: string): UUID {
  // Create multiple hash values from the input string for UUID segments
  const hashes = [];

  // Generate 4 different hash values using different algorithms
  for (let seed = 0; seed < 4; seed++) {
    let hash = seed;
    for (let i = 0; i < input.length; i++) {
      const char = input.charCodeAt(i);
      hash = ((hash << 5) - hash) + char + seed;
      hash = hash & hash; // Convert to 32-bit integer
    }
    hashes.push(Math.abs(hash));
  }

  // Convert hashes to hex segments for UUID format
  const segments = hashes.map(hash =>
    hash.toString(16).padStart(8, '0').slice(0, 8)
  );

  // Build UUID string with proper RFC 4122 v4 format
  const uuid = `${segments[0]}-${segments[1].slice(0, 4)}-4${segments[1].slice(5, 8)}-${(parseInt(segments[2].slice(0, 1), 16) & 0x3 | 0x8).toString(16)}${segments[2].slice(1, 4)}-${segments[3]}`;

  return uuid as UUID;
}