/**
 * ProcessRegistry Tests
 * Test auto-discovery and daemon registration
 */

import { ProcessRegistry } from './ProcessRegistry.js';
import { ProcessConfig } from '../interfaces/IProcessCoordinator.js';
import { mkdtemp, writeFile, mkdir } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';
import { rimraf } from 'rimraf';

describe('ProcessRegistry', () => {
  let registry: ProcessRegistry;
  let tempDir: string;

  beforeEach(async () => {
    registry = new ProcessRegistry([]);
    tempDir = await mkdtemp(join(tmpdir(), 'continuum-test-'));
  });

  afterEach(async () => {
    try {
      await rimraf(tempDir);
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('Daemon Discovery', () => {
    test('should discover valid daemon packages', async () => {
      // Create test daemon directory
      const daemonDir = join(tempDir, 'test-daemon');
      await mkdir(daemonDir, { recursive: true });

      // Create package.json
      const packageJson = {
        name: 'test-daemon',
        version: '1.0.0',
        daemon: {
          type: 'test-daemon',
          capabilities: ['test', 'demo'],
          entryPoint: 'index.ts'
        }
      };

      await writeFile(
        join(daemonDir, 'package.json'),
        JSON.stringify(packageJson, null, 2)
      );

      // Create entry point
      await writeFile(
        join(daemonDir, 'index.ts'),
        'export class TestDaemon {}'
      );

      // Discover daemons
      const discovered = await registry.discoverProcesses(tempDir);

      expect(discovered.size).toBe(1);
      expect(discovered.has('test-daemon')).toBe(true);

      const entry = discovered.get('test-daemon')!;
      expect(entry.type).toBe('test-daemon');
      expect(entry.config.capabilities).toEqual(['test', 'demo']);
      expect(entry.config.entryPoint).toMatch(/index\.ts$/);
    });

    test('should ignore directories without package.json', async () => {
      const noPackageDir = join(tempDir, 'no-package');
      await mkdir(noPackageDir, { recursive: true });
      await writeFile(join(noPackageDir, 'index.ts'), 'export class Test {}');

      const discovered = await registry.discoverProcesses(tempDir);

      expect(discovered.size).toBe(0);
    });

    test('should ignore packages without daemon config', async () => {
      const regularDir = join(tempDir, 'regular-package');
      await mkdir(regularDir, { recursive: true });

      const packageJson = {
        name: 'regular-package',
        version: '1.0.0'
        // No daemon config
      };

      await writeFile(
        join(regularDir, 'package.json'),
        JSON.stringify(packageJson, null, 2)
      );

      const discovered = await registry.discoverProcesses(tempDir);

      expect(discovered.size).toBe(0);
    });

    test('should find alternative entry points', async () => {
      const daemonDir = join(tempDir, 'alt-entry-daemon');
      await mkdir(daemonDir, { recursive: true });

      const packageJson = {
        name: 'alt-entry-daemon',
        version: '1.0.0',
        daemon: {
          type: 'alt-entry-daemon',
          capabilities: ['alt']
          // No entryPoint specified
        }
      };

      await writeFile(
        join(daemonDir, 'package.json'),
        JSON.stringify(packageJson, null, 2)
      );

      // Create daemon.ts as alternative entry point
      await writeFile(
        join(daemonDir, 'daemon.ts'),
        'export class AltEntryDaemon {}'
      );

      const discovered = await registry.discoverProcesses(tempDir);

      expect(discovered.size).toBe(1);
      
      const entry = discovered.get('alt-entry-daemon')!;
      expect(entry.config.entryPoint).toMatch(/daemon\.ts$/);
    });

    test('should handle multiple daemons in subdirectories', async () => {
      // Create multiple daemon directories
      const daemons = ['daemon-a', 'daemon-b', 'daemon-c'];
      
      for (const daemonName of daemons) {
        const daemonDir = join(tempDir, daemonName);
        await mkdir(daemonDir, { recursive: true });

        const packageJson = {
          name: daemonName,
          version: '1.0.0',
          daemon: {
            type: daemonName,
            capabilities: [daemonName.replace('daemon-', '')]
          }
        };

        await writeFile(
          join(daemonDir, 'package.json'),
          JSON.stringify(packageJson, null, 2)
        );

        await writeFile(
          join(daemonDir, 'index.ts'),
          `export class ${daemonName.split('-').map(s => s.charAt(0).toUpperCase() + s.slice(1)).join('')}Daemon {}`
        );
      }

      const discovered = await registry.discoverProcesses(tempDir);

      expect(discovered.size).toBe(3);
      for (const daemonName of daemons) {
        expect(discovered.has(daemonName)).toBe(true);
      }
    });
  });

  describe('Configuration Access', () => {
    test('should return available daemon configurations', async () => {
      // Set up test daemon
      const daemonDir = join(tempDir, 'config-test-daemon');
      await mkdir(daemonDir, { recursive: true });

      const packageJson = {
        name: 'config-test-daemon',
        version: '1.0.0',
        daemon: {
          type: 'config-test',
          capabilities: ['config', 'test'],
          maxMemory: 512,
          maxCpu: 0.5
        }
      };

      await writeFile(
        join(daemonDir, 'package.json'),
        JSON.stringify(packageJson, null, 2)
      );

      await writeFile(join(daemonDir, 'index.ts'), 'export class ConfigTestDaemon {}');
      await registry.discoverProcesses(tempDir);

      const available = registry.getAvailable();
      expect(available.size).toBe(1);

      const entry = available.get('config-test')!;
      expect(entry.config.maxMemory).toBe(512);
      expect(entry.config.maxCpu).toBe(0.5);
      expect(entry.packageJson.name).toBe('config-test-daemon');
    });

    test('should get specific daemon config', async () => {
      // Set up test daemon
      const daemonDir = join(tempDir, 'specific-daemon');
      await mkdir(daemonDir, { recursive: true });

      const packageJson = {
        name: 'specific-daemon',
        daemon: {
          type: 'specific',
          capabilities: ['specific-capability']
        }
      };

      await writeFile(
        join(daemonDir, 'package.json'),
        JSON.stringify(packageJson, null, 2)
      );

      await writeFile(join(daemonDir, 'index.ts'), 'export class SpecificDaemon {}');
      await registry.discoverProcesses(tempDir);

      const config = registry.getConfig('specific');
      expect(config).not.toBeNull();
      expect(config!.type).toBe('specific');
      expect(config!.capabilities).toEqual(['specific-capability']);

      const nonExistent = registry.getConfig('non-existent');
      expect(nonExistent).toBeNull();
    });
  });

  describe('Capability Queries', () => {
    beforeEach(async () => {
      // Set up multiple test daemons with different capabilities
      const daemons = [
        { name: 'ui-daemon', caps: ['ui', 'rendering'] },
        { name: 'api-daemon', caps: ['api', 'http'] },
        { name: 'multi-daemon', caps: ['ui', 'api', 'storage'] }
      ];

      for (const daemon of daemons) {
        const daemonDir = join(tempDir, daemon.name);
        await mkdir(daemonDir, { recursive: true });

        const packageJson = {
          name: daemon.name,
          daemon: {
            type: daemon.name,
            capabilities: daemon.caps
          }
        };

        await writeFile(
          join(daemonDir, 'package.json'),
          JSON.stringify(packageJson, null, 2)
        );

        await writeFile(join(daemonDir, 'index.ts'), `export class ${daemon.name}Class {}`);
      }

      await registry.discoverProcesses(tempDir);
    });

    test('should return all capabilities', () => {
      const capabilities = registry.getCapabilities();
      
      expect(capabilities.size).toBe(3);
      expect(capabilities.get('ui-daemon')).toEqual(['ui', 'rendering']);
      expect(capabilities.get('api-daemon')).toEqual(['api', 'http']);
      expect(capabilities.get('multi-daemon')).toEqual(['ui', 'api', 'storage']);
    });

    test('should find daemons by capability', () => {
      const uiDaemons = registry.findByCapability('ui');
      expect(uiDaemons).toEqual(['ui-daemon', 'multi-daemon']);

      const apiDaemons = registry.findByCapability('api');
      expect(apiDaemons).toEqual(['api-daemon', 'multi-daemon']);

      const storageDaemons = registry.findByCapability('storage');
      expect(storageDaemons).toEqual(['multi-daemon']);

      const nonExistent = registry.findByCapability('non-existent');
      expect(nonExistent).toEqual([]);
    });
  });

  describe('Configuration Validation', () => {
    test('should validate complete configuration', () => {
      const validConfig: ProcessConfig = {
        type: 'test-daemon',
        entryPoint: '/path/to/entry.ts',
        capabilities: ['test'],
        packagePath: '/path/to/package'
      };

      expect(registry.validateConfig(validConfig)).toBe(true);
    });

    test('should reject configuration missing required fields', () => {
      const missingType = {
        entryPoint: '/path/to/entry.ts',
        capabilities: ['test'],
        packagePath: '/path/to/package'
      } as ProcessConfig;

      expect(registry.validateConfig(missingType)).toBe(false);

      const missingEntryPoint = {
        type: 'test',
        capabilities: ['test'],
        packagePath: '/path/to/package'
      } as ProcessConfig;

      expect(registry.validateConfig(missingEntryPoint)).toBe(false);

      const missingCapabilities = {
        type: 'test',
        entryPoint: '/path/to/entry.ts',
        packagePath: '/path/to/package'
      } as ProcessConfig;

      expect(registry.validateConfig(missingCapabilities)).toBe(false);
    });

    test('should reject invalid capabilities format', () => {
      const invalidCapabilities = {
        type: 'test',
        entryPoint: '/path/to/entry.ts',
        capabilities: 'not-an-array',
        packagePath: '/path/to/package'
      } as any;

      expect(registry.validateConfig(invalidCapabilities)).toBe(false);
    });
  });
});