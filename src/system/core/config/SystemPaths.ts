/**
 * SystemPaths - SINGLE SOURCE OF TRUTH for all filesystem paths
 *
 * Two-root architecture:
 * - PERSISTENT ($HOME/.continuum): genome, personas, databases, blobs, models, datasets
 * - RUNTIME ($REPO/.continuum): logs, sessions, sockets, registry, temp, shared, reports, media
 *
 * 25+ consumers import SystemPaths — the composite singleton routes each section
 * to the correct root transparently. Zero consumer code changes needed.
 *
 * Example:
 * ```typescript
 * import { SystemPaths } from '@system/core/config/SystemPaths';
 *
 * // Persistent → $HOME/.continuum
 * const adapterDir = SystemPaths.genome.adapters;
 * const personaDb = SystemPaths.personas.longterm('helper-abc123');
 *
 * // Runtime → $REPO/.continuum
 * const logPath = SystemPaths.logs.system;
 * const socketPath = SystemPaths.sockets.core;
 * ```
 */

import * as path from 'path';
import * as os from 'os';

/**
 * Path tree structure - SAME regardless of base location.
 * This is the SINGLE SOURCE OF TRUTH for the directory structure.
 * Every directory that exists on disk gets declared here.
 */
export interface ContinuumPaths {
  /** Root .continuum directory */
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
    /** .genome.tgz distribution archives for adapter trading */
    packages: string;
    /** Micromamba Python environment for PEFT training */
    python: string;
  };

  /** Persona storage (identity, memories, databases) */
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

  /** Content-addressable blob storage */
  blobs: {
    root: string;
  };

  /** Shared data (room exports, collaborative files) */
  shared: {
    root: string;
    rooms: (roomId: string) => string;
  };

  /** Generated reports */
  reports: {
    root: string;
  };

  /** Downloaded/cached ML models */
  models: {
    root: string;
  };

  /** Unix domain sockets for IPC */
  sockets: {
    root: string;
    core: string;
    archive: string;
    inference: string;
  };

  /** Training datasets */
  datasets: {
    root: string;
    parsed: string;
    prepared: string;
  };

  /** Media processing workspace */
  media: {
    root: string;
    temp: string;
  };
}

/**
 * Create path tree for ANY base location.
 * This factory ensures SAME structure regardless of $HOME vs $REPO.
 */
export function createPathsForBase(baseRoot: string): ContinuumPaths {
  return {
    root: baseRoot,

    database: {
      root: path.join(baseRoot, 'data'),
      main: path.join(baseRoot, 'data', 'database.sqlite'),
      backup: path.join(baseRoot, 'data', 'backups'),
    },

    logs: {
      root: path.join(baseRoot, 'jtag', 'logs'),

      personas: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'logs');
      },

      subsystem: (uniqueId: string, subsystem: 'mind' | 'body' | 'soul' | 'cns'): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'logs', `${subsystem}.log`);
      },

      system: path.join(baseRoot, 'jtag', 'logs', 'system'),
      sql: path.join(baseRoot, 'jtag', 'logs', 'system', 'data', 'sql.log'),
      errors: path.join(baseRoot, 'jtag', 'logs', 'system', 'core', 'errors.log'),
    },

    sessions: {
      root: path.join(baseRoot, 'jtag', 'sessions'),
      user: path.join(baseRoot, 'jtag', 'sessions', 'user'),
      validation: path.join(baseRoot, 'jtag', 'sessions', 'validation'),
    },

    registry: {
      root: path.join(baseRoot, 'jtag', 'registry'),
      processes: path.join(baseRoot, 'jtag', 'registry', 'process-registry.json'),
      ports: path.join(baseRoot, 'jtag', 'registry', 'dynamic-ports.json'),
    },

    temp: {
      root: path.join(baseRoot, 'jtag', 'temp'),
      screenshots: path.join(baseRoot, 'jtag', 'temp', 'screenshots'),
      artifacts: path.join(baseRoot, 'jtag', 'temp', 'artifacts'),
    },

    genome: {
      root: path.join(baseRoot, 'genome'),
      adapters: path.join(baseRoot, 'genome', 'adapters'),
      training: path.join(baseRoot, 'genome', 'training-data'),
      packages: path.join(baseRoot, 'genome', 'packages'),
      python: path.join(baseRoot, 'genome', 'python'),
    },

    personas: {
      root: path.join(baseRoot, 'personas'),

      dir: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId);
      },

      data: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'data');
      },

      logs: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'logs');
      },

      state: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'data', 'state.db');
      },

      memory: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'data', 'memory.db');
      },

      longterm: (uniqueId: string): string => {
        return path.join(baseRoot, 'personas', uniqueId, 'data', 'longterm.db');
      },
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
      },
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
      },
    },

    blobs: {
      root: path.join(baseRoot, 'blobs'),
    },

    shared: {
      root: path.join(baseRoot, 'shared'),
      rooms: (roomId: string): string => {
        return path.join(baseRoot, 'shared', 'rooms', roomId);
      },
    },

    reports: {
      root: path.join(baseRoot, 'reports'),
    },

    models: {
      root: path.join(baseRoot, 'models'),
    },

    sockets: {
      root: path.join(baseRoot, 'sockets'),
      core: path.join(baseRoot, 'sockets', 'core.sock'),
      archive: path.join(baseRoot, 'sockets', 'archive.sock'),
      inference: path.join(baseRoot, 'sockets', 'inference.sock'),
    },

    datasets: {
      root: path.join(baseRoot, 'datasets'),
      parsed: path.join(baseRoot, 'datasets', 'parsed'),
      prepared: path.join(baseRoot, 'datasets', 'prepared'),
    },

    media: {
      root: path.join(baseRoot, 'media'),
      temp: path.join(baseRoot, 'media', 'temp'),
    },
  };
}

/**
 * Create a composite path tree that routes persistent data to one root
 * and runtime data to another.
 *
 * Persistent ($HOME): database, genome, personas, users, agents, blobs, models, datasets
 * Runtime ($REPO): logs, sessions, registry, temp, sockets, shared, reports, media
 */
export function createCompositeSystemPaths(
  persistentRoot: string,
  runtimeRoot: string,
): ContinuumPaths {
  const home = createPathsForBase(persistentRoot);
  const repo = createPathsForBase(runtimeRoot);

  return {
    root: runtimeRoot,

    // PERSISTENT → $HOME
    database: home.database,
    genome: home.genome,
    personas: home.personas,
    users: home.users,
    agents: home.agents,
    blobs: home.blobs,
    models: home.models,
    datasets: home.datasets,

    // RUNTIME → $REPO
    logs: repo.logs,
    sessions: repo.sessions,
    registry: repo.registry,
    temp: repo.temp,
    sockets: repo.sockets,
    shared: repo.shared,
    reports: repo.reports,
    media: repo.media,
  };
}

/**
 * Base locations - THE ONLY PLACES these strings should exist
 */
const REPO_ROOT = path.join(process.cwd(), '.continuum');
const HOME_ROOT = path.join(os.homedir(), '.continuum');

/**
 * Default paths — composite: persistent data in $HOME, runtime data in $REPO.
 * 25+ consumers import this. The composite routing is transparent.
 */
export const SystemPaths: ContinuumPaths = createCompositeSystemPaths(HOME_ROOT, REPO_ROOT);

/**
 * Global paths ($HOME) - use for shared/user-global resources
 * Example: shared personas, global genomes, user preferences
 */
export const GlobalPaths: ContinuumPaths = createPathsForBase(HOME_ROOT);

/**
 * Project paths ($REPO) - explicit repo-local tree
 * Use when you specifically need a repo-relative path regardless of composite routing.
 */
export const ProjectPaths: ContinuumPaths = createPathsForBase(REPO_ROOT);
