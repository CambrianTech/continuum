/**
 * SecurityTier - Risk-based access control for coding agent execution
 *
 * Four tiers of access, each with explicit tool allowlists/denylists:
 * - discovery: Read-only exploration (tree, search, read, history)
 * - read: Analysis without modification (adds diff, data/list)
 * - write: File modifications within persona workspace (adds write, edit, undo)
 * - system: Full access including shell execution (requires governance approval)
 *
 * The PlanFormulator assesses risk and assigns a required tier.
 * The ToolAllowlistEnforcer gates every tool call through the tier.
 */

import type { SecurityTierLevel, RiskLevel } from '../shared/CodingTypes';

// Re-export for consumers that import from this module
export type { SecurityTierLevel, RiskLevel };

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export interface SecurityTier {
  readonly level: SecurityTierLevel;
  readonly allowedCommands: readonly string[];
  readonly deniedCommands: readonly string[];
  readonly maxToolCalls: number;
  readonly maxDurationMs: number;
  readonly maxFileSizeBytes: number;
  readonly allowProcessSpawn: boolean;
  readonly allowNetworkAccess: boolean;
  readonly requiresApproval: boolean;
}

// ────────────────────────────────────────────────────────────
// Tier Definitions
// ────────────────────────────────────────────────────────────

const DISCOVERY_TIER: SecurityTier = {
  level: 'discovery',
  allowedCommands: [
    'code/tree',
    'code/search',
    'code/read',
    'code/history',
  ],
  deniedCommands: [
    'code/write',
    'code/edit',
    'code/undo',
    'code/delete',
    'development/*',
    'system/*',
  ],
  maxToolCalls: 30,
  maxDurationMs: 60_000,
  maxFileSizeBytes: 0, // No writes allowed
  allowProcessSpawn: false,
  allowNetworkAccess: false,
  requiresApproval: false,
};

const READ_TIER: SecurityTier = {
  level: 'read',
  allowedCommands: [
    ...DISCOVERY_TIER.allowedCommands,
    'code/diff',
    'data/list',
    'data/read',
  ],
  deniedCommands: [
    'code/write',
    'code/edit',
    'code/undo',
    'code/delete',
    'development/*',
    'system/*',
  ],
  maxToolCalls: 30,
  maxDurationMs: 60_000,
  maxFileSizeBytes: 0, // No writes allowed
  allowProcessSpawn: false,
  allowNetworkAccess: false,
  requiresApproval: false,
};

const WRITE_TIER: SecurityTier = {
  level: 'write',
  allowedCommands: [
    ...READ_TIER.allowedCommands,
    'code/write',
    'code/edit',
    'code/undo',
    'code/diff',
  ],
  deniedCommands: [
    'code/delete',
    'development/exec',
    'development/sandbox-execute',
    'system/*',
  ],
  maxToolCalls: 20,
  maxDurationMs: 120_000,
  maxFileSizeBytes: 1_048_576, // 1MB
  allowProcessSpawn: false,
  allowNetworkAccess: false,
  requiresApproval: false, // Risk-based (PlanGovernance decides)
};

const SYSTEM_TIER: SecurityTier = {
  level: 'system',
  allowedCommands: ['*'],
  deniedCommands: [], // No restrictions
  maxToolCalls: 50,
  maxDurationMs: 300_000,
  maxFileSizeBytes: 10_485_760, // 10MB
  allowProcessSpawn: true,
  allowNetworkAccess: true,
  requiresApproval: true, // Always requires governance approval
};

// ────────────────────────────────────────────────────────────
// Tier Registry
// ────────────────────────────────────────────────────────────

const TIERS: Record<SecurityTierLevel, SecurityTier> = {
  discovery: DISCOVERY_TIER,
  read: READ_TIER,
  write: WRITE_TIER,
  system: SYSTEM_TIER,
};

/**
 * Get the SecurityTier definition for a given level.
 */
export function getTier(level: SecurityTierLevel): SecurityTier {
  return TIERS[level];
}

/**
 * All tier levels in ascending order of privilege.
 */
export const TIER_LEVELS: readonly SecurityTierLevel[] = ['discovery', 'read', 'write', 'system'];

/**
 * Check if tier A has equal or greater privilege than tier B.
 */
export function tierAtLeast(a: SecurityTierLevel, b: SecurityTierLevel): boolean {
  return TIER_LEVELS.indexOf(a) >= TIER_LEVELS.indexOf(b);
}

// ────────────────────────────────────────────────────────────
// Risk → Tier Mapping
// ────────────────────────────────────────────────────────────

/**
 * Map a risk level to the minimum security tier required.
 * Higher risk → higher tier → more restrictions (and potentially approval).
 */
export function riskToTier(risk: RiskLevel): SecurityTierLevel {
  switch (risk) {
    case 'low': return 'write';
    case 'medium': return 'write';
    case 'high': return 'write'; // Same tier, but PlanGovernance requires approval at high+
    case 'critical': return 'system';
  }
}

/**
 * Whether a given risk level should require governance approval.
 */
export function riskRequiresApproval(risk: RiskLevel, isMultiAgent: boolean): boolean {
  if (isMultiAgent) return true;
  if (risk === 'high' || risk === 'critical') return true;
  return false;
}
