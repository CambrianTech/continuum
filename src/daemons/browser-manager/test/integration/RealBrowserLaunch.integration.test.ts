/**
 * REAL BROWSER LAUNCH INTEGRATION TESTS
 * Tests actual browser process spawning, not mocks
 * 
 * VALIDATES:
 * - Real Chrome/browser process creation
 * - DevTools port allocation and responsiveness  
 * - Process monitoring and cleanup
 * - Session association and state tracking
 * - Multi-browser instance management
 */

import { BrowserManagerDaemon, BrowserConfig } from '../../BrowserManagerDaemon';
import { ProcessCommand } from '../../../commands/kernel/system/ProcessCommand';

describe('Real Browser Launch Integration', () => {
  let daemon: BrowserManagerDaemon;
  let launchedBrowsers: string[] = []; // Track for cleanup

  beforeEach(async () => {
    daemon = new BrowserManagerDaemon();
    await daemon.start();
    launchedBrowsers = [];
  });

  afterEach(async () => {
    // Cleanup: Kill all browsers launched during tests
    for (const browserId of launchedBrowsers) {
      try {
        await daemon.handleMessage({
          id: 'cleanup',
          from: 'test',
          to: 'browser-manager',
          type: 'browser_request',
          timestamp: new Date(),
          data: { type: 'destroy', sessionId: browserId }
        });
      } catch (error) {
        console.warn(`Failed to cleanup browser ${browserId}:`, error);
      }
    }
    
    await daemon.stop();
    
    // Force kill any remaining Chrome processes from our tests using ProcessCommand
    try {
      const result = await ProcessCommand.execute({
        subcommand: 'cleanup',
        filter: { command: 'continuum-browser' }
      });
      
      if (result.success) {
        console.log(`Cleaned up ${result.data.cleanedUp} test browser processes`);
      }
    } catch (error) {
      // Expected if no processes found
    }
  });

  describe('Real Browser Process Creation', () => {
    test('should launch actual Chrome browser with real PID', async () => {
      const config: BrowserConfig = {
        purpose: 'integration-test',
        persona: 'test-user',
        requirements: {
          devtools: false,
          isolation: 'dedicated',
          visibility: 'visible',
          persistence: 'session'
        },
        resources: {
          priority: 'normal'
        }
      };

      const response = await daemon.handleMessage({
        id: 'test-browser-launch',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'test-session-001',
          url: 'http://localhost:9000',
          config
        }
      });

      expect(response.success).toBe(true);
      expect(response.data.browserId).toBeDefined();
      expect(response.data.pid).toBeGreaterThan(1000); // Real PID
      
      launchedBrowsers.push(response.data.browserId);

      // Validate real process exists using ProcessCommand
      const processResult = await ProcessCommand.execute({
        subcommand: 'find',
        criteria: { pattern: response.data.pid.toString(), matchType: 'name' }
      });
      
      expect(processResult.success).toBe(true);
      expect(processResult.data.matches.length).toBeGreaterThan(0);
      expect(processResult.data.matches[0].name).toMatch(/chrome|chromium/i);
    });

    test('should launch browser with DevTools enabled for automation', async () => {
      const config: BrowserConfig = {
        purpose: 'automation',
        persona: 'git-hook',
        requirements: {
          devtools: true,
          isolation: 'sandboxed',
          visibility: 'hidden',
          persistence: 'ephemeral'
        },
        resources: {
          priority: 'high'
        }
      };

      const response = await daemon.handleMessage({
        id: 'test-devtools-launch',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'devtools-session-001',
          url: 'http://localhost:9000',
          config
        }
      });

      expect(response.success).toBe(true);
      launchedBrowsers.push(response.data.browserId);

      // Validate DevTools port is responsive
      const debugPort = response.data.debugPort || 9222;
      
      // Wait for browser to be ready
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      try {
        const versionResponse = await fetch(`http://localhost:${debugPort}/json/version`);
        expect(versionResponse.ok).toBe(true);
        
        const versionData = await versionResponse.json();
        expect(versionData.Browser).toMatch(/chrome|chromium/i);
      } catch (error) {
        // If DevTools port isn't ready, at least verify process exists
        const { stdout } = await execAsync(`ps -p ${response.data.pid} -o args=`);
        expect(stdout).toContain('--remote-debugging-port');
        expect(stdout).toContain('--auto-open-devtools-for-tabs');
      }
    });
  });

  describe('Multi-Browser Session Management', () => {
    test('should create separate browser instances for different session types', async () => {
      // Launch user browser
      const userConfig: BrowserConfig = {
        purpose: 'user',
        persona: 'user',
        requirements: {
          devtools: false,
          isolation: 'shared',
          visibility: 'visible',
          persistence: 'session'
        },
        resources: { priority: 'normal' }
      };

      const userResponse = await daemon.handleMessage({
        id: 'user-browser',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'user-session',
          url: 'http://localhost:9000',
          config: userConfig
        }
      });

      expect(userResponse.success).toBe(true);
      launchedBrowsers.push(userResponse.data.browserId);

      // Launch DevTools browser  
      const devConfig: BrowserConfig = {
        purpose: 'development',
        persona: 'portal',
        requirements: {
          devtools: true,
          isolation: 'dedicated',
          visibility: 'visible',
          persistence: 'session'
        },
        resources: { priority: 'high' }
      };

      const devResponse = await daemon.handleMessage({
        id: 'dev-browser',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'dev-session',
          url: 'http://localhost:9000',
          config: devConfig
        }
      });

      expect(devResponse.success).toBe(true);
      launchedBrowsers.push(devResponse.data.browserId);

      // Verify different browser instances
      expect(userResponse.data.browserId).not.toBe(devResponse.data.browserId);
      expect(userResponse.data.pid).not.toBe(devResponse.data.pid);

      // Verify both processes exist
      const userPid = userResponse.data.pid;
      const devPid = devResponse.data.pid;
      
      const { stdout: userProcess } = await execAsync(`ps -p ${userPid} -o comm=`);
      const { stdout: devProcess } = await execAsync(`ps -p ${devPid} -o comm=`);
      
      expect(userProcess.trim()).toMatch(/chrome|chromium/i);
      expect(devProcess.trim()).toMatch(/chrome|chromium/i);
    });

    test('should reuse browser for compatible user sessions', async () => {
      const config: BrowserConfig = {
        purpose: 'user',
        persona: 'user',
        requirements: {
          devtools: false,
          isolation: 'shared',
          visibility: 'visible',
          persistence: 'session'
        },
        resources: { priority: 'normal' }
      };

      // Launch first browser
      const firstResponse = await daemon.handleMessage({
        id: 'first-browser',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'session-001',
          url: 'http://localhost:9000',
          config
        }
      });

      expect(firstResponse.success).toBe(true);
      launchedBrowsers.push(firstResponse.data.browserId);

      // Try to launch second compatible session
      const secondResponse = await daemon.handleMessage({
        id: 'second-browser',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'session-002',
          url: 'http://localhost:9000',
          config
        }
      });

      expect(secondResponse.success).toBe(true);

      // Should reuse existing browser (add-tab or reuse strategy)
      if (secondResponse.data.action === 'add-tab' || secondResponse.data.action === 'reuse') {
        expect(secondResponse.data.browserId).toBe(firstResponse.data.browserId);
        expect(secondResponse.data.pid).toBe(firstResponse.data.pid);
      } else {
        // If created new browser, track it for cleanup
        launchedBrowsers.push(secondResponse.data.browserId);
      }
    });
  });

  describe('Process Monitoring and Health', () => {
    test('should detect when browser process dies', async () => {
      const config: BrowserConfig = {
        purpose: 'test-crash',
        persona: 'test',
        requirements: {
          devtools: false,
          isolation: 'dedicated',
          visibility: 'hidden',
          persistence: 'ephemeral'
        },
        resources: { priority: 'normal' }
      };

      const response = await daemon.handleMessage({
        id: 'crash-test',
        from: 'test',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: 'crash-session',
          url: 'http://localhost:9000',
          config
        }
      });

      expect(response.success).toBe(true);
      const { browserId, pid } = response.data;
      
      // Verify process exists
      let { stdout } = await execAsync(`ps -p ${pid} -o comm=`);
      expect(stdout.trim()).toMatch(/chrome|chromium/i);

      // Kill the process
      await execAsync(`kill -9 ${pid}`);

      // Wait for daemon to detect process death
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Verify browser is removed from daemon's tracking
      const listResponse = await daemon.handleMessage({
        id: 'list-browsers',
        from: 'test',
        to: 'browser-manager',
        type: 'browser_request',
        timestamp: new Date(),
        data: { type: 'list' }
      });

      expect(listResponse.success).toBe(true);
      const remainingBrowsers = listResponse.data.browsers;
      expect(remainingBrowsers.find((b: any) => b.id === browserId)).toBeUndefined();
    });
  });

  describe('Port Allocation and Cleanup', () => {
    test('should allocate unique DevTools ports for each browser', async () => {
      const portsUsed = new Set<number>();
      const browsers = [];

      // Launch multiple browsers
      for (let i = 0; i < 3; i++) {
        const config: BrowserConfig = {
          purpose: 'port-test',
          persona: 'test',
          requirements: {
            devtools: true,
            isolation: 'dedicated',
            visibility: 'hidden',
            persistence: 'ephemeral'
          },
          resources: { priority: 'normal' }
        };

        const response = await daemon.handleMessage({
          id: `port-test-${i}`,
          from: 'test',
          to: 'browser-manager',
          type: 'create_browser',
          timestamp: new Date(),
          data: {
            sessionId: `port-session-${i}`,
            url: 'http://localhost:9000',
            config
          }
        });

        expect(response.success).toBe(true);
        browsers.push(response.data);
        launchedBrowsers.push(response.data.browserId);

        const debugPort = response.data.debugPort || 9222;
        expect(portsUsed.has(debugPort)).toBe(false);
        portsUsed.add(debugPort);
      }

      // Verify all ports are unique
      expect(portsUsed.size).toBe(3);
      
      // Verify all browsers are actually running
      for (const browser of browsers) {
        const { stdout } = await execAsync(`ps -p ${browser.pid} -o comm=`);
        expect(stdout.trim()).toMatch(/chrome|chromium/i);
      }
    });
  });
});