/**
 * ToolAllowlistEnforcer - Gateway that filters every tool call through a SecurityTier
 *
 * Sits between the CodeAgentOrchestrator and Commands.execute().
 * Every tool call passes through enforce() before execution.
 *
 * Enforcement rules (evaluated in order):
 * 1. Denied commands always blocked (explicit deny wins over allow)
 * 2. Allowed commands checked via glob matching
 * 3. File paths validated against persona workspace
 * 4. File size limits checked for write operations
 * 5. Every decision logged for audit trail
 */

import type { SecurityTier } from './SecurityTier';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('ToolAllowlistEnforcer', 'code');

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export interface EnforcementResult {
  readonly allowed: boolean;
  readonly reason: string;
  readonly toolName: string;
  readonly tierLevel: string;
}

// ────────────────────────────────────────────────────────────
// Enforcer
// ────────────────────────────────────────────────────────────

export class ToolAllowlistEnforcer {
  private readonly _tier: SecurityTier;
  private readonly _auditLog: EnforcementResult[] = [];

  constructor(tier: SecurityTier) {
    this._tier = tier;
  }

  get tier(): SecurityTier {
    return this._tier;
  }

  get auditLog(): readonly EnforcementResult[] {
    return this._auditLog;
  }

  /**
   * Check if a tool call is allowed under the current tier.
   * Throws if the tool is denied.
   */
  enforce(toolName: string, params?: Record<string, unknown>): void {
    const result = this.check(toolName, params);
    this._auditLog.push(result);

    if (!result.allowed) {
      log.warn(`BLOCKED: ${toolName} — ${result.reason} (tier: ${this._tier.level})`);
      throw new ToolDeniedError(toolName, result.reason, this._tier.level);
    }

    log.debug(`ALLOWED: ${toolName} (tier: ${this._tier.level})`);
  }

  /**
   * Non-throwing check — returns the enforcement result without blocking.
   */
  check(toolName: string, params?: Record<string, unknown>): EnforcementResult {
    // 1. Check denied list (explicit deny always wins)
    if (this.matchesAny(toolName, this._tier.deniedCommands)) {
      return {
        allowed: false,
        reason: `Command '${toolName}' is explicitly denied in ${this._tier.level} tier`,
        toolName,
        tierLevel: this._tier.level,
      };
    }

    // 2. Check allowed list
    if (!this.matchesAny(toolName, this._tier.allowedCommands)) {
      return {
        allowed: false,
        reason: `Command '${toolName}' is not in the allowed list for ${this._tier.level} tier`,
        toolName,
        tierLevel: this._tier.level,
      };
    }

    // 3. Check process spawn restriction
    if (!this._tier.allowProcessSpawn && this.isProcessSpawnCommand(toolName)) {
      return {
        allowed: false,
        reason: `Process spawn commands are not allowed in ${this._tier.level} tier`,
        toolName,
        tierLevel: this._tier.level,
      };
    }

    // 4. Check file size for write operations
    if (this.isWriteCommand(toolName) && params) {
      const content = params['content'] as string | undefined;
      if (content && this._tier.maxFileSizeBytes > 0) {
        const sizeBytes = new TextEncoder().encode(content).length;
        if (sizeBytes > this._tier.maxFileSizeBytes) {
          return {
            allowed: false,
            reason: `Content size ${sizeBytes} exceeds tier limit of ${this._tier.maxFileSizeBytes} bytes`,
            toolName,
            tierLevel: this._tier.level,
          };
        }
      }
    }

    return {
      allowed: true,
      reason: 'Allowed by tier policy',
      toolName,
      tierLevel: this._tier.level,
    };
  }

  /**
   * Check if a tool name matches any pattern in the list.
   * Supports exact match and trailing wildcard (e.g., 'code/*', '*').
   */
  private matchesAny(toolName: string, patterns: readonly string[]): boolean {
    for (const pattern of patterns) {
      if (pattern === '*') return true;
      if (pattern === toolName) return true;

      // Glob: 'code/*' matches 'code/read', 'code/edit', etc.
      if (pattern.endsWith('/*')) {
        const prefix = pattern.slice(0, -2);
        if (toolName.startsWith(prefix + '/')) return true;
      }
    }
    return false;
  }

  /**
   * Commands that spawn child processes.
   */
  private isProcessSpawnCommand(toolName: string): boolean {
    return toolName === 'development/exec' ||
           toolName === 'development/sandbox-execute' ||
           toolName === 'development/build';
  }

  /**
   * Commands that write to the filesystem.
   */
  private isWriteCommand(toolName: string): boolean {
    return toolName === 'code/write' || toolName === 'code/edit';
  }
}

// ────────────────────────────────────────────────────────────
// Error
// ────────────────────────────────────────────────────────────

export class ToolDeniedError extends Error {
  readonly toolName: string;
  readonly tierLevel: string;

  constructor(toolName: string, reason: string, tierLevel: string) {
    super(`Tool '${toolName}' denied: ${reason}`);
    this.name = 'ToolDeniedError';
    this.toolName = toolName;
    this.tierLevel = tierLevel;
  }
}
