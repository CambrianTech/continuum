/**
 * Integration tests for daemon module structure validation
 * Ensures all daemons follow the modular architecture pattern
 */

import { describe, it, before } from 'node:test';
import assert from 'node:assert';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

describe('Daemon Module Structure Integration Tests', () => {
  let daemonsPath: string;
  let daemonDirs: string[] = [];
  
  before(async () => {
    // Find daemons directory
    const currentDir = path.dirname(fileURLToPath(import.meta.url));
    daemonsPath = path.resolve(currentDir, '../../daemons');
    
    // Get all daemon directories
    const entries = await fs.promises.readdir(daemonsPath, { withFileTypes: true });
    daemonDirs = entries
      .filter(entry => entry.isDirectory())
      .filter(entry => !entry.name.startsWith('.'))
      .filter(entry => entry.name !== 'base') // base is not a daemon
      .map(entry => entry.name);
  });
  
  describe('Module Structure Compliance', () => {
    it('should have at least one daemon to test', () => {
      assert(daemonDirs.length > 0, 'No daemon directories found');
    });
    
    daemonDirs.forEach(daemonName => {
      describe(`${daemonName} daemon`, () => {
        const daemonPath = path.join(daemonsPath, daemonName);
        
        it('should have package.json', async () => {
          const packageJsonPath = path.join(daemonPath, 'package.json');
          assert(fs.existsSync(packageJsonPath), `${daemonName} missing package.json`);
          
          // Validate package.json content
          const content = await fs.promises.readFile(packageJsonPath, 'utf-8');
          const packageJson = JSON.parse(content);
          
          assert(packageJson.name, 'package.json must have name');
          assert(packageJson.version, 'package.json must have version');
          assert(packageJson.continuum, 'package.json must have continuum section');
          assert.strictEqual(
            packageJson.continuum.type, 
            'daemon',
            'continuum.type must be "daemon"'
          );
        });
        
        it('should have daemon implementation file', async () => {
          // Look for main daemon file
          const possibleFiles = [
            `${toPascalCase(daemonName)}Daemon.ts`,
            `${daemonName}.ts`,
            'index.ts'
          ];
          
          const foundFiles = possibleFiles.filter(file => 
            fs.existsSync(path.join(daemonPath, file))
          );
          
          assert(
            foundFiles.length > 0,
            `${daemonName} missing daemon implementation. Expected one of: ${possibleFiles.join(', ')}`
          );
        });
        
        it('should extend BaseDaemon', async () => {
          // Find the main daemon file
          const possibleFiles = [
            `${toPascalCase(daemonName)}Daemon.ts`,
            `${daemonName}.ts`,
            'index.ts'
          ];
          
          for (const file of possibleFiles) {
            const filePath = path.join(daemonPath, file);
            if (fs.existsSync(filePath)) {
              const content = await fs.promises.readFile(filePath, 'utf-8');
              
              // Check for BaseDaemon extension
              assert(
                content.includes('extends BaseDaemon'),
                `${daemonName} must extend BaseDaemon`
              );
              
              // Check for required method implementations
              assert(
                content.includes('async onStart()') || content.includes('async onStart():'),
                `${daemonName} must implement onStart() method`
              );
              
              assert(
                content.includes('async onStop()') || content.includes('async onStop():'),
                `${daemonName} must implement onStop() method`
              );
              
              break;
            }
          }
        });
        
        it('should have test directory structure', () => {
          const testPath = path.join(daemonPath, 'test');
          
          // Test directory is optional but recommended
          if (fs.existsSync(testPath)) {
            const unitPath = path.join(testPath, 'unit');
            const integrationPath = path.join(testPath, 'integration');
            
            assert(
              fs.existsSync(unitPath) || fs.existsSync(integrationPath),
              `${daemonName}/test should have unit/ or integration/ subdirectories`
            );
          }
        });
        
        it('should not have cross-cutting dependencies', async () => {
          // Check main daemon file for inappropriate imports
          const possibleFiles = [
            `${toPascalCase(daemonName)}Daemon.ts`,
            `${daemonName}.ts`,
            'index.ts'
          ];
          
          for (const file of possibleFiles) {
            const filePath = path.join(daemonPath, file);
            if (fs.existsSync(filePath)) {
              const content = await fs.promises.readFile(filePath, 'utf-8');
              
              // Check for cross-cutting imports
              const badImports = [
                'from \'../../core/',
                'from \'../../tools/',
                'from \'../../data/',
                'from \'../../../core/',
                'from \'../../../tools/',
                'from \'../../../data/'
              ];
              
              for (const badImport of badImports) {
                assert(
                  !content.includes(badImport),
                  `${daemonName} has cross-cutting import: ${badImport}`
                );
              }
              
              break;
            }
          }
        });
        
        it('should have proper daemon naming convention', () => {
          const className = toPascalCase(daemonName) + 'Daemon';
          const possibleFiles = [
            `${className}.ts`,
            `${daemonName}.ts`,
            'index.ts'
          ];
          
          const foundFiles = possibleFiles.filter(file => 
            fs.existsSync(path.join(daemonPath, file))
          );
          
          // Prefer PascalCase naming
          if (foundFiles.includes(`${className}.ts`)) {
            assert(true, `${daemonName} follows naming convention`);
          } else {
            console.warn(`${daemonName} should be named ${className}.ts for consistency`);
          }
        });
      });
    });
  });
  
  describe('Daemon Registration', () => {
    it('should be discoverable by DaemonDiscovery', async () => {
      const { DaemonDiscovery } = await import('../../system/daemon-discovery/DaemonDiscovery');
      const discovery = new DaemonDiscovery();
      
      const discovered = await discovery.discoverDaemons();
      const discoveredNames = discovered.map(d => d.name);
      
      console.log('Expected daemons:', daemonDirs);
      console.log('Discovered daemons:', discoveredNames);
      console.log('Missing daemons:', daemonDirs.filter(d => !discoveredNames.includes(d)));
      
      // All daemon directories should be discovered
      for (const daemonName of daemonDirs) {
        assert(
          discoveredNames.includes(daemonName),
          `${daemonName} should be discovered by DaemonDiscovery`
        );
      }
    });
  });
});

// Helper function to convert kebab-case to PascalCase
function toPascalCase(str: string): string {
  return str
    .split('-')
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}