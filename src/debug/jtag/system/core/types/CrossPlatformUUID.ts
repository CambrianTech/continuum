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
 *
 * NOTE: Server code should use crypto.randomUUID() directly from Node.js crypto module.
 * This function is for browser environments only.
 */
export function generateUUID(): UUID {
  // Browser environment - use crypto.randomUUID if available
  if (typeof globalThis !== 'undefined' && globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID() as UUID;
  }

  // Fallback implementation using Math.random (for older browsers)
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

/**
 * ShortId type - last 6 characters of a UUID for human-friendly references
 * Format: 6 hex characters (optionally prefixed with #)
 * Examples: "7bd593", "#7bd593"
 *
 * Used for chat messages (#881f5d), proposals (#b45103), etc.
 *
 * This is a branded type - you cannot assign a plain string to ShortId
 * without validation. Use toShortId() or normalizeShortId() to create.
 */
export type ShortId = string & { readonly __brand: 'ShortId' };

/**
 * Convert UUID to ShortId (last 6 characters)
 * @param uuid - Full UUID to convert
 * @returns Last 6 characters of the UUID
 *
 * @example
 * toShortId("b45103a4-8919-4d4d-af02-e6ee787bd593") // "7bd593"
 */
export function toShortId(uuid: UUID): ShortId {
  return uuid.slice(-6) as ShortId;
}

/**
 * Check if a string is a valid ShortId
 * @param id - String to check
 * @returns True if string is 6 hex characters (optionally with # prefix)
 *
 * @example
 * isShortId("#7bd593") // true
 * isShortId("7bd593")  // true
 * isShortId("7bd59")   // false (too short)
 * isShortId("xyz123")  // false (invalid hex)
 */
export function isShortId(id: string): id is ShortId {
  const normalized = id.replace(/^#/, '');
  return /^[0-9a-f]{6}$/i.test(normalized);
}

/**
 * Normalize ShortId (remove # prefix, validate format)
 * @param id - ShortId to normalize (with or without # prefix)
 * @returns Normalized 6-character hex string
 * @throws Error if not a valid ShortId
 *
 * @example
 * normalizeShortId("#7bd593") // "7bd593"
 * normalizeShortId("7bd593")  // "7bd593"
 * normalizeShortId("xyz")     // throws Error
 */
export function normalizeShortId(id: string): ShortId {
  const normalized = id.replace(/^#/, '');
  if (!isShortId(normalized)) {
    throw new Error(`Invalid ShortId format: ${id}. Expected 6 hex characters (e.g., #7bd593)`);
  }
  return normalized as ShortId;
}