/**
 * DefaultSentinelRules - Pre-configured sentinel patterns for common build tools
 *
 * These rules help classify shell output so personas can react appropriately:
 * - Errors are high priority → fix immediately
 * - Warnings are medium priority → consider fixing
 * - Success messages confirm progress
 * - Verbose output is filtered to reduce noise
 */

import type { SentinelRule, OutputClassification, SentinelAction } from '../../../../shared/generated';

// Helper to create rules concisely
const rule = (
  pattern: string,
  classification: OutputClassification,
  action: SentinelAction = 'Emit'
): SentinelRule => ({ pattern, classification, action });

/**
 * TypeScript compiler (tsc, tsx)
 */
export const TYPESCRIPT_RULES: SentinelRule[] = [
  // Errors
  rule('error TS\\d+:', 'Error'),
  rule('Cannot find module', 'Error'),
  rule("Type '.*' is not assignable to type", 'Error'),
  rule("Property '.*' does not exist on type", 'Error'),
  rule('has no exported member', 'Error'),

  // Warnings
  rule('warning TS\\d+:', 'Warning'),
  rule('is declared but its value is never read', 'Warning'),
  rule("'.*' is deprecated", 'Warning'),

  // Success
  rule('Successfully compiled', 'Success'),
  rule('Found 0 errors', 'Success'),
  rule('✅', 'Success'),

  // Info
  rule('Starting compilation', 'Info'),
  rule('Watching for file changes', 'Info'),
];

/**
 * Rust compiler (cargo, rustc)
 */
export const RUST_RULES: SentinelRule[] = [
  // Errors
  rule('error\\[E\\d+\\]:', 'Error'),
  rule('error: aborting due to', 'Error'),
  rule('cannot find .* in this scope', 'Error'),
  rule('mismatched types', 'Error'),
  rule('borrow of moved value', 'Error'),

  // Warnings
  rule('warning:', 'Warning'),
  rule('unused variable', 'Warning'),
  rule('dead_code', 'Warning'),
  rule('#\\[warn\\(', 'Warning'),

  // Success
  rule('Finished.*target', 'Success'),
  rule('Compiling .* v\\d', 'Info'),
  rule('Running.*tests', 'Info'),

  // Test results
  rule('test .* \\.\\.\\. ok', 'Success'),
  rule('test .* \\.\\.\\. FAILED', 'Error'),
  rule('test result: ok\\.', 'Success'),
  rule('test result: FAILED\\.', 'Error'),
];

/**
 * npm/node
 */
export const NPM_RULES: SentinelRule[] = [
  // Errors
  rule('npm ERR!', 'Error'),
  rule('Error: ', 'Error'),
  rule('SyntaxError:', 'Error'),
  rule('TypeError:', 'Error'),
  rule('ReferenceError:', 'Error'),
  rule('ENOENT', 'Error'),
  rule('MODULE_NOT_FOUND', 'Error'),

  // Warnings
  rule('npm WARN', 'Warning'),
  rule('deprecated', 'Warning'),
  rule('ExperimentalWarning:', 'Warning'),

  // Success
  rule('added \\d+ packages', 'Success'),
  rule('up to date', 'Success'),
  rule('Server listening', 'Success'),
  rule('✓', 'Success'),

  // Verbose (suppress unless debugging)
  rule('npm timing', 'Verbose', 'Suppress'),
  rule('npm http', 'Verbose', 'Suppress'),
];

/**
 * Git commands
 */
export const GIT_RULES: SentinelRule[] = [
  // Errors
  rule('fatal:', 'Error'),
  rule('error: ', 'Error'),
  rule('CONFLICT', 'Error'),
  rule('merge failed', 'Error'),

  // Warnings
  rule('warning:', 'Warning'),
  rule('detached HEAD', 'Warning'),

  // Success
  rule('Your branch is up to date', 'Success'),
  rule('\\d+ file.* changed', 'Success'),
  rule('Successfully', 'Success'),

  // Info
  rule('On branch', 'Info'),
  rule('nothing to commit', 'Info'),
];

/**
 * Python
 */
export const PYTHON_RULES: SentinelRule[] = [
  // Errors
  rule('Traceback', 'Error'),
  rule('Error:', 'Error'),
  rule('Exception:', 'Error'),
  rule('ModuleNotFoundError:', 'Error'),
  rule('ImportError:', 'Error'),
  rule('SyntaxError:', 'Error'),

  // Warnings
  rule('Warning:', 'Warning'),
  rule('DeprecationWarning:', 'Warning'),

  // Test results
  rule('PASSED', 'Success'),
  rule('FAILED', 'Error'),
  rule('\\d+ passed', 'Success'),
];

/**
 * General patterns that apply to most build tools
 */
export const GENERAL_RULES: SentinelRule[] = [
  // Universal error patterns
  rule('[Ee]rror:', 'Error'),
  rule('[Ff]ailed', 'Error'),
  rule('[Ee]xception', 'Error'),
  rule('FATAL', 'Error'),
  rule('panic', 'Error'),

  // Universal warning patterns
  rule('[Ww]arning:', 'Warning'),
  rule('WARN', 'Warning'),

  // Universal success patterns
  rule('[Ss]uccess', 'Success'),
  rule('[Cc]omplete', 'Success'),
  rule('[Dd]one', 'Info'),
  rule('[Ff]inished', 'Success'),
  rule('✅|✓|✔', 'Success'),

  // Progress indicators
  rule('^\\s*\\d+%', 'Info'),
  rule('Building', 'Info'),
  rule('Compiling', 'Info'),
  rule('Installing', 'Info'),
];

/**
 * Get combined rules for a project type
 */
export function getRulesForProject(projectType: 'typescript' | 'rust' | 'python' | 'node' | 'general'): SentinelRule[] {
  switch (projectType) {
    case 'typescript':
      return [...TYPESCRIPT_RULES, ...NPM_RULES, ...GIT_RULES, ...GENERAL_RULES];
    case 'rust':
      return [...RUST_RULES, ...GIT_RULES, ...GENERAL_RULES];
    case 'python':
      return [...PYTHON_RULES, ...GIT_RULES, ...GENERAL_RULES];
    case 'node':
      return [...NPM_RULES, ...GIT_RULES, ...GENERAL_RULES];
    case 'general':
    default:
      return [...GIT_RULES, ...GENERAL_RULES];
  }
}

/**
 * Detect project type from file patterns in workspace
 */
export function detectProjectType(files: string[]): 'typescript' | 'rust' | 'python' | 'node' | 'general' {
  const hasFile = (pattern: string) => files.some(f => f.includes(pattern));

  if (hasFile('tsconfig.json') || hasFile('.ts')) return 'typescript';
  if (hasFile('Cargo.toml') || hasFile('.rs')) return 'rust';
  if (hasFile('setup.py') || hasFile('requirements.txt') || hasFile('.py')) return 'python';
  if (hasFile('package.json')) return 'node';
  return 'general';
}
