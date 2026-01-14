/**
 * StringUtils - Safe string operations to prevent undefined crashes
 *
 * CLAUDE.md says: "100% of claude fallbacks fire 100% of the time"
 * These utilities provide safe defaults WITHOUT hiding real bugs.
 */

/**
 * Safely get text from a content object, returning empty string if undefined.
 * Use this for ChatMessageEntity.content.text access patterns.
 *
 * @param content - Content object that may have text property
 * @returns The text string or empty string if undefined
 */
export function getTextSafe(content: { text?: string } | undefined | null): string {
  return content?.text ?? '';
}

/**
 * Safely slice text with bounds checking and undefined handling.
 * Prevents "Cannot read properties of undefined (reading 'slice')" errors.
 *
 * @param text - String to slice (can be undefined/null)
 * @param start - Start index (default 0)
 * @param end - End index (optional)
 * @returns Sliced string or empty string if input is undefined
 */
export function sliceSafe(text: string | undefined | null, start = 0, end?: number): string {
  if (text === undefined || text === null) return '';
  return end !== undefined ? text.slice(start, end) : text.slice(start);
}

/**
 * Truncate text to a maximum length with ellipsis.
 * Safely handles undefined/null input.
 *
 * @param text - String to truncate (can be undefined/null)
 * @param maxLength - Maximum length before truncation
 * @param ellipsis - Ellipsis string (default '...')
 * @returns Truncated string with ellipsis if needed
 */
export function truncate(text: string | undefined | null, maxLength: number, ellipsis = '...'): string {
  if (text === undefined || text === null) return '';
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + ellipsis;
}

/**
 * Get preview of content text with truncation.
 * Common pattern: show first N chars of message content.
 *
 * @param content - Content object with optional text
 * @param maxLength - Maximum preview length (default 50)
 * @returns Preview string
 */
export function contentPreview(content: { text?: string } | undefined | null, maxLength = 50): string {
  return truncate(getTextSafe(content), maxLength);
}

/**
 * Extract text from LLM message content that can be either:
 * - A plain string
 * - An object with text property (ChatMessage['content'])
 * - An array of content parts (multimodal)
 *
 * This handles the polymorphic content format used in AI provider messages.
 *
 * @param content - Message content in any format
 * @returns Plain text string
 */
export function getMessageText(content: string | { text?: string } | Array<{ type: string; text?: string }> | undefined | null): string {
  if (content === undefined || content === null) return '';
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    // Multimodal content - extract text from text parts
    return content
      .filter((part): part is { type: string; text: string } => part.type === 'text' && typeof part.text === 'string')
      .map(part => part.text)
      .join(' ');
  }
  if (typeof content === 'object' && 'text' in content) {
    return content.text ?? '';
  }
  return '';
}

/**
 * Get truncated preview of LLM message content.
 * Handles all content formats (string, object, array).
 *
 * @param content - Message content in any format
 * @param maxLength - Maximum length (default 100)
 * @returns Truncated preview string
 */
export function messagePreview(content: string | { text?: string } | Array<{ type: string; text?: string }> | undefined | null, maxLength = 100): string {
  return truncate(getMessageText(content), maxLength);
}
