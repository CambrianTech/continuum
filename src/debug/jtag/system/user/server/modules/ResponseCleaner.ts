/**
 * ResponseCleaner - Cleans AI-generated responses before posting
 *
 * Problem: LLMs sometimes copy formatting from conversation history,
 * adding unwanted prefixes like "[HH:MM] Name: " to their responses.
 *
 * Solution: Strip these patterns using regex heuristics.
 *
 * Future: Could become AI-powered via ThoughtStream adapter
 * - An AI evaluates: "Does this response have formatting issues?"
 * - Returns cleaned version with confidence score
 * - Pluggable via recipe configuration
 */

/**
 * Configuration for ResponseCleaner
 */
export interface ResponseCleanerConfig {
  /** Logger function for debug output */
  log?: (message: string) => void;
}

/**
 * ResponseCleaner - Strips unwanted prefixes from AI responses
 *
 * Usage:
 * ```typescript
 * const cleaner = new ResponseCleaner({ log: this.log.bind(this) });
 * const cleaned = cleaner.clean(aiResponse.text);
 * ```
 */
export class ResponseCleaner {
  private log: (message: string) => void;

  constructor(config: ResponseCleanerConfig = {}) {
    this.log = config.log ?? (() => {});
  }

  /**
   * Clean AI response by stripping unwanted prefixes
   *
   * Examples to strip:
   * - "[11:59] GPT Assistant: Yes, Joel..." → "Yes, Joel..."
   * - "GPT Assistant: Yes, Joel..." → "Yes, Joel..."
   * - "[11:59] Yes, Joel..." → "Yes, Joel..."
   *
   * @param response - Raw AI response
   * @returns Cleaned response with prefixes removed
   */
  clean(response: string): string {
    const original = response.trim();
    let cleaned = original;

    // Pattern 1: Strip "[HH:MM] Name: " prefix
    // Matches: [11:59] GPT Assistant: message
    cleaned = cleaned.replace(/^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*/, '');

    // Pattern 2: Strip "Name: " prefix at start
    // Matches: GPT Assistant: message
    // Only if it looks like a name (starts with capital, contains letters/spaces)
    cleaned = cleaned.replace(/^[A-Z][A-Za-z\s]+:\s*/, '');

    // Pattern 3: Strip just "[HH:MM] " timestamp prefix
    // Matches: [11:59] message
    cleaned = cleaned.replace(/^\[\d{1,2}:\d{2}\]\s*/, '');

    // Pattern 4: Strip markdown role markers some models add
    // Matches: **Assistant:** or *Assistant:* at start
    cleaned = cleaned.replace(/^\*{1,2}[A-Za-z\s]+:\*{1,2}\s*/, '');

    // Log if we cleaned anything
    if (cleaned !== original) {
      this.log(`🧹 Stripped prefix from AI response`);
      this.log(`   Original: "${original.slice(0, 80)}..."`);
      this.log(`   Cleaned:  "${cleaned.slice(0, 80)}..."`);
    }

    return cleaned.trim();
  }

  /**
   * Check if response appears to have a prefix that needs cleaning
   * Useful for metrics/monitoring
   */
  hasPrefix(response: string): boolean {
    const trimmed = response.trim();
    return (
      /^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*/.test(trimmed) ||
      /^[A-Z][A-Za-z\s]+:\s*/.test(trimmed) ||
      /^\[\d{1,2}:\d{2}\]\s*/.test(trimmed) ||
      /^\*{1,2}[A-Za-z\s]+:\*{1,2}\s*/.test(trimmed)
    );
  }
}
