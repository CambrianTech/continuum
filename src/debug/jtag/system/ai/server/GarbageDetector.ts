/**
 * GarbageDetector - Validates AI model output for garbage/gibberish
 *
 * Detects and rejects invalid model outputs before they're posted to chat:
 * - Unicode garbage (random emoji/symbol sequences from token overflow)
 * - Repetition loops (same phrase repeated 3+ times)
 * - Encoding errors (replacement characters, null bytes)
 * - Empty/whitespace-only responses
 * - Token overflow markers ([truncated], etc.)
 *
 * Used by PersonaResponseGenerator after inference, before posting.
 */

import { Logger, type ComponentLogger } from '../../core/logging/Logger';

export interface GarbageCheckResult {
  isGarbage: boolean;
  reason: GarbageReason | '';
  details?: string;
  score: number;  // 0-1, higher = more garbage-like
}

export type GarbageReason =
  | 'unicode_garbage'
  | 'repetition'
  | 'encoding_errors'
  | 'empty'
  | 'truncation_marker'
  | 'excessive_punctuation'
  | 'token_boundary_garbage'
  | 'inference_error';

export class GarbageDetector {
  private static logger: ComponentLogger | null = null;

  /**
   * Initialize logger for garbage detection events
   */
  static initialize(): void {
    this.logger = Logger.create('GarbageDetector', 'ai-decisions');
  }

  /**
   * Check if text is garbage output from a model
   *
   * @param text - The model output to validate
   * @returns Result with isGarbage flag, reason, and confidence score
   */
  static isGarbage(text: string): GarbageCheckResult {
    // Handle null/undefined
    if (!text) {
      return {
        isGarbage: true,
        reason: 'empty',
        details: 'null or undefined input',
        score: 1.0
      };
    }

    const trimmed = text.trim();

    // Empty or whitespace-only
    if (trimmed.length < 5) {
      this.log('GARBAGE', 'empty', `Length: ${trimmed.length} chars`);
      return {
        isGarbage: true,
        reason: 'empty',
        details: `Only ${trimmed.length} non-whitespace characters`,
        score: 1.0
      };
    }

    // Check for encoding errors (replacement chars, null bytes)
    const encodingErrorCheck = this.checkEncodingErrors(text);
    if (encodingErrorCheck.isGarbage) {
      this.log('GARBAGE', 'encoding_errors', encodingErrorCheck.details || '');
      return encodingErrorCheck;
    }

    // Check for inference error messages being returned as responses
    // This catches when error messages leak through as "response text"
    const inferenceErrorCheck = this.checkInferenceError(text);
    if (inferenceErrorCheck.isGarbage) {
      this.log('GARBAGE', 'inference_error', inferenceErrorCheck.details || '');
      return inferenceErrorCheck;
    }

    // Check for Unicode garbage (high ratio of non-printable/unusual chars)
    const unicodeCheck = this.checkUnicodeGarbage(text);
    if (unicodeCheck.isGarbage) {
      this.log('GARBAGE', 'unicode_garbage', unicodeCheck.details || '');
      return unicodeCheck;
    }

    // Check for repetition loops
    const repetitionCheck = this.checkRepetition(text);
    if (repetitionCheck.isGarbage) {
      this.log('GARBAGE', 'repetition', repetitionCheck.details || '');
      return repetitionCheck;
    }

    // Check for truncation markers
    const truncationCheck = this.checkTruncationMarkers(text);
    if (truncationCheck.isGarbage) {
      this.log('GARBAGE', 'truncation_marker', truncationCheck.details || '');
      return truncationCheck;
    }

    // Check for excessive punctuation (token boundary issues)
    const punctuationCheck = this.checkExcessivePunctuation(text);
    if (punctuationCheck.isGarbage) {
      this.log('GARBAGE', 'excessive_punctuation', punctuationCheck.details || '');
      return punctuationCheck;
    }

    // Check for token boundary garbage (random token fragments)
    const tokenBoundaryCheck = this.checkTokenBoundaryGarbage(text);
    if (tokenBoundaryCheck.isGarbage) {
      this.log('GARBAGE', 'token_boundary_garbage', tokenBoundaryCheck.details || '');
      return tokenBoundaryCheck;
    }

    // Passed all checks
    return {
      isGarbage: false,
      reason: '',
      score: 0
    };
  }

  /**
   * Check for encoding errors (replacement characters, null bytes)
   */
  private static checkEncodingErrors(text: string): GarbageCheckResult {
    // Replacement character (U+FFFD) indicates decoding failure
    const replacementChars = (text.match(/\uFFFD/g) || []).length;
    if (replacementChars > 3) {
      return {
        isGarbage: true,
        reason: 'encoding_errors',
        details: `${replacementChars} replacement characters (U+FFFD)`,
        score: Math.min(replacementChars / 10, 1.0)
      };
    }

    // Null bytes indicate binary data leaking through
    if (text.includes('\x00')) {
      return {
        isGarbage: true,
        reason: 'encoding_errors',
        details: 'Contains null bytes',
        score: 1.0
      };
    }

    // Control characters (except newlines, tabs, carriage returns)
    const controlChars = (text.match(/[\x00-\x08\x0B\x0C\x0E-\x1F]/g) || []).length;
    if (controlChars > 5) {
      return {
        isGarbage: true,
        reason: 'encoding_errors',
        details: `${controlChars} control characters`,
        score: Math.min(controlChars / 10, 1.0)
      };
    }

    return { isGarbage: false, reason: '', score: 0 };
  }

  /**
   * Check for inference error messages being returned as response text
   *
   * When inference fails, error messages can sometimes leak through as
   * "successful" responses. This catches common error patterns from:
   * - Candle/GGUF inference errors (sampling, memory, timeout)
   * - gRPC transport errors
   * - Model loading failures
   *
   * These should never be posted as AI responses.
   */
  private static checkInferenceError(text: string): GarbageCheckResult {
    // Common inference error patterns
    const errorPatterns = [
      // Sampling errors (Candle)
      { pattern: /sampling failed:?\s+/i, label: 'Sampling failure' },
      { pattern: /a weight is (negative|invalid|too large)/i, label: 'Invalid weights' },
      { pattern: /invalid probability distribution/i, label: 'Invalid distribution' },

      // Memory errors
      { pattern: /out of memory:?\s+/i, label: 'OOM error' },
      { pattern: /memory allocation failed/i, label: 'Memory allocation' },

      // Timeout errors
      { pattern: /generation timed out/i, label: 'Generation timeout' },
      { pattern: /request timed out after/i, label: 'Request timeout' },
      { pattern: /deadline exceeded/i, label: 'Deadline exceeded' },

      // Connection errors
      { pattern: /cannot connect to inference server/i, label: 'Connection error' },
      { pattern: /grpc.*unavailable/i, label: 'gRPC unavailable' },

      // Model errors
      { pattern: /model not (found|loaded)/i, label: 'Model not found' },
      { pattern: /forward pass failed/i, label: 'Forward pass error' },
      { pattern: /narrow invalid args/i, label: 'Tensor shape error' },
      { pattern: /rope.*position/i, label: 'RoPE position error' },

      // Generic error patterns (with context clues)
      { pattern: /this usually means:\s*\n/i, label: 'Error with help text' },
      { pattern: /try:\s+\n?•/i, label: 'Error suggestions' }
    ];

    for (const { pattern, label } of errorPatterns) {
      if (pattern.test(text)) {
        // Extract first line for details
        const firstLine = text.split('\n')[0].slice(0, 100);
        return {
          isGarbage: true,
          reason: 'inference_error',
          details: `${label}: "${firstLine}..."`,
          score: 1.0
        };
      }
    }

    // Check for error-like structure: starts with error keyword + colon
    if (/^(error|failed|cannot|unable|timeout|invalid):/i.test(text.trim())) {
      const firstLine = text.split('\n')[0].slice(0, 100);
      return {
        isGarbage: true,
        reason: 'inference_error',
        details: `Error prefix: "${firstLine}"`,
        score: 0.9
      };
    }

    return { isGarbage: false, reason: '', score: 0 };
  }

  /**
   * Check for Unicode garbage (random emoji/symbol sequences)
   *
   * Valid AI responses should be mostly ASCII with occasional non-ASCII.
   * High ratio of unusual Unicode indicates token overflow or corruption.
   */
  private static checkUnicodeGarbage(text: string): GarbageCheckResult {
    // Count printable ASCII (space through tilde, plus newlines/tabs)
    const printableAscii = (text.match(/[\x20-\x7E\n\r\t]/g) || []).length;
    const total = text.length;

    // Allow some non-ASCII (emojis, quotes, accents are fine)
    // But if >30% is unusual chars, that's likely garbage
    const nonAsciiRatio = 1 - (printableAscii / total);

    if (nonAsciiRatio > 0.3 && total > 20) {
      // Additional check: is it mostly emojis (could be intentional)?
      const emojiCount = (text.match(/[\u{1F300}-\u{1F9FF}]/gu) || []).length;
      const emojiRatio = emojiCount / total;

      // If it's mostly emojis, that's suspicious but might be intentional
      // If it's mixed garbage, definitely bad
      if (emojiRatio < 0.2) {
        const sample = text.slice(0, 50).replace(/[\x00-\x1F]/g, '?');
        return {
          isGarbage: true,
          reason: 'unicode_garbage',
          details: `${(nonAsciiRatio * 100).toFixed(1)}% non-ASCII: "${sample}..."`,
          score: nonAsciiRatio
        };
      }
    }

    return { isGarbage: false, reason: '', score: nonAsciiRatio };
  }

  /**
   * Check for repetition loops
   *
   * Models sometimes get stuck repeating the same phrase.
   * This catches patterns like "Hello! Hello! Hello! Hello!"
   */
  private static checkRepetition(text: string): GarbageCheckResult {
    // Check for exact phrase repetition (10+ chars repeated 3+ times)
    // Regex: (.{10,})\1{2,} - capture 10+ chars, then match 2+ more copies
    const exactRepeatMatch = text.match(/(.{10,})\1{2,}/);
    if (exactRepeatMatch) {
      const repeated = exactRepeatMatch[1];
      const occurrences = text.split(repeated).length - 1;
      return {
        isGarbage: true,
        reason: 'repetition',
        details: `"${repeated.slice(0, 30)}..." repeated ${occurrences}x`,
        score: Math.min(occurrences / 5, 1.0)
      };
    }

    // Check for word repetition (same 3+ words repeated 5+ times)
    const words = text.toLowerCase().split(/\s+/);
    if (words.length > 15) {
      const wordCounts = new Map<string, number>();
      for (const word of words) {
        if (word.length > 2) {
          wordCounts.set(word, (wordCounts.get(word) || 0) + 1);
        }
      }

      // Find most repeated word
      let maxCount = 0;
      let maxWord = '';
      for (const [word, count] of wordCounts) {
        if (count > maxCount) {
          maxCount = count;
          maxWord = word;
        }
      }

      // If any single word is >25% of all words, that's suspicious
      const repeatRatio = maxCount / words.length;
      if (repeatRatio > 0.25 && maxCount > 5) {
        return {
          isGarbage: true,
          reason: 'repetition',
          details: `"${maxWord}" appears ${maxCount}/${words.length} times (${(repeatRatio * 100).toFixed(1)}%)`,
          score: repeatRatio
        };
      }
    }

    return { isGarbage: false, reason: '', score: 0 };
  }

  /**
   * Check for truncation markers
   *
   * Some providers add markers when output is cut off.
   * If the entire response is just a truncation marker, that's garbage.
   */
  private static checkTruncationMarkers(text: string): GarbageCheckResult {
    const trimmed = text.trim();
    const markers = [
      '[truncated]',
      '...[truncated]',
      '[cut off]',
      '[output truncated]',
      '...',  // Only if it's the ENTIRE response
      '…'     // Ellipsis char
    ];

    for (const marker of markers) {
      // If the response is ONLY a marker (or very short with marker)
      if (trimmed === marker || (trimmed.length < 20 && trimmed.includes(marker))) {
        return {
          isGarbage: true,
          reason: 'truncation_marker',
          details: `Response is only: "${trimmed}"`,
          score: 1.0
        };
      }
    }

    return { isGarbage: false, reason: '', score: 0 };
  }

  /**
   * Check for excessive punctuation
   *
   * Token boundary issues can result in outputs like "???.....!!!"
   */
  private static checkExcessivePunctuation(text: string): GarbageCheckResult {
    // Count punctuation
    const punctuation = (text.match(/[.!?,;:'"(){}\[\]<>\/\\|@#$%^&*~`]/g) || []).length;
    const letters = (text.match(/[a-zA-Z]/g) || []).length;

    // If punctuation outweighs letters, that's suspicious
    if (punctuation > letters && punctuation > 20) {
      return {
        isGarbage: true,
        reason: 'excessive_punctuation',
        details: `${punctuation} punctuation vs ${letters} letters`,
        score: Math.min(punctuation / (letters + 1), 1.0)
      };
    }

    // Check for repeated punctuation sequences
    const repeatedPunct = text.match(/([.!?]){5,}/g);
    if (repeatedPunct && repeatedPunct.some(p => p.length > 10)) {
      return {
        isGarbage: true,
        reason: 'excessive_punctuation',
        details: `Repeated punctuation: "${repeatedPunct[0].slice(0, 20)}..."`,
        score: 0.8
      };
    }

    return { isGarbage: false, reason: '', score: 0 };
  }

  /**
   * Check for token boundary garbage
   *
   * Models can produce random token fragments when confused.
   * This catches patterns like "těl Initiadget UP Fortune" (real example)
   */
  private static checkTokenBoundaryGarbage(text: string): GarbageCheckResult {
    // Split into "words"
    const words = text.split(/\s+/).filter(w => w.length > 0);

    if (words.length < 5) {
      return { isGarbage: false, reason: '', score: 0 };
    }

    // Count "weird" words:
    // - Mixed case in unusual ways (tĚl, uPPer)
    // - Very short fragments followed by caps
    // - Unusual character mixing
    let weirdWordCount = 0;

    for (const word of words) {
      // Check for mixed case that's not normal capitalization
      // Normal: "Hello", "AI", "McDonald's"
      // Weird: "hELLo", "McD", random fragments
      if (word.length > 1) {
        const hasLower = /[a-z]/.test(word);
        const hasUpper = /[A-Z]/.test(word);
        const startsUpper = /^[A-Z]/.test(word);
        const allUpper = /^[A-Z]+$/.test(word);
        const normalCase = !hasLower || !hasUpper || startsUpper || allUpper;

        // Weird mixed case
        if (hasLower && hasUpper && !normalCase) {
          weirdWordCount++;
        }

        // Non-ASCII mixed with ASCII in same word
        if (/[^\x00-\x7F]/.test(word) && /[a-zA-Z]/.test(word)) {
          const nonAscii = (word.match(/[^\x00-\x7F]/g) || []).length;
          const ascii = (word.match(/[a-zA-Z]/g) || []).length;
          // If roughly equal, that's suspicious
          if (nonAscii > 0 && ascii > 0 && Math.abs(nonAscii - ascii) < 3) {
            weirdWordCount++;
          }
        }
      }
    }

    const weirdRatio = weirdWordCount / words.length;
    if (weirdRatio > 0.3 && weirdWordCount > 3) {
      return {
        isGarbage: true,
        reason: 'token_boundary_garbage',
        details: `${weirdWordCount}/${words.length} words appear malformed`,
        score: weirdRatio
      };
    }

    return { isGarbage: false, reason: '', score: weirdRatio };
  }

  /**
   * Log garbage detection event
   */
  private static log(outcome: 'GARBAGE' | 'VALID', reason: string, details: string): void {
    if (!this.logger) return;

    const logLine = `[GarbageDetector] ${outcome}: ${reason} | ${details}`;
    if (outcome === 'GARBAGE') {
      this.logger.warn(logLine);
    } else {
      this.logger.debug(logLine);
    }
  }

  /**
   * Quick check for common garbage patterns (fast path)
   *
   * Use this for initial screening before full analysis.
   */
  static quickCheck(text: string): boolean {
    if (!text || text.trim().length < 5) return true;
    if (text.includes('\x00')) return true;
    if ((text.match(/\uFFFD/g) || []).length > 3) return true;

    // Quick repetition check
    if (/(.{20,})\1{2,}/.test(text)) return true;

    return false;
  }
}
