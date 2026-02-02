/**
 * PlanGovernance - Risk-based approval routing for coding plans
 *
 * Determines whether a plan needs team approval before execution,
 * creates DecisionProposals for review, and handles governance callbacks.
 *
 * Approval rules:
 * - Auto-approve: single-agent + riskLevel low/medium
 * - Require approval: multi-agent OR riskLevel high/critical
 * - Always require: system-tier operations
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { RiskLevel, SecurityTierLevel } from '../shared/CodingTypes';
import { CodingPlanEntity, type CodingPlanStatus } from '../../data/entities/CodingPlanEntity';
import { riskRequiresApproval } from './SecurityTier';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('PlanGovernance', 'code');

// ────────────────────────────────────────────────────────────
// Governance decision outcomes
// ────────────────────────────────────────────────────────────

export type GovernanceOutcome =
  | 'approved'
  | 'approved_with_changes'
  | 'changes_requested'
  | 'rejected';

export interface GovernanceDecision {
  readonly proposalId: UUID;
  readonly outcome: GovernanceOutcome;
  readonly reasoning: string;
  readonly suggestedChanges?: string;
}

// ────────────────────────────────────────────────────────────
// Implementation
// ────────────────────────────────────────────────────────────

export class PlanGovernance {

  /**
   * Determine if a plan needs team approval before execution.
   */
  shouldRequireApproval(plan: CodingPlanEntity): boolean {
    // System tier always requires approval
    if (plan.securityTier === 'system') {
      return true;
    }

    // Delegate to SecurityTier's risk-based logic
    const isMultiAgent = plan.assignees.length > 1;
    return riskRequiresApproval(plan.riskLevel, isMultiAgent);
  }

  /**
   * Create a DecisionProposal for plan review.
   * Returns the proposal ID, or undefined if proposal creation failed.
   */
  async proposePlan(plan: CodingPlanEntity): Promise<UUID | undefined> {
    try {
      // Dynamic import to avoid circular dependency
      const { DecisionPropose } = await import(
        '../../../commands/collaboration/decision/propose/shared/DecisionProposeTypes'
      );

      const fileList = this.extractTargetFiles(plan);
      const stepSummary = plan.steps
        .map(s => `  ${s.stepNumber}. [${s.action}] ${s.description}`)
        .join('\n');

      const rationale = [
        `**Task:** ${plan.taskDescription}`,
        `**Approach:** ${plan.summary}`,
        `**Risk Level:** ${plan.riskLevel} (${plan.riskReason ?? 'No reason provided'})`,
        `**Security Tier:** ${plan.securityTier}`,
        `**Assignees:** ${plan.assignees.length} agent(s)`,
        `**Steps (${plan.steps.length}):**\n${stepSummary}`,
        fileList.length > 0 ? `**Target Files:**\n${fileList.map(f => `  - ${f}`).join('\n')}` : '',
      ].filter(Boolean).join('\n\n');

      const result = await DecisionPropose.execute({
        topic: `Coding Plan: ${plan.summary}`,
        rationale,
        options: [
          { label: 'Approve', description: 'Approve the plan for execution' },
          { label: 'Approve with Changes', description: 'Approve with suggested modifications' },
          { label: 'Request Changes', description: 'Send back for revision' },
          { label: 'Reject', description: 'Decline this plan' },
        ],
        scope: 'all',
        significanceLevel: this.riskToSignificance(plan.riskLevel),
        proposerId: plan.leadId,
      });

      if (result.success && result.proposalId) {
        log.info(`Plan proposed for governance: ${result.proposalId} (plan: ${plan.taskId})`);
        return result.proposalId;
      }

      log.warn(`Governance proposal creation returned success=false`);
      return undefined;
    } catch (error) {
      log.warn(`Governance proposal failed: ${error instanceof Error ? error.message : String(error)}`);
      return undefined;
    }
  }

  /**
   * Handle a governance decision callback.
   * Returns the CodingPlanStatus the plan should transition to.
   */
  resolveDecision(decision: GovernanceDecision): CodingPlanStatus {
    switch (decision.outcome) {
      case 'approved':
        return 'approved';
      case 'approved_with_changes':
        return 'approved';
      case 'changes_requested':
        return 'draft';
      case 'rejected':
        return 'cancelled';
    }
  }

  // ────────────────────────────────────────────────────────────
  // Private helpers
  // ────────────────────────────────────────────────────────────

  /**
   * Extract unique target files from all plan steps.
   */
  private extractTargetFiles(plan: CodingPlanEntity): string[] {
    const files = new Set<string>();
    for (const step of plan.steps) {
      for (const file of step.targetFiles) {
        files.add(file);
      }
    }
    return Array.from(files).sort();
  }

  /**
   * Map risk level to governance significance.
   */
  private riskToSignificance(risk: RiskLevel): 'low' | 'medium' | 'high' | 'critical' {
    return risk; // 1:1 mapping — risk levels align with significance levels
  }
}
