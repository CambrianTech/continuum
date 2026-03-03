/**
 * GapDetector — Aggregates existing signals to detect skill gaps
 *
 * Reads from three sources that already collect data:
 *   1. RustCognitionBridge.coverageReport() — domains with/without adapters, success/failure
 *   2. TrainingBuffer.getBufferStats() — per-trait correction signal accumulation
 *   3. GenomeLayerEntity fitness (EMA success rate from FitnessTracker)
 *
 * Produces a GapReport: sorted list of DomainGap objects with severity scores
 * and suggested actions (enroll-academy, fine-tune-lora, retrain, none).
 *
 * No new data collection — purely aggregation of existing signals.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { RustCognitionBridge } from './RustCognitionBridge';
import type { TrainingBuffer } from './TrainingBuffer';
import type { CoverageReport } from '../../../../shared/generated/persona/CoverageReport';
import type { DomainActivity } from '../../../../shared/generated/persona/DomainActivity';
import { GenomeLayerEntity } from '../../../genome/entities/GenomeLayerEntity';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';

// ============================================================================
// Types
// ============================================================================

export type SuggestedAction = 'enroll-academy' | 'fine-tune-lora' | 'retrain' | 'none';

/** Evidence for why a gap was detected */
export interface GapSignal {
  source: 'coverage' | 'fitness' | 'corrections' | 'meta-learning';
  description: string;
  value: number;
}

/** Single domain gap assessment */
export interface DomainGap {
  domain: string;
  severity: number;             // 0.0-1.0 weighted composite
  hasAdapter: boolean;
  adapterFitness: number;       // EMA success rate if adapter exists, 0 otherwise
  failureRate: number;          // from Rust DomainActivity
  interactionCount: number;
  correctionCount: number;      // from TrainingBuffer
  suggestedAction: SuggestedAction;
  suggestedMode: 'knowledge' | 'coding';
  signals: GapSignal[];
}

/** Full gap report from a single assessment */
export interface GapReport {
  gaps: DomainGap[];
  assessedAt: number;
  totalDomains: number;
  coverageRatio: number;
}

/** Training history entry for meta-learning */
interface TrainingHistoryEntry {
  domain: string;
  preFitness: number;
  postFitness: number;
  timestamp: number;
  action: SuggestedAction;
}

// ============================================================================
// Constants
// ============================================================================

/** Minimum interactions before a domain without adapter becomes actionable */
const MIN_INTERACTIONS_FOR_ENROLLMENT = 10;

/** Fitness threshold below which an adapter needs retraining */
const FITNESS_RETRAIN_THRESHOLD = 0.4;

/** Fitness band where corrections can improve an adapter */
const FITNESS_FINETUNE_UPPER = 0.6;

/** Fitness above this = adapter is performing well */
const FITNESS_GOOD_THRESHOLD = 0.6;

/** Correction count that triggers severity on its own */
const CORRECTION_SEVERITY_THRESHOLD = 5;

/** Max training history entries per domain */
const MAX_HISTORY_PER_DOMAIN = 10;

/** Failed trainings count that triggers meta-learning escalation */
const META_LEARNING_FAILURE_THRESHOLD = 2;

/** Severity boost when meta-learning detects stalled improvement */
const META_LEARNING_SEVERITY_BOOST = 0.3;

// ============================================================================
// GapDetector
// ============================================================================

export class GapDetector {
  private readonly personaId: UUID;
  private readonly log: (message: string) => void;
  private readonly rustBridge: RustCognitionBridge;
  private readonly trainingBuffer: TrainingBuffer;

  /** Per-domain training history for meta-learning */
  private trainingHistory: Map<string, TrainingHistoryEntry[]> = new Map();

  constructor(
    personaId: UUID,
    rustBridge: RustCognitionBridge,
    trainingBuffer: TrainingBuffer,
    logger: (message: string) => void,
  ) {
    this.personaId = personaId;
    this.rustBridge = rustBridge;
    this.trainingBuffer = trainingBuffer;
    this.log = logger;
  }

  /**
   * Run a full gap assessment across all domains.
   *
   * Aggregates:
   *   - Rust coverage report (domains, interactions, adapters)
   *   - TrainingBuffer stats (correction signal counts)
   *   - GenomeLayerEntity fitness from DB (EMA success rates)
   *
   * Returns gaps sorted by severity (highest first).
   */
  async assess(): Promise<GapReport> {
    // 1. Get coverage report from Rust
    let coverage: CoverageReport;
    try {
      coverage = await this.rustBridge.coverageReport();
    } catch (error) {
      this.log(`⚠️ GapDetector: coverageReport failed: ${error}`);
      return { gaps: [], assessedAt: Date.now(), totalDomains: 0, coverageRatio: 1 };
    }

    // 2. Get correction signal counts from TrainingBuffer
    const bufferStats = this.trainingBuffer.getBufferStats();

    // 3. Load genome layer fitness for this persona's adapters
    const fitnessMap = await this.loadAdapterFitness();

    // 4. Assess each domain
    const gaps: DomainGap[] = [];
    const allDomains = [...coverage.gaps, ...coverage.covered];

    for (const activity of allDomains) {
      const gap = this.assessDomain(activity, bufferStats, fitnessMap);
      if (gap.severity > 0 && gap.suggestedAction !== 'none') {
        gaps.push(gap);
      }
    }

    // Sort by severity descending
    gaps.sort((a, b) => b.severity - a.severity);

    this.log(`🔬 GapDetector: ${gaps.length} gaps from ${allDomains.length} domains (coverage=${(coverage.coverage_ratio * 100).toFixed(0)}%)`);

    return {
      gaps,
      assessedAt: Date.now(),
      totalDomains: allDomains.length,
      coverageRatio: coverage.coverage_ratio,
    };
  }

  /**
   * Record training outcome for meta-learning.
   * Call this after a training task completes to track improvement.
   */
  recordTrainingOutcome(domain: string, preFitness: number, postFitness: number, action: SuggestedAction): void {
    const history = this.trainingHistory.get(domain) ?? [];
    history.push({
      domain,
      preFitness,
      postFitness,
      timestamp: Date.now(),
      action,
    });

    // Keep bounded
    if (history.length > MAX_HISTORY_PER_DOMAIN) {
      history.splice(0, history.length - MAX_HISTORY_PER_DOMAIN);
    }

    this.trainingHistory.set(domain, history);
  }

  // ── Private ──────────────────────────────────────────────────────────

  /**
   * Assess a single domain and produce a DomainGap.
   */
  private assessDomain(
    activity: DomainActivity,
    bufferStats: Record<string, { count: number; oldestAge: number }>,
    fitnessMap: Map<string, number>,
  ): DomainGap {
    const signals: GapSignal[] = [];

    const failureRate = activity.interaction_count > 0
      ? activity.failure_count / activity.interaction_count
      : 0;

    const correctionCount = bufferStats[activity.domain]?.count ?? 0;
    const adapterFitness = fitnessMap.get(activity.domain) ?? 0;

    let severity: number;
    let suggestedAction: SuggestedAction;
    let suggestedMode: 'knowledge' | 'coding' = 'knowledge';

    if (!activity.has_adapter && activity.interaction_count >= MIN_INTERACTIONS_FOR_ENROLLMENT) {
      // Case 1: No adapter, has meaningful activity
      severity = 0.3 + (failureRate * 0.4) + Math.min(activity.interaction_count / 50, 0.3);

      signals.push({
        source: 'coverage',
        description: `No adapter for ${activity.domain} with ${activity.interaction_count} interactions`,
        value: severity,
      });

      if (failureRate > 0) {
        signals.push({
          source: 'coverage',
          description: `${(failureRate * 100).toFixed(0)}% failure rate`,
          value: failureRate,
        });
      }

      suggestedAction = 'enroll-academy';
      suggestedMode = this.inferDomainMode(activity.domain);

    } else if (activity.has_adapter && adapterFitness < FITNESS_RETRAIN_THRESHOLD) {
      // Case 2: Adapter exists but performing poorly — full retrain
      severity = (1 - adapterFitness) * 0.7 + Math.min(correctionCount / CORRECTION_SEVERITY_THRESHOLD, 1.0) * 0.3;

      signals.push({
        source: 'fitness',
        description: `Adapter fitness ${(adapterFitness * 100).toFixed(0)}% below retrain threshold`,
        value: 1 - adapterFitness,
      });

      suggestedAction = 'retrain';
      suggestedMode = this.inferDomainMode(activity.domain);

    } else if (activity.has_adapter && adapterFitness < FITNESS_FINETUNE_UPPER && correctionCount > 0) {
      // Case 3: Adapter in mid-range fitness with correction signals — fine-tune
      severity = (1 - adapterFitness) * 0.5 + Math.min(correctionCount / CORRECTION_SEVERITY_THRESHOLD, 1.0) * 0.5;

      signals.push({
        source: 'fitness',
        description: `Adapter fitness ${(adapterFitness * 100).toFixed(0)}% with ${correctionCount} corrections`,
        value: 1 - adapterFitness,
      });
      signals.push({
        source: 'corrections',
        description: `${correctionCount} correction signals accumulated`,
        value: correctionCount / CORRECTION_SEVERITY_THRESHOLD,
      });

      suggestedAction = 'fine-tune-lora';

    } else if (correctionCount >= CORRECTION_SEVERITY_THRESHOLD) {
      // Case 4: Correction accumulation only (no other strong signal)
      severity = Math.min(correctionCount / CORRECTION_SEVERITY_THRESHOLD, 1.0) * 0.5;

      signals.push({
        source: 'corrections',
        description: `${correctionCount} corrections accumulated (threshold=${CORRECTION_SEVERITY_THRESHOLD})`,
        value: correctionCount / CORRECTION_SEVERITY_THRESHOLD,
      });

      suggestedAction = activity.has_adapter ? 'fine-tune-lora' : 'enroll-academy';

    } else {
      // Domain is performing well or has insufficient data
      severity = 0;
      suggestedAction = 'none';
    }

    // Meta-learning: check if previous trainings failed to improve
    const metaBoost = this.checkMetaLearning(activity.domain, suggestedAction, suggestedMode);
    if (metaBoost.boosted) {
      severity = Math.min(severity + META_LEARNING_SEVERITY_BOOST, 1.0);
      suggestedAction = metaBoost.escalatedAction;
      suggestedMode = metaBoost.escalatedMode;

      signals.push({
        source: 'meta-learning',
        description: `${metaBoost.failedAttempts} recent trainings didn't improve fitness — escalated`,
        value: META_LEARNING_SEVERITY_BOOST,
      });
    }

    return {
      domain: activity.domain,
      severity: Math.min(severity, 1.0),
      hasAdapter: activity.has_adapter,
      adapterFitness,
      failureRate,
      interactionCount: activity.interaction_count,
      correctionCount,
      suggestedAction,
      suggestedMode,
      signals,
    };
  }

  /**
   * Check meta-learning: if 2+ recent trainings didn't improve fitness,
   * boost severity and switch training mode.
   */
  private checkMetaLearning(
    domain: string,
    currentAction: SuggestedAction,
    currentMode: 'knowledge' | 'coding',
  ): {
    boosted: boolean;
    failedAttempts: number;
    escalatedAction: SuggestedAction;
    escalatedMode: 'knowledge' | 'coding';
  } {
    const history = this.trainingHistory.get(domain);
    if (!history || history.length < META_LEARNING_FAILURE_THRESHOLD) {
      return { boosted: false, failedAttempts: 0, escalatedAction: currentAction, escalatedMode: currentMode };
    }

    // Count recent trainings that didn't improve fitness (postFitness <= preFitness)
    const recentHistory = history.slice(-META_LEARNING_FAILURE_THRESHOLD);
    const failedAttempts = recentHistory.filter(h => h.postFitness <= h.preFitness).length;

    if (failedAttempts >= META_LEARNING_FAILURE_THRESHOLD) {
      // Switch mode: if knowledge isn't working, try coding (and vice versa)
      const escalatedMode = currentMode === 'knowledge' ? 'coding' : 'knowledge';

      // Escalate action: if fine-tuning isn't working, try academy (full retrain)
      const escalatedAction: SuggestedAction = currentAction === 'fine-tune-lora' ? 'retrain' : currentAction;

      return { boosted: true, failedAttempts, escalatedAction, escalatedMode };
    }

    return { boosted: false, failedAttempts, escalatedAction: currentAction, escalatedMode: currentMode };
  }

  /**
   * Load adapter fitness values for this persona's genome layers.
   * Returns a map of domain/traitType → fitness.successRate.
   */
  private async loadAdapterFitness(): Promise<Map<string, number>> {
    const fitnessMap = new Map<string, number>();

    try {
      const result = await ORM.query<GenomeLayerEntity>({
        collection: GenomeLayerEntity.collection,
        filter: { creatorId: this.personaId },
        limit: 100,
      }, 'default');

      const layers = result.data ?? [];
      for (const record of layers) {
        const layer = record.data;
        if (layer.traitType && layer.fitness) {
          fitnessMap.set(layer.traitType, layer.fitness.successRate);
        }
      }
    } catch (error) {
      this.log(`⚠️ GapDetector: Failed to load adapter fitness: ${error}`);
    }

    return fitnessMap;
  }

  /**
   * Infer whether a domain is better served by 'knowledge' or 'coding' academy mode.
   */
  private inferDomainMode(domain: string): 'knowledge' | 'coding' {
    const codingDomains = ['code', 'typescript', 'rust', 'python', 'javascript', 'debugging', 'refactoring'];
    return codingDomains.some(d => domain.toLowerCase().includes(d)) ? 'coding' : 'knowledge';
  }
}
