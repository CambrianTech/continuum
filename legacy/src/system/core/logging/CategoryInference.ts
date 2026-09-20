/**
 * Category Inference - Convention-based log routing
 *
 * Automatically determines log subdirectory from component name suffix.
 * No case statements - just pattern matching.
 *
 * Usage:
 *   inferCategory('ArchiveDaemonServer')  → 'daemons/ArchiveDaemonServer'
 *   inferCategory('SqliteStorageAdapter') → 'adapters/SqliteStorageAdapter'
 *   inferCategory('DataWorker')           → 'workers/DataWorker'
 */

export interface CategoryRule {
  /** Regex pattern to match component name suffix */
  pattern: RegExp;
  /** Subdirectory for matching components */
  subdirectory: string;
}

/**
 * Default category rules - order matters (check longer suffixes first)
 */
export const DEFAULT_CATEGORY_RULES: CategoryRule[] = [
  // Daemons
  { pattern: /DaemonServer$/, subdirectory: 'daemons' },
  { pattern: /Daemon$/, subdirectory: 'daemons' },

  // Adapters
  { pattern: /StorageAdapter$/, subdirectory: 'adapters' },
  { pattern: /Adapter$/, subdirectory: 'adapters' },

  // Workers
  { pattern: /Worker$/, subdirectory: 'workers' },

  // Commands
  { pattern: /Command$/, subdirectory: 'commands' },
  { pattern: /ServerCommand$/, subdirectory: 'commands' },
  { pattern: /BrowserCommand$/, subdirectory: 'commands' },

  // AI/Provider
  { pattern: /Provider$/, subdirectory: 'ai' },
  { pattern: /PersonaUser$/, subdirectory: 'ai' },

  // Data
  { pattern: /Executor$/, subdirectory: 'data' },
  { pattern: /QueryBuilder$/, subdirectory: 'data' },

  // Core
  { pattern: /Manager$/, subdirectory: 'core' },
  { pattern: /Router$/, subdirectory: 'core' },
  { pattern: /Service$/, subdirectory: 'core' },
  { pattern: /Registry$/, subdirectory: 'core' },
];

/**
 * Infer log category from component name using suffix conventions
 *
 * @param component - Component class name (e.g., 'ArchiveDaemonServer')
 * @param rules - Category rules to use (defaults to DEFAULT_CATEGORY_RULES)
 * @returns Category path like 'daemons/ArchiveDaemonServer'
 */
export function inferCategory(
  component: string,
  rules: CategoryRule[] = DEFAULT_CATEGORY_RULES
): string {
  for (const rule of rules) {
    if (rule.pattern.test(component)) {
      return `${rule.subdirectory}/${component}`;
    }
  }

  // Default: put in core/ with component name
  return `core/${component}`;
}

/**
 * Add custom category rules (prepends to existing rules)
 *
 * @param customRules - Rules to add
 * @returns Combined rules array
 */
export function extendCategoryRules(customRules: CategoryRule[]): CategoryRule[] {
  return [...customRules, ...DEFAULT_CATEGORY_RULES];
}
