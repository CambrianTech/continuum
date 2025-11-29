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
    /** Get log directory for a persona (by display name, not UUID) */
    personas: (personaName: string) => string;
    /** Get specific subsystem log file */
    subsystem: (personaName: string, subsystem: 'mind' | 'body' | 'soul' | 'cns') => string;
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
    /** Get persona directory (by display name) */
    dir: (personaName: string) => string;
    /** Get persona state database */
    state: (personaName: string) => string;
    /** Get persona memory database */
    memory: (personaName: string) => string;
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
      root: path.join(baseRoot, 'database'),
      main: path.join(baseRoot, 'database', 'main.db'),
      backup: path.join(baseRoot, 'database', 'backups')
    },

    logs: {
      root: path.join(baseRoot, 'logs'),

      personas: (personaName: string): string => {
        const safeName = personaName.replace(/[^a-zA-Z0-9-_]/g, '-').toLowerCase();
        return path.join(baseRoot, 'logs', safeName);
      },

      subsystem: (personaName: string, subsystem: 'mind' | 'body' | 'soul' | 'cns'): string => {
        const safeName = personaName.replace(/[^a-zA-Z0-9-_]/g, '-').toLowerCase();
        return path.join(baseRoot, 'logs', safeName, `${subsystem}.log`);
      },

      system: path.join(baseRoot, 'logs', 'system'),
      sql: path.join(baseRoot, 'logs', 'system', 'sql.log'),
      errors: path.join(baseRoot, 'logs', 'system', 'errors.log')
    },

    sessions: {
      root: path.join(baseRoot, 'sessions'),
      user: path.join(baseRoot, 'sessions', 'user'),
      validation: path.join(baseRoot, 'sessions', 'validation')
    },

    registry: {
      root: path.join(baseRoot, 'registry'),
      processes: path.join(baseRoot, 'registry', 'process-registry.json'),
      ports: path.join(baseRoot, 'registry', 'dynamic-ports.json')
    },

    temp: {
      root: path.join(baseRoot, 'temp'),
      screenshots: path.join(baseRoot, 'temp', 'screenshots'),
      artifacts: path.join(baseRoot, 'temp', 'artifacts')
    },

    genome: {
      root: path.join(baseRoot, 'genome'),
      adapters: path.join(baseRoot, 'genome', 'lora-adapters'),
      training: path.join(baseRoot, 'genome', 'training-data')
    },

    personas: {
      root: path.join(baseRoot, 'personas'),

      dir: (personaName: string): string => {
        const safeName = personaName.replace(/[^a-zA-Z0-9-_]/g, '-').toLowerCase();
        return path.join(baseRoot, 'personas', safeName);
      },

      state: (personaName: string): string => {
        const safeName = personaName.replace(/[^a-zA-Z0-9-_]/g, '-').toLowerCase();
        return path.join(baseRoot, 'personas', safeName, 'state.db');
      },

      memory: (personaName: string): string => {
        const safeName = personaName.replace(/[^a-zA-Z0-9-_]/g, '-').toLowerCase();
        return path.join(baseRoot, 'personas', safeName, 'memory.db');
      }
    }
  };
}

/**
 * Base locations - THE ONLY PLACES these strings should exist
 */
const REPO_ROOT = path.join(process.cwd(), '.continuum', 'jtag');
const HOME_ROOT = path.join(os.homedir(), '.continuum', 'jtag');

/**
 * Default paths (repo-local) - use for project-specific resources
 */
export const SystemPaths: ContinuumPaths = createPathsForBase(REPO_ROOT);

/**
 * Global paths ($HOME) - use for shared/user-global resources
 * Example: shared personas, global genomes, user preferences
 */
export const GlobalPaths: ContinuumPaths = createPathsForBase(HOME_ROOT);

