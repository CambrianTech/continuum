/**
 * Core Modules - Centralized Module Management System
 * 
 * Universal module discovery, utilities, and management for Continuum
 * Used by testing, integrations, build systems, and runtime module loading
 */

export { ModuleDiscovery, moduleDiscovery } from './discovery.js';
export { ModuleUtils } from './utils.js';
export type { ModuleType, ModuleInfo, ModuleDependency } from './discovery.js';