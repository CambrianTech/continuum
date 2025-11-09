/**
 * Version Comparison Utility
 *
 * Semantic version comparison with package.json-style constraint rules.
 * Returns action enum indicating what to do given installed vs required version.
 */

/**
 * Action to take based on version comparison
 */
export enum VersionAction {
  /** Dependency not installed, needs installation */
  INSTALL = 'install',

  /** Installed version too old, needs upgrade */
  UPGRADE = 'upgrade',

  /** Installed version satisfies requirement */
  SATISFIED = 'satisfied',

  /** Installed version too new, would need downgrade (rare) */
  DOWNGRADE_REQUIRED = 'downgrade_required',

  /** Invalid version format or comparison error */
  INVALID_VERSION = 'invalid_version'
}

/**
 * Version constraint rule type (package.json style)
 *
 * Supported formats:
 * - ">=1.2.3" - Greater than or equal
 * - ">1.2.3"  - Strictly greater than
 * - "<=1.2.3" - Less than or equal
 * - "<1.2.3"  - Strictly less than
 * - "=1.2.3"  - Exact match
 * - "1.2.3"   - Exact match (= implied)
 * - "^1.2.3"  - Compatible (major version must match, minor/patch can be higher)
 * - "~1.2.3"  - Approximately (major + minor must match, patch can be higher)
 * - "*"       - Any version
 */
export type VersionRule = string;

/**
 * Parsed semantic version
 */
interface SemanticVersion {
  major: number;
  minor: number;
  patch: number;
  raw: string;
}

/**
 * Compare two versions and determine what action to take
 *
 * @param installedVersion - Currently installed version (e.g., "1.2.3") or null/undefined if not installed
 * @param rule - Version constraint rule (e.g., ">=1.0.0", "^2.0.0", "~1.5.0")
 * @returns VersionAction enum indicating what to do
 *
 * @example
 * ```typescript
 * compareVersions(null, ">=4.0.0")              // INSTALL
 * compareVersions("3.2.1", ">=4.0.0")           // UPGRADE
 * compareVersions("5.0.0", ">=4.0.0")           // SATISFIED
 * compareVersions("2.0.0", "^1.0.0")            // DOWNGRADE_REQUIRED (major too new)
 * compareVersions("invalid", ">=1.0.0")         // INVALID_VERSION
 * ```
 */
export function compareVersions(
  installedVersion: string | null | undefined,
  rule: VersionRule
): VersionAction {
  // Not installed
  if (!installedVersion) {
    return VersionAction.INSTALL;
  }

  // Wildcard rule - always satisfied
  if (rule === '*') {
    return VersionAction.SATISFIED;
  }

  // Parse installed version
  const installed = parseVersion(installedVersion);
  if (!installed) {
    return VersionAction.INVALID_VERSION;
  }

  // Parse rule (extract operator and version)
  const ruleMatch = rule.match(/^([><=^~]*)(.+)$/);
  if (!ruleMatch) {
    return VersionAction.INVALID_VERSION;
  }

  const operator = ruleMatch[1] || '=';  // Default to exact match
  const requiredVersionStr = ruleMatch[2];
  const required = parseVersion(requiredVersionStr);

  if (!required) {
    return VersionAction.INVALID_VERSION;
  }

  // Perform comparison based on operator
  switch (operator) {
    case '>=':
      return compareSemanticVersions(installed, required) >= 0
        ? VersionAction.SATISFIED
        : VersionAction.UPGRADE;

    case '>':
      return compareSemanticVersions(installed, required) > 0
        ? VersionAction.SATISFIED
        : VersionAction.UPGRADE;

    case '<=':
      return compareSemanticVersions(installed, required) <= 0
        ? VersionAction.SATISFIED
        : VersionAction.DOWNGRADE_REQUIRED;

    case '<':
      return compareSemanticVersions(installed, required) < 0
        ? VersionAction.SATISFIED
        : VersionAction.DOWNGRADE_REQUIRED;

    case '=':
    case '':
      return compareSemanticVersions(installed, required) === 0
        ? VersionAction.SATISFIED
        : compareSemanticVersions(installed, required) < 0
          ? VersionAction.UPGRADE
          : VersionAction.DOWNGRADE_REQUIRED;

    case '^':
      // Compatible: major must match, minor/patch can be higher
      if (installed.major !== required.major) {
        return installed.major < required.major
          ? VersionAction.UPGRADE
          : VersionAction.DOWNGRADE_REQUIRED;
      }
      return compareSemanticVersions(installed, required) >= 0
        ? VersionAction.SATISFIED
        : VersionAction.UPGRADE;

    case '~':
      // Approximately: major + minor must match, patch can be higher
      if (installed.major !== required.major || installed.minor !== required.minor) {
        const cmp = compareSemanticVersions(installed, required);
        return cmp < 0
          ? VersionAction.UPGRADE
          : VersionAction.DOWNGRADE_REQUIRED;
      }
      return installed.patch >= required.patch
        ? VersionAction.SATISFIED
        : VersionAction.UPGRADE;

    default:
      return VersionAction.INVALID_VERSION;
  }
}

/**
 * Parse semantic version string (e.g., "1.2.3", "1.2", or "v1.2.3")
 *
 * @param versionStr - Version string to parse
 * @returns Parsed version or null if invalid
 */
function parseVersion(versionStr: string): SemanticVersion | null {
  // Strip leading 'v' if present
  const cleaned = versionStr.trim().replace(/^v/, '');

  // Match major.minor.patch OR major.minor (patch defaults to 0)
  const match = cleaned.match(/^(\d+)\.(\d+)(?:\.(\d+))?/);
  if (!match) {
    return null;
  }

  return {
    major: parseInt(match[1], 10),
    minor: parseInt(match[2], 10),
    patch: match[3] ? parseInt(match[3], 10) : 0,  // Default patch to 0 if not present
    raw: versionStr
  };
}

/**
 * Compare two semantic versions
 *
 * @returns -1 if a < b, 0 if a === b, 1 if a > b
 */
function compareSemanticVersions(a: SemanticVersion, b: SemanticVersion): number {
  if (a.major !== b.major) {
    return a.major < b.major ? -1 : 1;
  }
  if (a.minor !== b.minor) {
    return a.minor < b.minor ? -1 : 1;
  }
  if (a.patch !== b.patch) {
    return a.patch < b.patch ? -1 : 1;
  }
  return 0;
}

/**
 * Format VersionAction as human-readable message
 *
 * @param action - VersionAction enum value
 * @param installedVersion - Currently installed version
 * @param rule - Version rule that was checked
 * @returns Human-readable message
 *
 * @example
 * ```typescript
 * formatActionMessage(VersionAction.UPGRADE, "3.2.1", ">=4.0.0")
 * // Returns: "Upgrade required: installed version 3.2.1 does not satisfy >=4.0.0"
 * ```
 */
export function formatActionMessage(
  action: VersionAction,
  installedVersion: string | null | undefined,
  rule: VersionRule
): string {
  switch (action) {
    case VersionAction.INSTALL:
      return `Installation required: dependency not found (requirement: ${rule})`;

    case VersionAction.UPGRADE:
      return `Upgrade required: installed version ${installedVersion} does not satisfy ${rule}`;

    case VersionAction.SATISFIED:
      return `Version satisfied: installed version ${installedVersion} satisfies ${rule}`;

    case VersionAction.DOWNGRADE_REQUIRED:
      return `Downgrade required: installed version ${installedVersion} is too new for ${rule}`;

    case VersionAction.INVALID_VERSION:
      return `Invalid version: could not parse "${installedVersion}" or rule "${rule}"`;

    default:
      return `Unknown action: ${action}`;
  }
}
