/**
 * Unique ID Utilities
 *
 * Generates short, simple, valid uniqueId formats for users
 * Format: @username (e.g., @joel, @sentinel, @helper)
 */

/**
 * Convert any string into a valid uniqueId format
 *
 * Rules:
 * - Prefix with @ if not already present
 * - Remove spaces, convert to lowercase
 * - Keep only alphanumeric and dashes
 * - Max length: 30 characters
 *
 * @param input - Any display name or identifier
 * @returns Valid uniqueId in @username format
 *
 * @example
 * generateUniqueId('Joel') → '@joel'
 * generateUniqueId('Helper AI') → '@helperai'
 * generateUniqueId('Code Review AI') → '@codereviewai'
 * generateUniqueId('Sentinel') → '@sentinel'
 */
export function generateUniqueId(input: string): string {
  // Remove @ if present at start (we'll add it back)
  let cleaned = input.trim();
  if (cleaned.startsWith('@')) {
    cleaned = cleaned.substring(1);
  }

  // Convert to lowercase
  cleaned = cleaned.toLowerCase();

  // Remove spaces and special characters, keep only alphanumeric and dashes
  cleaned = cleaned.replace(/[^a-z0-9-]/g, '');

  // Limit length
  if (cleaned.length > 29) {
    cleaned = cleaned.substring(0, 29);
  }

  // Add @ prefix
  return `@${cleaned}`;
}

/**
 * Generate uniqueId for human users
 * @param displayName - Human's display name
 * @returns uniqueId in @username format
 */
export function generateHumanUniqueId(displayName: string): string {
  return generateUniqueId(displayName);
}

/**
 * Generate uniqueId for AI persona users
 * @param displayName - Persona's display name
 * @returns uniqueId in @username format
 */
export function generatePersonaUniqueId(displayName: string): string {
  return generateUniqueId(displayName);
}

/**
 * Generate uniqueId for AI agent users
 * @param displayName - Agent's display name
 * @returns uniqueId in @username format
 */
export function generateAgentUniqueId(displayName: string): string {
  return generateUniqueId(displayName);
}
