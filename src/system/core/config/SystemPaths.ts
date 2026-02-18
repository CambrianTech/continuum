/**
 * SystemPaths - SINGLE SOURCE OF TRUTH for all filesystem paths
 *
 * Registry Pattern: Same tree structure off EITHER base location
 * - $HOME/.continuum/jtag (global shared resources)
 * - $REPO/.continuum/jtag (project-specific resources)
 *
 * Like Claude Code's # memoization: asks which location when saving.
 * This makes personas/genomes act like a "container registry" - can exist in either location.
 *
 * Example:
 * ```typescript
 * import { SystemPaths, createPathsForBase } from '@system/core/config/SystemPaths';
 *
 * // Use default repo-local paths
 * const logPath = SystemPaths.logs.personas('helper-ai');
 *
 * // Or create global paths (e.g., for shared personas)
 * const globalPaths = createPathsForBase(path.join(os.homedir(), '.continuum', 'jtag'));
 * const globalPersonaLog = globalPaths.logs.personas('helper-ai');
 * ```
 */

import * as path from 'path';
import * as os from 'os';

/**
 * Path tree structure - SAME regardless of base location
 * This is the SINGLE SOURCE OF TRUTH for the directory structure
 */
export interface ContinuumPaths {
  /** Root .continuum/jtag directory */
  root: string;

  /** Database storage paths */
  database: {
    root: string;
    main: string;
    backup: string;
  };

  /** Log file paths */
  logs: {
    root: string;
    /** Get log directory for a persona (by uniqueId: {name}-{shortId}) */
    personas: (uniqueId: string) => string;
    /** Get specific subsystem log file */
    subsystem: (uniqueId: string, subsystem: 'mind' | 'body' | 'soul' | 'cns') => string;
    /** System-wide logs (not persona-specific) */
    system: string;
    sql: string;
    errors: string;
  };

  /** Session data paths */
  sessions: {
    root: string;
    user: string;
    validation: string;
  };

  /** Registry and process tracking */
  registry: {
    root: string;
    processes: string;
    ports: string;
  };

  /** Temporary files and caches */
  temp: {
    root: string;
    screenshots: string;
    artifacts: string;
  };

  /** LoRA adapters and genome storage */
  genome: {
    root: string;
    adapters: string;
    training: string;
  };

  /** Persona storage (can be in $HOME or $REPO) */
  personas: {
    root: string;
    /** Get persona directory (by uniqueId: {name}-{shortId}) - the persona's $HOME */
    dir: (uniqueId: string) => string;
    /** Get persona data directory (all databases) */
    data: (uniqueId: string) => string;
    /** Get persona logs directory */
    logs: (uniqueId: string) => string;
    /** Get persona state database */
    state: (uniqueId: string) => string;
    /** Get persona memory database */
    memory: (uniqueId: string) => string;
    /** Get persona long-term memory database (Hippocampus) */
    longterm: (uniqueId: string) => string;
  };

  /** Human user storage (like personas, but for humans) */
  users: {
    root: string;
    /** Get user directory (by uniqueId) - the human's $HOME */
    dir: (uniqueId: string) => string;
    /** Get user data directory (all databases) */
    data: (uniqueId: string) => string;
    /** Get user logs directory */
    logs: (uniqueId: string) => string;
    /** Get user state database */
    state: (uniqueId: string) => string;
  };

  /** Agent storage (external AI agents like Claude Code, GPT, etc.) */
  agents: {
    root: string;
    /** Get agent directory (by uniqueId) - the agent's $HOME */
    dir: (uniqueId: string) => string;
    /** Get agent data directory (all databases) */
    data: (uniqueId: string) => string;
    /** Get agent logs directory */
    logs: (uniqueId: string) => string;
    /** Get agent state database */
    state: (uniqueId: string) => string;
  };
}

/**
 * Create path tree for ANY base location
 * This factory ensures SAME structure regardless of $HOME vs $REPO
 */
export function createPathsForBase(baseRoot: string): ContinuumPaths {
  return {
    root: baseRoot,

    database: {
      root: path.join(baseRoot, 'jtag', 'database'),
      main: path.join(baseRoot, 'jtag', 'database', 'main.db'),
      backup: path.join(baseRoot, 'jtag', 'database', 'backups')
    },

    logs: {
      root: path.join(baseRoot, 'jtag', 'logs'),

      personas: (uniqueId: string): string => {
        // uniqueId already in format "{name}-{shortId}", use directly
        return path.join(baseRoot, 'personas', uniqueId, 'logs');
      },

      subsystem: (uniqueId: string, subsystem: 'mind' | 'body' | 'soul' | 'cns'): string => {
        // uniqueId already in format "{name}-{shortId}", use directly
        return path.join(baseRoot, 'personas', uniqueId, 'logs', `${subsystem}.log`);
      },

      // System logs - all under logs/system/ with subdirectories
      system: path.join(baseRoot, 'jtag', 'logs', 'system'),
      sql: path.join(baseRoot, 'jtag', 'logs', 'system', 'data', 'sql.log'),
      errors: path.join(baseRoot, 'jtag', 'logs', 'system', 'core', 'errors.log')
    },

    sessions: {
      root: path.join(baseRoot, 'jtag', 'sessions'),
      user: path.join(baseRoot, 'jtag', 'sessions', 'user'),
      validation: path.join(baseRoot, 'jtag', 'sessions', 'validation')
    },

    registry: {
      root: path.join(baseRoot, 'jtag', 'registry'),
      processes: path.join(baseRoot, 'jtag', 'registry', 'process-registry.json'),
      ports: path.join(baseRoot, 'jtag', 'registry', 'dynamic-ports.json')
    },

    temp: {
      root: path.join(baseRoot, 'jtag', 'temp'),
      screenshots: path.join(baseRoot, 'jtag', 'temp', 'screenshots'),
      artifacts: path.join(baseRoot, 'jtag', 'temp', 'artifacts')
    },

    genome: {
      root: path.join(baseRoot, 'genome'),
      adapters: path.join(baseRoot, 'genome', 'adapters'),
      training: path.join(baseRoot, 'genome', 'training-data')
    },

    personas: {
      root: path.join(baseRoot, 'personas'),

      dir: (uniqueId: string): string => {
        // uniqueId already in format "{name}-{shortId}", use directly
        return path.join(baseRoot, 'personas', uniqueId);
      },

      data: (uniqueId: string): string => {
        // All databases live in data/ subdirectory
        return path.join(baseRoot, 'personas', uniqueId, 'data');
      },

      logs: (uniqueId: string): string => {
        // All log files live in logs/ subdirectory
        return path.join(baseRoot, 'personas', uniqueId, 'logs');
      },

      state: (uniqueId: string): string => {
        // State database in data/ subdirectory
        return path.join(baseRoot, 'personas', uniqueId, 'data', 'state.db');
      },

      memory: (uniqueId: string): string => {
        // Memory database in data/ subdirectory
        return path.join(baseRoot, 'personas', uniqueId, 'data', 'memory.db');
      },

      longterm: (uniqueId: string): string => {
        // Hippocampus long-term memory in data/ subdirectory
        return path.join(baseRoot, 'personas', uniqueId, 'data', 'longterm.db');
      }
    },

    users: {
      root: path.join(baseRoot, 'users'),

      dir: (uniqueId: string): string => {
        return path.join(baseRoot, 'users', uniqueId);
      },

      data: (uniqueId: string): string => {
        return path.join(baseRoot, 'users', uniqueId, 'data');
      },

      logs: (uniqueId: string): string => {
        return path.join(baseRoot, 'users', uniqueId, 'logs');
      },

      state: (uniqueId: string): string => {
        return path.join(baseRoot, 'users', uniqueId, 'data', 'state.db');
      }
    },

    agents: {
      root: path.join(baseRoot, 'agents'),

      dir: (uniqueId: string): string => {
        return path.join(baseRoot, 'agents', uniqueId);
      },

      data: (uniqueId: string): string => {
        return path.join(baseRoot, 'agents', uniqueId, 'data');
      },

      logs: (uniqueId: string): string => {
        return path.join(baseRoot, 'agents', uniqueId, 'logs');
      },

      state: (uniqueId: string): string => {
        return path.join(baseRoot, 'agents', uniqueId, 'data', 'state.db');
      }
    }
  };
}

/**
 * Base locations - THE ONLY PLACES these strings should exist
 */
const REPO_ROOT = path.join(process.cwd(), '.continuum');
const HOME_ROOT = path.join(os.homedir(), '.continuum');

/**
 * Default paths (repo-local) - use for project-specific resources
 */
export const SystemPaths: ContinuumPaths = createPathsForBase(REPO_ROOT);

/**
 * Global paths ($HOME) - use for shared/user-global resources
 * Example: shared personas, global genomes, user preferences
 */
export const GlobalPaths: ContinuumPaths = createPathsForBase(HOME_ROOT);

