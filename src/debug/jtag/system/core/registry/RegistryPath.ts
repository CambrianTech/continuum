/**
 * Registry Path Resolution - Centralized registry path management
 * 
 * Ensures all parts of the system use the same registry location regardless of working directory.
 */

import path from 'path';
import fs from 'fs';

/**
 * Get the canonical registry directory path
 * Always returns absolute path to avoid working directory issues
 */
export function getRegistryDir(): string {
  // Find the JTAG root directory (contains package.json)
  let currentDir = process.cwd();
  const maxLevels = 10; // Prevent infinite loops
  
  for (let i = 0; i < maxLevels; i++) {
    const packagePath = path.join(currentDir, 'package.json');
    if (fs.existsSync(packagePath)) {
      // Check if this is the JTAG package.json by looking for JTAG-specific content
      try {
        const packageContent = fs.readFileSync(packagePath, 'utf-8');
        if (packageContent.includes('@continuum/jtag') || 
            packageContent.includes('jtag-system') ||
            fs.existsSync(path.join(currentDir, 'system', 'core'))) {
          // Found JTAG root
          return path.join(currentDir, '.continuum', 'jtag', 'registry');
        }
      } catch {
        // Continue searching
      }
    }
    
    const parentDir = path.dirname(currentDir);
    if (parentDir === currentDir) break; // Reached filesystem root
    currentDir = parentDir;
  }
  
  // Fallback: use current directory
  console.warn('⚠️  Registry Path: Could not find JTAG root, using current directory');
  return path.join(process.cwd(), '.continuum', 'jtag', 'registry');
}

/**
 * Get the canonical registry file path
 */
export function getRegistryFile(): string {
  return path.join(getRegistryDir(), 'process-registry.json');
}

/**
 * Ensure registry directory exists
 */
export function ensureRegistryDir(): void {
  const registryDir = getRegistryDir();
  if (!fs.existsSync(registryDir)) {
    fs.mkdirSync(registryDir, { recursive: true });
  }
}