/**
 * DaemonTypes - Central type definitions for the daemon system
 * 
 * All daemon-related types defined once and used everywhere
 */

import { BaseDaemon } from '../daemons/base/BaseDaemon';

/**
 * Daemon package.json continuum configuration
 */
export interface DaemonPackageConfig {
  type: 'daemon';
  className?: string;
  priority?: number;
  dependencies?: string[];
  autoStart?: boolean;
  singleton?: boolean;
}

/**
 * Daemon discovery information
 */
export interface DaemonInfo {
  name: string;
  path: string;
  packageJson: {
    name: string;
    version: string;
    main?: string;
    continuum: DaemonPackageConfig;
  };
  className: string;
}

/**
 * Daemon constructor type
 */
export interface DaemonConstructor {
  new(): BaseDaemon;
}

/**
 * Daemon module exports
 */
export interface DaemonModule {
  default?: DaemonConstructor;
  [key: string]: DaemonConstructor | undefined;
}

/**
 * Daemon startup configuration
 */
export interface DaemonStartupConfig {
  discoveryPath?: string;
  autoStart?: boolean;
  startupOrder?: string[];
  excludeDaemons?: string[];
}

/**
 * Daemon registry entry
 */
export interface DaemonRegistryEntry {
  daemon: BaseDaemon;
  info: DaemonInfo;
  status: 'stopped' | 'starting' | 'running' | 'stopping' | 'failed';
  startTime?: Date;
  error?: Error;
}