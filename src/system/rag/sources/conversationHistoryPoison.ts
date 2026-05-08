// Patterns for detecting generated chat artifacts that poison future RAG turns.
// Keep this file pure: no ORM, logger, or server imports, so it can be tested
// without booting the Continuum runtime.

// Full date + time at line start
const FABRICATED_DATE_RE = /^\s*\d{1,4}[/-]\d{1,2}[/-]\d{1,4}\s+\d{1,2}:\d{2}\s+[A-Z]/gm;
// Bracketed time at line start: [02:01], [14:30], etc.
const FABRICATED_BRACKET_TIME_RE = /^\s*\[\d{1,2}:\d{2}\]\s+[A-Z]/gm;
// Multi-word speaker prefix: "Teacher AI:", "Helper AI:", "CodeReview AI:"
const FABRICATED_SPEAKER_RE = /^[A-Z][a-zA-Z]+\s+[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*:\s+\S/gm;
// Single-word known AI speaker prefix: "Gemini:", "Groq:", etc.
const FABRICATED_SINGLE_SPEAKER_RE = /^(?:Gemini|Groq|Together|Fireworks|Claude|GPT|Local|Joel|Anonymous|Qwen|DeepSeek|Grok|Candle|Helper|Teacher|CodeReview):\s+\S/gm;

// Persona meta-summary pattern observed during startup smoke tests.
const META_SUMMARY_ECHO_RE = /\bI received a message from\s+[A-Z][\w -]{1,80}:\s*["“][\s\S]{10,}["”][\s\S]{0,800}\b(?:This indicates|The key pattern here|successfully acknowledged|responded to the startup smoke test)\b/i;

export type ConversationHistoryPoisonReason = 'fabricated-conversation' | 'meta-summary-echo';

/**
 * Check if a message body is a fabricated multi-party conversation.
 * Returns true if the message contains 3+ timestamped lines,
 * 4+ multi-word speaker prefixes with 2+ distinct names, or
 * 3+ single-word known AI speaker prefixes.
 */
export function isFabricatedConversation(text: string): boolean {
  if (!text || text.length < 60) return false;

  const dateMatches = text.match(FABRICATED_DATE_RE);
  if (dateMatches && dateMatches.length >= 3) return true;

  const bracketMatches = text.match(FABRICATED_BRACKET_TIME_RE);
  if (bracketMatches && bracketMatches.length >= 3) return true;

  const speakerMatches = text.match(FABRICATED_SPEAKER_RE);
  if (speakerMatches && speakerMatches.length >= 4) {
    const names = new Set(speakerMatches.map(m => m.split(':')[0].trim()));
    if (names.size >= 2) return true;
  }

  const singleMatches = text.match(FABRICATED_SINGLE_SPEAKER_RE);
  if (singleMatches && singleMatches.length >= 3) {
    const names = new Set(singleMatches.map(m => m.split(':')[0].trim()));
    if (names.size >= 2) return true;
  }

  return false;
}

export function isMetaSummaryEcho(text: string): boolean {
  if (!text || text.length < 80) return false;
  return META_SUMMARY_ECHO_RE.test(text);
}

export function detectConversationHistoryPoison(text: string): ConversationHistoryPoisonReason | null {
  if (isFabricatedConversation(text)) return 'fabricated-conversation';
  if (isMetaSummaryEcho(text)) return 'meta-summary-echo';
  return null;
}
