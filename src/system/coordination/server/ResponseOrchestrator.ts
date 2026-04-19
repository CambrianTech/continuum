/**
 * ResponseOrchestrator — A.2 of the Shared Cognition pipeline
 *
 * Takes a SharedAnalysis + the room's personas, produces a ResponderDecision
 * for each persona. Decides WHO responds based on specialty match against
 * the analysis's suggestedAngles.
 *
 * Not a cap — a relevance filter. If 5 specialists each have something
 * additive, all 5 contribute. The filter says "stay silent when you're
 * not adding signal," not "stay silent because we hit a number."
 *
 * The personas themselves can override these decisions via lever calls
 * (cedeFloorTo, claimLead, inviteSpecialist, etc.) — this module provides
 * the defaults that fire when no lever is pulled.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type {
  SharedAnalysis,
  SharedAnalysisIntent,
  ResponderDecision,
} from '../shared/SharedCognitionTypes';

/**
 * Persona info needed for specialty matching.
 * Kept minimal — the orchestrator doesn't need the full PersonaUser.
 */
export interface OrchestrablePersona {
  personaId: UUID;
  displayName: string;
  /** The persona's specialty domain (e.g. 'code', 'teaching', 'general', 'analysis'). */
  specialty: string;
  /** Whether this persona uses local inference (affects slot contention awareness). */
  isLocal: boolean;
}

/**
 * Orchestration configuration — tunables that control how aggressively
 * the filter limits responders. These are defaults; lever calls override.
 */
export interface OrchestratorConfig {
  /**
   * Minimum relevance score (0.0-1.0) for a persona to respond.
   * Below this threshold, the persona stays silent with a reason.
   * Default: 0.3 (low bar — specialty just needs SOME relevance).
   */
  relevanceThreshold: number;

  /**
   * For 'social' intent messages (greetings, acknowledgments), max
   * responders. Most social messages only need 1-2 voices, not 14.
   * Default: 2.
   */
  maxSocialResponders: number;
}

const DEFAULT_CONFIG: OrchestratorConfig = {
  relevanceThreshold: 0.3,
  maxSocialResponders: 2,
};

/**
 * Pick which personas respond to a message given the shared analysis.
 *
 * Pure function — no side effects, no IPC, no state. Takes data in,
 * produces decisions out. Testable in isolation.
 */
export function pickResponders(
  analysis: SharedAnalysis,
  personas: OrchestrablePersona[],
  config: OrchestratorConfig = DEFAULT_CONFIG
): ResponderDecision[] {
  const decisions: ResponderDecision[] = [];

  for (const persona of personas) {
    const decision = evaluatePersona(analysis, persona, config);
    decisions.push(decision);
  }

  // For social messages, cap responders to avoid 14 personas saying "hi"
  if (analysis.intent === 'social') {
    return capSocialResponders(decisions, config.maxSocialResponders);
  }

  return decisions;
}

/**
 * Evaluate a single persona against the shared analysis.
 */
function evaluatePersona(
  analysis: SharedAnalysis,
  persona: OrchestrablePersona,
  config: OrchestratorConfig
): ResponderDecision {
  // Match persona's specialty against suggestedAngles
  const matchedAngles: string[] = [];
  let relevanceScore = 0;

  for (const [angleKey, angleValue] of Object.entries(analysis.suggestedAngles)) {
    if (!angleValue) continue; // Empty = no signal for this specialty

    // Match specialty to angle key (fuzzy — 'code' matches 'code-review', etc.)
    if (specialtyMatchesAngle(persona.specialty, angleKey)) {
      matchedAngles.push(angleKey);
      // Score based on how much content the angle has (more = more relevant)
      relevanceScore = Math.max(relevanceScore, Math.min(1.0, angleValue.length / 100));
    }
  }

  // 'general' specialty always gets a base relevance for non-trivial messages
  if (persona.specialty === 'general' && analysis.intent !== 'social') {
    relevanceScore = Math.max(relevanceScore, 0.4);
    if (matchedAngles.length === 0 && analysis.suggestedAngles['general']) {
      matchedAngles.push('general');
    }
  }

  // Threshold check
  const shouldRespond = relevanceScore >= config.relevanceThreshold && matchedAngles.length > 0;

  const explanation = shouldRespond
    ? `Specialty '${persona.specialty}' matches angles [${matchedAngles.join(', ')}] (relevance=${relevanceScore.toFixed(2)})`
    : relevanceScore < config.relevanceThreshold
      ? `Relevance too low (${relevanceScore.toFixed(2)} < ${config.relevanceThreshold}) — nothing additive from '${persona.specialty}'`
      : `No matching angles for specialty '${persona.specialty}'`;

  return {
    personaId: persona.personaId,
    shouldRespond,
    relevanceScore,
    matchedAngles,
    explanation,
  };
}

/**
 * For social messages, keep only the top N by relevance. The rest get
 * shouldRespond=false with a social-cap explanation.
 */
function capSocialResponders(
  decisions: ResponderDecision[],
  maxResponders: number
): ResponderDecision[] {
  const responding = decisions.filter(d => d.shouldRespond);
  if (responding.length <= maxResponders) return decisions;

  // Sort by relevance descending, keep top N
  responding.sort((a, b) => b.relevanceScore - a.relevanceScore);
  const kept = new Set(responding.slice(0, maxResponders).map(d => d.personaId));

  return decisions.map(d => {
    if (d.shouldRespond && !kept.has(d.personaId)) {
      return {
        ...d,
        shouldRespond: false,
        explanation: `Social message — capped at ${maxResponders} responders (relevance=${d.relevanceScore.toFixed(2)}, outranked)`,
      };
    }
    return d;
  });
}

/**
 * Fuzzy match between a persona's specialty and an angle key.
 * 'code' matches 'code-review', 'code-quality', etc.
 * 'teaching' matches 'education', 'explanation', etc.
 */
function specialtyMatchesAngle(specialty: string, angleKey: string): boolean {
  const s = specialty.toLowerCase();
  const a = angleKey.toLowerCase();

  // Exact match
  if (s === a) return true;

  // Containment (either direction)
  if (s.includes(a) || a.includes(s)) return true;

  // Known synonym mappings
  const synonyms: Record<string, string[]> = {
    'code': ['code-review', 'programming', 'engineering', 'software', 'debugging', 'architecture'],
    'teaching': ['education', 'explanation', 'tutorial', 'learning', 'pedagogy'],
    'analysis': ['review', 'evaluation', 'assessment', 'critique'],
    'general': ['broad', 'versatile', 'multi-purpose'],
    'creative': ['writing', 'storytelling', 'content'],
  };

  const specSynonyms = synonyms[s] || [];
  if (specSynonyms.some(syn => a.includes(syn) || syn.includes(a))) return true;

  // Reverse: check if angle has synonyms that match specialty
  for (const [key, syns] of Object.entries(synonyms)) {
    if (a.includes(key) || key.includes(a)) {
      if (syns.some(syn => s.includes(syn) || syn.includes(s))) return true;
    }
  }

  return false;
}
