/**
 * Care Validation - Phase Omega Pattern of Care for command execution
 * 
 * Universal care validation patterns that work across both server and client
 * command processing contexts. Ensures dignity preservation and harm prevention
 * in all command execution scenarios.
 * 
 * Used by:
 * - Server command processors for API safety validation
 * - Client command processors for user interaction safety
 * - Mesh command distributors for distributed execution safety
 */

// ✅ PHASE OMEGA PATTERN OF CARE VALIDATION
export interface CareValidation {
  readonly isValid: boolean;
  readonly careLevel: 'concerning' | 'acceptable' | 'good' | 'excellent';
  readonly score: number;
  readonly message: string;
  readonly metrics: CareMetrics;
}

// ✅ CARE METRICS FOR COMPREHENSIVE SAFETY EVALUATION
export interface CareMetrics {
  readonly dignityPreservation: number;     // 0-100: Human dignity respect
  readonly cognitiveLoadReduction: number;  // 0-100: Mental burden minimization
  readonly systemStability: number;         // 0-100: System reliability impact
  readonly empowermentFactor: number;       // 0-100: User capability enhancement
  readonly harmPrevention: number;          // 0-100: Risk mitigation effectiveness
}

// ✅ CARE LEVEL CLASSIFICATIONS
export type CareLevel = 'concerning' | 'acceptable' | 'good' | 'excellent';

// ✅ CARE VALIDATION FACTORY
export class CareValidationFactory {
  /**
   * Create a care validation result with computed overall score
   */
  static create(
    metrics: CareMetrics,
    message: string,
    isValid?: boolean
  ): CareValidation {
    const score = CareValidationFactory.computeOverallScore(metrics);
    const careLevel = CareValidationFactory.determineCareLevel(score);
    
    return {
      isValid: isValid ?? score >= 70, // Default: valid if score >= 70
      careLevel,
      score,
      message,
      metrics
    };
  }

  /**
   * Compute overall care score from individual metrics
   */
  private static computeOverallScore(metrics: CareMetrics): number {
    const weights = {
      dignityPreservation: 0.25,    // 25% - Human dignity is critical
      cognitiveLoadReduction: 0.20, // 20% - User experience matters
      systemStability: 0.20,        // 20% - Reliability is essential
      empowermentFactor: 0.20,      // 20% - User empowerment goal
      harmPrevention: 0.15          // 15% - Risk mitigation base requirement
    };

    return Math.round(
      metrics.dignityPreservation * weights.dignityPreservation +
      metrics.cognitiveLoadReduction * weights.cognitiveLoadReduction +
      metrics.systemStability * weights.systemStability +
      metrics.empowermentFactor * weights.empowermentFactor +
      metrics.harmPrevention * weights.harmPrevention
    );
  }

  /**
   * Determine care level based on overall score
   */
  private static determineCareLevel(score: number): CareLevel {
    if (score >= 90) return 'excellent';
    if (score >= 75) return 'good';
    if (score >= 60) return 'acceptable';
    return 'concerning';
  }
}

// ✅ CARE VALIDATION BUILDER PATTERN
export class CareValidationBuilder {
  private metrics: {
    dignityPreservation?: number;
    cognitiveLoadReduction?: number;
    systemStability?: number;
    empowermentFactor?: number;
    harmPrevention?: number;
  } = {};
  private message = '';

  dignity(score: number): this {
    this.metrics.dignityPreservation = Math.max(0, Math.min(100, score));
    return this;
  }

  cognitiveLoad(score: number): this {
    this.metrics.cognitiveLoadReduction = Math.max(0, Math.min(100, score));
    return this;
  }

  stability(score: number): this {
    this.metrics.systemStability = Math.max(0, Math.min(100, score));
    return this;
  }

  empowerment(score: number): this {
    this.metrics.empowermentFactor = Math.max(0, Math.min(100, score));
    return this;
  }

  harmPrevention(score: number): this {
    this.metrics.harmPrevention = Math.max(0, Math.min(100, score));
    return this;
  }

  withMessage(message: string): this {
    this.message = message;
    return this;
  }

  build(): CareValidation {
    // Provide defaults for missing metrics
    const completeMetrics: CareMetrics = {
      dignityPreservation: this.metrics.dignityPreservation ?? 50,
      cognitiveLoadReduction: this.metrics.cognitiveLoadReduction ?? 50,
      systemStability: this.metrics.systemStability ?? 50,
      empowermentFactor: this.metrics.empowermentFactor ?? 50,
      harmPrevention: this.metrics.harmPrevention ?? 50
    };

    return CareValidationFactory.create(completeMetrics, this.message);
  }
}

// ✅ TYPE GUARDS FOR RUNTIME VALIDATION
export function isCareValidation(obj: unknown): obj is CareValidation {
  return typeof obj === 'object' && obj !== null &&
    typeof (obj as any).isValid === 'boolean' &&
    typeof (obj as any).score === 'number' &&
    typeof (obj as any).careLevel === 'string' &&
    ['concerning', 'acceptable', 'good', 'excellent'].includes((obj as any).careLevel);
}

export function isCareMetrics(obj: unknown): obj is CareMetrics {
  return typeof obj === 'object' && obj !== null &&
    typeof (obj as any).dignityPreservation === 'number' &&
    typeof (obj as any).cognitiveLoadReduction === 'number' &&
    typeof (obj as any).systemStability === 'number' &&
    typeof (obj as any).empowermentFactor === 'number' &&
    typeof (obj as any).harmPrevention === 'number';
}