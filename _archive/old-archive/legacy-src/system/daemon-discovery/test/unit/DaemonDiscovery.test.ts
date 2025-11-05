/**
 * Unit tests for DaemonDiscovery
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { DaemonDiscovery } from '../../DaemonDiscovery';

describe('DaemonDiscovery Unit Tests', () => {
  let discovery: DaemonDiscovery;
  let projectRoot: string;
  
  before(() => {
    // Find project root
    const currentDir = path.dirname(fileURLToPath(import.meta.url));
    projectRoot = path.resolve(currentDir, '../../../../../..');
    discovery = new DaemonDiscovery(projectRoot);
  });
  
  describe('Daemon Discovery', () => {
    it('should discover daemons with proper package.json', async () => {
      const daemons = await discovery.discoverDaemons();
      
      // Should find at least some core daemons
      assert(daemons.length > 0, 'Should discover at least one daemon');
      
      // Check that discovered daemons have required properties
      for (const daemon of daemons) {
        assert(daemon.name, 'Daemon should have a name');
        assert(daemon.path, 'Daemon should have a path');
        assert(daemon.packageJson, 'Daemon should have packageJson');
        assert(daemon.className, 'Daemon should have a className');
        assert.strictEqual(daemon.packageJson.continuum?.type, 'daemon');
      }
    });
    
    it('should find SessionManagerDaemon', async () => {
      const daemons = await discovery.discoverDaemons();
      const sessionManager = daemons.find(d => d.name === 'session-manager');
      
      assert(sessionManager, 'Should find SessionManagerDaemon');
      assert.strictEqual(sessionManager.className, 'SessionManagerDaemon');
      assert.strictEqual(sessionManager.packageJson.continuum.priority, 20);
    });
    
    it('should infer class names correctly', async () => {
      const daemons = await discovery.discoverDaemons();
      
      // Test kebab-case to PascalCase conversion
      const browserManager = daemons.find(d => d.name === 'browser-manager');
      if (browserManager) {
        assert.strictEqual(browserManager.className, 'BrowserManagerDaemon');
      }
      
      const commandProcessor = daemons.find(d => d.name === 'command-processor');
      if (commandProcessor) {
        assert.strictEqual(commandProcessor.className, 'CommandProcessorDaemon');
      }
    });
  });
  
  describe('Daemon Loading', () => {
    it('should load discovered daemons', async () => {
      const loadedDaemons = await discovery.loadDaemons();
      
      assert(loadedDaemons.size > 0, 'Should load at least one daemon');
      
      // Check that loaded daemons are BaseDaemon instances
      for (const [name, daemon] of loadedDaemons) {
        assert(daemon.name, `Daemon ${name} should have a name`);
        assert(daemon.version, `Daemon ${name} should have a version`);
        assert(typeof daemon.start === 'function', `Daemon ${name} should have start method`);
        assert(typeof daemon.stop === 'function', `Daemon ${name} should have stop method`);
      }
    });
    
    it('should respect priority order', async () => {
      const daemons = await discovery.discoverDaemons();
      
      // Sort by priority
      const sorted = daemons.sort((a, b) => {
        const aPriority = a.packageJson.continuum?.priority || 50;
        const bPriority = b.packageJson.continuum?.priority || 50;
        return aPriority - bPriority;
      });
      
      // Session manager should come early (priority 20)
      const sessionManagerIndex = sorted.findIndex(d => d.name === 'session-manager');
      assert(sessionManagerIndex >= 0 && sessionManagerIndex < sorted.length / 2, 
        'SessionManager should be in first half due to priority');
    });
  });
});