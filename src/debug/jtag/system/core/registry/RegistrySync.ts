/**
 * Registry State Synchronization - Distributed State Coordination
 * 
 * Solves the mind puzzle: Different components run from different working directories
 * but need to share the same process registry state for security consistency.
 * 
 * This is a distributed systems consistency problem disguised as a path issue.
 */

import fs from 'fs/promises';
import path from 'path';

/**
 * Registry locations that need to be synchronized
 */
const REGISTRY_LOCATIONS = [
  '.continuum/jtag/registry/process-registry.json',  // Main JTAG directory
  'examples/test-bench/.continuum/jtag/registry/process-registry.json'  // Test-bench directory
];

/**
 * Registry state structure
 */
interface RegistryState {
  registryVersion: string;
  lastUpdate: number; 
  processes: Record<string, any>;
}

/**
 * Synchronize registry state across all locations
 * Finds the most recent registry and propagates it to all locations
 */
export async function syncRegistryState(): Promise<RegistryState | null> {
  const registries: Array<{ location: string; state: RegistryState; lastUpdate: number }> = [];
  
  // Read all registry files
  for (const location of REGISTRY_LOCATIONS) {
    try {
      if (await fileExists(location)) {
        const content = await fs.readFile(location, 'utf-8');
        const state = JSON.parse(content) as RegistryState;
        registries.push({ location, state, lastUpdate: state.lastUpdate });
      }
    } catch (error) {
      console.warn(`⚠️  Registry Sync: Could not read ${location}: ${error}`);
    }
  }
  
  if (registries.length === 0) {
    console.log('📋 Registry Sync: No registries found');
    return null;
  }
  
  // Find the most recent registry (source of truth)
  const mostRecent = registries.reduce((latest, current) => 
    current.lastUpdate > latest.lastUpdate ? current : latest
  );
  
  console.log(`🔄 Registry Sync: Using ${mostRecent.location} as source of truth (${new Date(mostRecent.lastUpdate).toISOString()})`);
  
  // Propagate the most recent state to all locations
  for (const location of REGISTRY_LOCATIONS) {
    try {
      await ensureDirectoryExists(path.dirname(location));
      await fs.writeFile(location, JSON.stringify(mostRecent.state, null, 2), 'utf-8');
      console.log(`✅ Registry Sync: Synchronized ${location}`);
    } catch (error) {
      console.warn(`⚠️  Registry Sync: Could not write ${location}: ${error}`);
    }
  }
  
  const processCount = Object.keys(mostRecent.state.processes).length;
  console.log(`🎯 Registry Sync: Synchronized ${processCount} processes across all locations`);
  
  return mostRecent.state;
}

/**
 * Get canonical registry location for this execution context
 */
export function getCanonicalRegistryLocation(): string {
  // Always use test-bench location as canonical since that's where the system runs
  return 'examples/test-bench/.continuum/jtag/registry/process-registry.json';
}

/**
 * Helper: Check if file exists
 */
async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

/**
 * Helper: Ensure directory exists
 */
async function ensureDirectoryExists(dirPath: string): Promise<void> {
  try {
    await fs.mkdir(dirPath, { recursive: true });
  } catch (error: any) {
    if (error.code !== 'EEXIST') {
      throw error;
    }
  }
}