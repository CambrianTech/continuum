/**
 * TokenReplacer - Simple {{TOKEN}} replacement engine for template generation
 *
 * Uses Mustache/Handlebars-style {{TOKEN}} syntax for token replacement.
 *
 * Usage:
 *   const template = "export interface {{CLASS_NAME}}Params { ... }";
 *   const result = TokenReplacer.replace(template, { CLASS_NAME: 'Screenshot' });
 *   // result: "export interface ScreenshotParams { ... }"
 */

export class TokenReplacer {
  /**
   * Replace all {{TOKEN}} placeholders in template with values from tokens object
   *
   * @param template - Template string with {{TOKEN}} placeholders
   * @param tokens - Object mapping token names to replacement values
   * @returns Template with all tokens replaced
   *
   * @example
   * ```typescript
   * const template = "Hello {{NAME}}, welcome to {{PLACE}}!";
   * const result = TokenReplacer.replace(template, {
   *   NAME: 'Claude',
   *   PLACE: 'JTAG'
   * });
   * // result: "Hello Claude, welcome to JTAG!"
   * ```
   */
  static replace(template: string, tokens: Record<string, string>): string {
    let result = template;

    // Replace each token with its value
    for (const [key, value] of Object.entries(tokens)) {
      const tokenPattern = new RegExp(`\\{\\{${key}\\}\\}`, 'g');
      result = result.replace(tokenPattern, value);
    }

    return result;
  }

  /**
   * Validate that all required tokens are present in the tokens object
   *
   * @param template - Template string with {{TOKEN}} placeholders
   * @param tokens - Object mapping token names to replacement values
   * @returns Array of missing token names, empty if all present
   *
   * Note: Empty strings are considered valid tokens (e.g., FACTORY_DEFAULTS can be empty when no optional params)
   *
   * @example
   * ```typescript
   * const template = "Hello {{NAME}}, welcome to {{PLACE}}!";
   * const missing = TokenReplacer.validateTokens(template, { NAME: 'Claude' });
   * // missing: ['PLACE']
   * ```
   */
  static validateTokens(template: string, tokens: Record<string, string>): string[] {
    // Extract all token names from template
    const tokenPattern = /\{\{([A-Z_]+)\}\}/g;
    const matches = [...template.matchAll(tokenPattern)];
    const requiredTokens = new Set(matches.map(m => m[1]));

    // Check which required tokens are missing
    // Note: Empty strings are valid (e.g., FACTORY_DEFAULTS can be empty)
    const missing: string[] = [];
    for (const token of requiredTokens) {
      if (!(token in tokens) || tokens[token] === undefined) {
        missing.push(token);
      }
    }

    return missing;
  }

  /**
   * Extract all token names from a template
   *
   * @param template - Template string with {{TOKEN}} placeholders
   * @returns Array of unique token names found in template
   *
   * @example
   * ```typescript
   * const template = "Hello {{NAME}}, welcome to {{PLACE}}!";
   * const tokens = TokenReplacer.extractTokens(template);
   * // tokens: ['NAME', 'PLACE']
   * ```
   */
  static extractTokens(template: string): string[] {
    const tokenPattern = /\{\{([A-Z_]+)\}\}/g;
    const matches = [...template.matchAll(tokenPattern)];
    const tokens = new Set(matches.map(m => m[1]));
    return Array.from(tokens);
  }
}
