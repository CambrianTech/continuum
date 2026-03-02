/**
 * TextSentiment - Fast, deterministic sentiment and gesture extraction from text.
 *
 * TypeScript mirror of the Rust implementation in:
 *   workers/continuum-core/src/live/session/sentiment.rs
 *
 * Extracts emotional tone and body gesture cues via pattern matching.
 * Sub-millisecond execution — safe for the RAG hot path.
 * No ML model required; runs synchronously inline.
 *
 * Emotion priority order (first match wins):
 * 1. Emoji (highest signal): happy, sad, angry, surprised, relaxed
 * 2. Punctuation patterns: !!!->excited, ...->contemplative, ?!->surprised
 * 3. Keywords: "wonderful"->Happy, "sorry"->Sad, "wow"->Surprised
 *
 * Gesture extraction runs independently (scans full text for gesture keywords).
 *
 * IMPORTANT: Keep in sync with the Rust version. Both must produce identical
 * results for identical input — Rust drives avatar expressions, TypeScript
 * drives RAG context annotations.
 */

export type Emotion = 'neutral' | 'happy' | 'sad' | 'angry' | 'surprised' | 'relaxed';
export type Gesture = 'none' | 'wave' | 'think' | 'nod' | 'shrug' | 'point' | 'openHands';

export interface SentimentResult {
  readonly emotion: Emotion;
  readonly intensity: number;  // 0.0-1.0
  readonly gesture: Gesture;
}

// --- Emoji maps ---

const HAPPY_EMOJI = new Set([
  '😊', '😄', '😃', '😁', '🥰', '😍', '🤗', '💕', '❤', '♥',
  '😀', '🙂', '☺', '💖', '💗', '😻', '🎉', '🥳',
]);
const SAD_EMOJI = new Set(['😢', '😭', '😞', '😔', '🥺', '😿', '💔', '😥', '🙁', '☹']);
const ANGRY_EMOJI = new Set(['😠', '😡', '🤬', '💢', '👿', '😤']);
const SURPRISED_EMOJI = new Set(['😮', '😲', '🤯', '😱', '😳', '🫢', '❗', '‼', '⁉']);
const RELAXED_EMOJI = new Set(['😌', '😴', '🧘', '☮', '🍃', '✨', '🌸']);

// Gesture emoji (override emotion + gesture)
const WAVE_EMOJI = new Set(['👋']);
const THINK_EMOJI = new Set(['🤔']);
const SHRUG_EMOJI = new Set(['🤷']);
const POINT_EMOJI = new Set(['👉', '☝', '👆']);

// --- Keyword lists (match Rust exactly) ---

const HAPPY_STRONG = [
  'wonderful', 'fantastic', 'amazing', 'love it', 'brilliant',
  'delighted', 'thrilled', 'overjoyed', 'ecstatic',
];
const HAPPY_MILD = [
  'nice', 'good', 'great', 'happy', 'glad', 'pleased',
  'enjoy', 'lovely', 'beautiful', 'excellent',
];
const SAD_STRONG = [
  'heartbroken', 'devastated', 'terrible', 'awful', 'tragic',
  'miserable', 'hopeless',
];
const SAD_MILD = [
  'sorry', 'unfortunately', 'sadly', 'disappointed', 'regret',
  'miss', 'lonely', 'upset',
];
const ANGRY = [
  'furious', 'outraged', 'infuriating', 'ridiculous', 'unacceptable',
  'frustrated', 'annoyed', 'irritated',
];
const SURPRISED = [
  'wow', 'incredible', 'unbelievable', 'astonishing', 'shocking',
  'whoa', 'oh my', "can't believe", 'no way', 'mind-blowing',
];
const RELAXED = [
  'peaceful', 'calm', 'serene', 'tranquil', 'soothing',
  'gentle', 'mellow', 'at ease',
];

// --- Gesture keyword lists ---

const WAVE_KEYWORDS = [
  'hello', 'hi everyone', 'hey everyone', 'bye', 'goodbye',
  'good morning', 'good evening', 'welcome', 'farewell',
  'hi there', 'greetings',
];
const THINK_KEYWORDS = [
  'hmm', 'let me think', 'i wonder', 'perhaps', 'considering',
  'interesting question', "that's a good point", 'let me consider',
  'pondering', 'contemplating',
];
const NOD_KEYWORDS = [
  'absolutely', 'exactly', 'definitely', 'i agree', "that's right",
  'precisely', 'indeed', 'certainly', 'of course', 'without a doubt',
];
const SHRUG_KEYWORDS = [
  'not sure', "i don't know", 'maybe', 'who knows', 'hard to say',
  'it depends', 'uncertain', 'either way', "it's debatable",
];
const OPENHANDS_KEYWORDS = [
  "here's the thing", 'so basically', 'let me explain',
  'the way i see it', 'in other words', 'to put it simply',
  'what i mean is', 'the key insight',
];
const POINT_KEYWORDS = [
  'look at this', 'right there', 'check this out', 'notice how',
  'specifically', 'in particular', 'the important part',
  'pay attention to',
];

function scanEmoji(text: string): SentimentResult | null {
  for (const ch of text) {
    if (HAPPY_EMOJI.has(ch)) return { emotion: 'happy', intensity: 0.8, gesture: 'none' };
    if (SAD_EMOJI.has(ch)) return { emotion: 'sad', intensity: 0.7, gesture: 'none' };
    if (ANGRY_EMOJI.has(ch)) return { emotion: 'angry', intensity: 0.7, gesture: 'none' };
    if (SURPRISED_EMOJI.has(ch)) return { emotion: 'surprised', intensity: 0.8, gesture: 'none' };
    if (RELAXED_EMOJI.has(ch)) return { emotion: 'relaxed', intensity: 0.6, gesture: 'none' };
    if (WAVE_EMOJI.has(ch)) return { emotion: 'happy', intensity: 0.6, gesture: 'wave' };
    if (THINK_EMOJI.has(ch)) return { emotion: 'neutral', intensity: 0.3, gesture: 'think' };
    if (SHRUG_EMOJI.has(ch)) return { emotion: 'neutral', intensity: 0.3, gesture: 'shrug' };
    if (POINT_EMOJI.has(ch)) return { emotion: 'neutral', intensity: 0.3, gesture: 'point' };
  }
  return null;
}

function scanPunctuation(text: string): SentimentResult | null {
  const trimmed = text.trim();

  // Count trailing exclamation marks
  let trailingExcl = 0;
  for (let i = trimmed.length - 1; i >= 0 && trimmed[i] === '!'; i--) {
    trailingExcl++;
  }
  if (trailingExcl >= 3) return { emotion: 'surprised', intensity: 0.6, gesture: 'none' };
  if (trailingExcl >= 2) return { emotion: 'happy', intensity: 0.5, gesture: 'none' };

  if (trimmed.includes('?!') || trimmed.includes('!?')) {
    return { emotion: 'surprised', intensity: 0.6, gesture: 'none' };
  }

  if (trimmed.endsWith('...') || trimmed.endsWith('\u2026')) {
    return { emotion: 'sad', intensity: 0.3, gesture: 'none' };
  }

  return null;
}

function containsAny(lower: string, keywords: string[]): boolean {
  for (const kw of keywords) {
    if (lower.includes(kw)) return true;
  }
  return false;
}

function scanKeywords(text: string): SentimentResult | null {
  const lower = text.toLowerCase();

  // Priority order matches Rust: strong emotions first, then mild
  if (containsAny(lower, HAPPY_STRONG)) return { emotion: 'happy', intensity: 0.7, gesture: 'none' };
  if (containsAny(lower, SAD_STRONG)) return { emotion: 'sad', intensity: 0.7, gesture: 'none' };
  if (containsAny(lower, ANGRY)) return { emotion: 'angry', intensity: 0.6, gesture: 'none' };
  if (containsAny(lower, SURPRISED)) return { emotion: 'surprised', intensity: 0.7, gesture: 'none' };
  if (containsAny(lower, RELAXED)) return { emotion: 'relaxed', intensity: 0.5, gesture: 'none' };
  if (containsAny(lower, HAPPY_MILD)) return { emotion: 'happy', intensity: 0.5, gesture: 'none' };
  if (containsAny(lower, SAD_MILD)) return { emotion: 'sad', intensity: 0.4, gesture: 'none' };

  return null;
}

function detectGesture(text: string): Gesture {
  const lower = text.toLowerCase();

  // Priority order matches Rust
  for (const kw of WAVE_KEYWORDS) { if (lower.includes(kw)) return 'wave'; }
  for (const kw of THINK_KEYWORDS) { if (lower.includes(kw)) return 'think'; }
  for (const kw of NOD_KEYWORDS) { if (lower.includes(kw)) return 'nod'; }
  for (const kw of SHRUG_KEYWORDS) { if (lower.includes(kw)) return 'shrug'; }
  for (const kw of OPENHANDS_KEYWORDS) { if (lower.includes(kw)) return 'openHands'; }
  for (const kw of POINT_KEYWORDS) { if (lower.includes(kw)) return 'point'; }

  return 'none';
}

/**
 * Extract sentiment and gesture cues from text.
 * Returns the dominant emotion, intensity, and body gesture.
 *
 * Deterministic: same input always produces same output.
 * Matches the Rust implementation exactly.
 */
export function extractSentiment(text: string): SentimentResult {
  // Emotion detection (priority: emoji > punctuation > keywords)
  const emojiResult = scanEmoji(text);
  const punctResult = emojiResult ? null : scanPunctuation(text);
  const keywordResult = (emojiResult || punctResult) ? null : scanKeywords(text);

  const base = emojiResult ?? punctResult ?? keywordResult ?? {
    emotion: 'neutral' as Emotion,
    intensity: 0,
    gesture: 'none' as Gesture,
  };

  // Gesture detection — only if emoji didn't already set one
  if (base.gesture !== 'none') return base;

  const gesture = detectGesture(text);
  if (gesture === 'none') return base;

  return { ...base, gesture };
}

/**
 * Format emotion as a human-readable label for RAG context.
 * Returns null for neutral (no annotation needed).
 */
export function formatEmotionLabel(result: SentimentResult): string | null {
  if (result.emotion === 'neutral' && result.gesture === 'none') return null;

  const parts: string[] = [];

  if (result.emotion !== 'neutral') {
    parts.push(result.emotion);
  }

  if (result.gesture !== 'none') {
    const gestureLabels: Record<Gesture, string> = {
      none: '',
      wave: 'waving',
      think: 'thinking',
      nod: 'nodding',
      shrug: 'shrugging',
      point: 'pointing',
      openHands: 'explaining with open hands',
    };
    parts.push(gestureLabels[result.gesture]);
  }

  return parts.join(', ');
}
