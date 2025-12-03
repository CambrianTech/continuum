/**
 * JSON Extraction Utilities
 *
 * Handles extracting JSON from polluted CLI output where logs or other
 * text may appear before/after/within the JSON response.
 *
 * This is a SINGLE SOURCE OF TRUTH for JSON extraction across all scripts.
 */

/**
 * Extract JSON object from polluted string output
 *
 * Uses regex to find the outermost JSON object, handling:
 * - Log lines before JSON
 * - Log lines after JSON
 * - Multiple JSON objects (returns first complete one)
 * - Nested braces within the JSON
 *
 * @param output - Raw string output that may contain JSON
 * @returns Parsed JSON object or null if no valid JSON found
 */
export function extractJSON<T = any>(output: string): T | null {
  try {
    // Count braces to find the complete outermost JSON object
    let braceCount = 0;
    let jsonStart = -1;
    let jsonEnd = -1;
    let inString = false;
    let escapeNext = false;

    for (let i = 0; i < output.length; i++) {
      const char = output[i];

      // Handle string escaping
      if (escapeNext) {
        escapeNext = false;
        continue;
      }

      if (char === '\\') {
        escapeNext = true;
        continue;
      }

      // Track if we're inside a string (to ignore braces in strings)
      if (char === '"') {
        inString = !inString;
        continue;
      }

      // Only count braces outside of strings
      if (!inString) {
        if (char === '{') {
          if (braceCount === 0) {
            jsonStart = i;
          }
          braceCount++;
        } else if (char === '}') {
          braceCount--;
          if (braceCount === 0 && jsonStart !== -1) {
            jsonEnd = i + 1;
            break;
          }
        }
      }
    }

    if (jsonStart === -1 || jsonEnd === -1) {
      return null;
    }

    const jsonStr = output.substring(jsonStart, jsonEnd);
    return JSON.parse(jsonStr) as T;
  } catch (error) {
    // JSON parsing failed - not valid JSON
    return null;
  }
}

/**
 * Extract JSON and throw error if not found or invalid
 *
 * Use this when JSON is REQUIRED and absence is an error condition.
 *
 * @param output - Raw string output that should contain JSON
 * @param context - Context string for error message (e.g., "ping command")
 * @returns Parsed JSON object
 * @throws Error if JSON not found or invalid
 */
export function extractJSONOrThrow<T = any>(output: string, context: string = 'output'): T {
  const result = extractJSON<T>(output);

  if (result === null) {
    throw new Error(
      `No valid JSON found in ${context}. Output preview: ${output.substring(0, 200)}`
    );
  }

  return result;
}
