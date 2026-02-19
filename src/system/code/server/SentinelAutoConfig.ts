/**
 * SentinelAutoConfig - Auto-generate sentinel rules for shell commands
 *
 * When a persona executes a shell command (code/shell/execute), this class
 * detects if it's a build, test, or lint command and applies appropriate
 * sentinel classification rules automatically.
 *
 * Sentinel rules classify output lines as Error/Warning/Info/Success/Verbose,
 * enabling personas to get filtered, meaningful output instead of raw stdout/stderr.
 *
 * This bridges the gap between "sentinel exists" and "personas actually use it" —
 * rules are auto-injected so personas get classified output without having to
 * manually call code/shell/sentinel.
 */

import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
import type { SentinelRule } from '../../../shared/generated/code/SentinelRule';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('SentinelAutoConfig', 'code');

/**
 * Command category detected from the shell command string.
 */
type CommandCategory = 'build' | 'test' | 'lint' | 'unknown';

export class SentinelAutoConfig {

  /**
   * Detect command category and apply sentinel rules if applicable.
   * Fire-and-forget — caller should .catch() errors, this is non-blocking.
   */
  static async applyIfApplicable(
    personaId: string,
    executionId: string,
    cmd: string,
  ): Promise<void> {
    const category = SentinelAutoConfig.detectCategory(cmd);
    if (category === 'unknown') return;

    const rules = SentinelAutoConfig.rulesForCategory(category);
    if (rules.length === 0) return;

    try {
      const result = await CodeDaemon.shellSentinel(personaId, executionId, rules);
      log.info(`Auto-sentinel applied: ${category} (${result.ruleCount} rules) for execution ${executionId.slice(0, 8)}`);
    } catch (error: any) {
      log.warn(`Auto-sentinel failed for ${executionId.slice(0, 8)}: ${error.message}`);
    }
  }

  /**
   * Detect what kind of command this is from the command string.
   * Pattern-matches against common build/test/lint tool invocations.
   */
  static detectCategory(cmd: string): CommandCategory {
    const normalized = cmd.trim().toLowerCase();

    // Build commands
    if (
      /\b(npm run build|npx tsc|tsc\b|cargo build|make\b|cmake|xcodebuild|gradle\s+build|mvn\s+(compile|package)|dotnet\s+build|go\s+build)/.test(normalized)
    ) {
      return 'build';
    }

    // Test commands
    if (
      /\b(npm\s+test|npx\s+(vitest|jest|mocha)|vitest\b|jest\b|cargo\s+test|pytest\b|go\s+test|dotnet\s+test|gradle\s+test|mvn\s+test)/.test(normalized)
    ) {
      return 'test';
    }

    // Lint commands
    if (
      /\b(eslint|prettier|clippy|pylint|flake8|golangci-lint|rubocop)/.test(normalized)
    ) {
      return 'lint';
    }

    return 'unknown';
  }

  /**
   * Generate sentinel rules for a command category.
   * Rules are ordered: more specific patterns first, catch-all last.
   */
  static rulesForCategory(category: CommandCategory): SentinelRule[] {
    switch (category) {
      case 'build':
        return SentinelAutoConfig.buildRules();
      case 'test':
        return SentinelAutoConfig.testRules();
      case 'lint':
        return SentinelAutoConfig.lintRules();
      default:
        return [];
    }
  }

  // ────────────────────────────────────────────────────────────
  // Rule sets per category
  // ────────────────────────────────────────────────────────────

  private static buildRules(): SentinelRule[] {
    return [
      // Errors — compilation failures, missing modules, type errors
      { pattern: '(?i)(error|ERROR|Error TS\\d+|failed to compile|FAILED|cannot find module|fatal)', classification: 'Error', action: 'Emit' },
      // Warnings — non-fatal issues
      { pattern: '(?i)(warning|WARN|Warning TS\\d+|deprecated)', classification: 'Warning', action: 'Emit' },
      // Success markers
      { pattern: '(?i)(successfully compiled|built successfully|compiled successfully|build succeeded|✓)', classification: 'Success', action: 'Emit' },
      // Informational — progress, file counts
      { pattern: '(?i)(compiling|building|bundling|emitting|\\d+ modules?)', classification: 'Info', action: 'Emit' },
      // Suppress noisy lines — blank lines, stack traces from node_modules
      { pattern: '^\\s*$', classification: 'Verbose', action: 'Suppress' },
      { pattern: 'node_modules/', classification: 'Verbose', action: 'Suppress' },
    ];
  }

  private static testRules(): SentinelRule[] {
    return [
      // Test failures
      { pattern: '(?i)(FAIL|✗|✘|AssertionError|assertion failed|panicked|FAILED)', classification: 'Error', action: 'Emit' },
      // Test passes
      { pattern: '(?i)(PASS|✓|✔|ok\\b|passed)', classification: 'Success', action: 'Emit' },
      // Test summary lines
      { pattern: '(?i)(Tests?:|test result|Suites?:|Tests\\s+\\d+|Duration)', classification: 'Info', action: 'Emit' },
      // Warnings
      { pattern: '(?i)(WARN|warning|deprecated|skipped)', classification: 'Warning', action: 'Emit' },
      // Suppress noisy lines
      { pattern: '^\\s*$', classification: 'Verbose', action: 'Suppress' },
      { pattern: '^\\s+at\\s', classification: 'Verbose', action: 'Suppress' },
    ];
  }

  private static lintRules(): SentinelRule[] {
    return [
      // Lint errors
      { pattern: '(?i)(error|\\d+ errors?)', classification: 'Error', action: 'Emit' },
      // Lint warnings
      { pattern: '(?i)(warning|\\d+ warnings?)', classification: 'Warning', action: 'Emit' },
      // File references (useful context)
      { pattern: ':\\d+:\\d+', classification: 'Info', action: 'Emit' },
      // Summary
      { pattern: '(?i)(problems?|fixable|\\d+ files? checked)', classification: 'Info', action: 'Emit' },
      // Suppress blank/noisy
      { pattern: '^\\s*$', classification: 'Verbose', action: 'Suppress' },
    ];
  }
}
