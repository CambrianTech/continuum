/**
 * Bootstrap System Tests
 * Test the core OS bootstrap commands and initialization chain
 */

import { BootstrapSystem } from '../BootstrapSystem.js';
import { CoreCommand, CommandCategory, CoreCommandUtils } from '../types/CoreCommands.js';

describe('Bootstrap System', () => {
  let bootstrapSystem: BootstrapSystem;

  beforeEach(() => {
    bootstrapSystem = new BootstrapSystem();
  });

  afterEach(async () => {
    // Cleanup if needed
  });

  describe('System Initialization', () => {
    test('should initialize successfully', async () => {
      await bootstrapSystem.start();
      
      expect(bootstrapSystem.isReady()).toBe(true);
      
      const systemState = bootstrapSystem.getSystemState();
      expect(systemState.phase).toBe('ready');
      expect(systemState.systemReady).toBe(true);
    }, 10000);

    test('should emit bootstrap-ready event when ready', async () => {
      const readyPromise = new Promise<void>((resolve) => {
        bootstrapSystem.once('bootstrap-ready', resolve);
      });

      await bootstrapSystem.start();
      await readyPromise;
      
      expect(bootstrapSystem.isReady()).toBe(true);
    }, 10000);
  });

  describe('Core Command Enum', () => {
    test('should have all required core commands', () => {
      const requiredCommands = [
        CoreCommand.INFO,
        CoreCommand.STATUS, 
        CoreCommand.LIST,
        CoreCommand.HELP,
        CoreCommand.FILESAVE
      ];
      
      requiredCommands.forEach(command => {
        expect(Object.values(CoreCommand)).toContain(command);
      });
    });

    test('should categorize commands correctly', () => {
      const immediateCommands = CoreCommandUtils.getImmediateCommands();
      const postDiscoveryCommands = CoreCommandUtils.getPostDiscoveryCommands();
      
      expect(immediateCommands).toContain(CoreCommand.INFO);
      expect(immediateCommands).toContain(CoreCommand.STATUS);
      expect(postDiscoveryCommands).toContain(CoreCommand.LIST);
      expect(postDiscoveryCommands).toContain(CoreCommand.HELP);
    });

    test('should validate core commands correctly', () => {
      expect(CoreCommandUtils.isCoreCommand('info')).toBe(true);
      expect(CoreCommandUtils.isCoreCommand('list')).toBe(true);
      expect(CoreCommandUtils.isCoreCommand('nonexistent')).toBe(false);
      
      expect(() => CoreCommandUtils.validateCoreCommand('info')).not.toThrow();
      expect(() => CoreCommandUtils.validateCoreCommand('invalid')).toThrow();
    });
  });

  describe('Immediate Commands', () => {
    test('should execute info command before initialization', async () => {
      // Don't start the system - test immediate availability
      const result = await bootstrapSystem.executeCommand('info', {});
      
      expect(result.success).toBe(true);
      expect(result.data.version).toBeDefined();
      expect(result.data.system).toBeDefined();
      expect(result.data.server).toBeDefined();
      expect(result.data.processedBy).toBe('bootstrap-command-processor');
    });

    test('should execute status command before initialization', async () => {
      const result = await bootstrapSystem.executeCommand('status', {});
      
      expect(result.success).toBe(true);
      expect(result.data.systemReady).toBeDefined();
      expect(result.data.modulesDiscovered).toBeDefined();
      expect(result.data.processedBy).toBe('bootstrap-command-processor');
    });
  });

  describe('Post-Discovery Commands', () => {
    test('should queue list command until system ready', async () => {
      // Execute list before system initialization
      const listPromise = bootstrapSystem.executeCommand('list', {});
      
      // Start system initialization 
      await bootstrapSystem.start();
      
      // List command should now resolve
      const result = await listPromise;
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
      expect(result.data.bootstrapCommands).toBeDefined();
      expect(result.data.discoveredCommands).toBeDefined();
      expect(result.data.systemReady).toBe(true);
    }, 10000);

    test('should execute list command immediately after initialization', async () => {
      await bootstrapSystem.start();
      
      const result = await bootstrapSystem.executeCommand('list', {});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toContain('info');
      expect(result.data.commands).toContain('list');
      expect(result.data.commands).toContain('help');
      expect(result.data.totalCommands).toBeGreaterThan(0);
    });

    test('should execute help command after initialization', async () => {
      await bootstrapSystem.start();
      
      // General help
      const generalHelp = await bootstrapSystem.executeCommand('help', {});
      expect(generalHelp.success).toBe(true);
      expect(generalHelp.data.availableCommands).toBeDefined();
      
      // Specific command help
      const specificHelp = await bootstrapSystem.executeCommand('help', { command: 'info' });
      expect(specificHelp.success).toBe(true);
      expect(specificHelp.data.command).toBe('info');
      expect(specificHelp.data.description).toBeDefined();
    });
  });

  describe('Command Queueing', () => {
    test('should queue multiple commands and process them in order', async () => {
      const promises = [
        bootstrapSystem.executeCommand('list', {}),
        bootstrapSystem.executeCommand('help', {}),
        bootstrapSystem.executeCommand('info', {}), // This should execute immediately
      ];
      
      // Start system after queueing commands
      await bootstrapSystem.start();
      
      const results = await Promise.all(promises);
      
      // All commands should succeed
      results.forEach(result => {
        expect(result.success).toBe(true);
      });
      
      // List and help should have been queued, info should be immediate
      expect(results[0].data.commands).toBeDefined(); // list result
      expect(results[1].data.availableCommands).toBeDefined(); // help result  
      expect(results[2].data.version).toBeDefined(); // info result
    }, 10000);

    test('should handle command queue size correctly', async () => {
      const initialState = bootstrapSystem.getSystemState();
      expect(initialState.queuedCommands).toBe(0);
      
      // Queue some commands
      const promises = [
        bootstrapSystem.executeCommand('list', {}),
        bootstrapSystem.executeCommand('help', {})
      ];
      
      // Check queue size increased
      const queuedState = bootstrapSystem.getSystemState();
      expect(queuedState.queuedCommands).toBeGreaterThan(0);
      
      // Start system and wait for completion
      await bootstrapSystem.start();
      await Promise.all(promises);
      
      // Queue should be empty
      const completedState = bootstrapSystem.getSystemState();
      expect(completedState.queuedCommands).toBe(0);
    }, 10000);
  });

  describe('Error Handling', () => {
    test('should handle unknown commands gracefully', async () => {
      await expect(bootstrapSystem.executeCommand('unknown-command', {}))
        .rejects.toThrow('Bootstrap command not found: unknown-command');
    });

    test('should handle commands that require unavailable system state', async () => {
      // Try to execute a post-discovery command on a system that's not ready
      // but bypass the queuing mechanism by testing the registry directly
      const bootstrapCommandRegistry = (bootstrapSystem as any).commandRegistry;
      const systemState = {
        modulesDiscovered: false,
        commandsLoaded: false,
        daemonsReady: false,
        discoveredCommands: [],
        fileSystemReady: true
      };
      
      await expect(bootstrapCommandRegistry.executeCommand('list', {}, systemState))
        .rejects.toThrow('Command list cannot execute in current system state');
    });
  });

  describe('System State Tracking', () => {
    test('should track initialization phases correctly', async () => {
      const initialState = bootstrapSystem.getSystemState();
      expect(initialState.phase).toBe('starting');
      expect(initialState.systemReady).toBe(false);
      
      await bootstrapSystem.start();
      
      const readyState = bootstrapSystem.getSystemState();
      expect(readyState.phase).toBe('ready');
      expect(readyState.systemReady).toBe(true);
      expect(readyState.initializationTime).toBeGreaterThan(0);
    }, 10000);

    test('should provide available commands based on system state', async () => {
      const initialCommands = bootstrapSystem.getSystemState().commandsAvailable;
      expect(initialCommands).toContain('info');
      expect(initialCommands).toContain('status');
      
      await bootstrapSystem.start();
      
      const readyCommands = bootstrapSystem.getSystemState().commandsAvailable;
      expect(readyCommands).toContain('info');
      expect(readyCommands).toContain('status');
      expect(readyCommands).toContain('list');
      expect(readyCommands).toContain('help');
    });
  });

  describe('Event System', () => {
    test('should emit system-ready event when initialization completes', async () => {
      let systemReadyFired = false;
      
      bootstrapSystem.once('system-ready', () => {
        systemReadyFired = true;
      });
      
      await bootstrapSystem.start();
      
      expect(systemReadyFired).toBe(true);
    }, 10000);

    test('should emit phase-complete events during initialization', async () => {
      const phaseEvents: string[] = [];
      
      bootstrapSystem.on('phase-complete', (phaseName: string) => {
        phaseEvents.push(phaseName);
      });
      
      await bootstrapSystem.start();
      
      expect(phaseEvents.length).toBeGreaterThan(0);
      expect(phaseEvents).toContain('command-discovery');
    }, 10000);
  });
});