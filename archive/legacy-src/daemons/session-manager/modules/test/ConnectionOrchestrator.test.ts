/**
 * Unit Test: Connection Orchestrator
 * 
 * Tests connection routing logic independently of session storage
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ConnectionOrchestrator, SessionProvider } from '../ConnectionOrchestrator.js';
import { BrowserSession, SessionType } from '../../SessionManagerDaemon.js';

describe('ConnectionOrchestrator', () => {
  let orchestrator: ConnectionOrchestrator;
  let mockSessionProvider: SessionProvider;
  let mockSessions: Map<string, BrowserSession>;

  beforeEach(() => {
    mockSessions = new Map();
    
    // Create mock session provider
    mockSessionProvider = {
      getSession: (sessionId: string) => mockSessions.get(sessionId) || null,
      
      getLatestSession: (criteria) => {
        const sessions = Array.from(mockSessions.values())
          .filter(session => {
            if (criteria.owner && session.owner !== criteria.owner) return false;
            if (criteria.type && session.type !== criteria.type) return false;
            if (criteria.active !== undefined && session.isActive !== criteria.active) return false;
            return true;
          })
          .sort((a, b) => b.lastActive.getTime() - a.lastActive.getTime());
        
        return sessions.length > 0 ? sessions[0] : null;
      },
      
      createSession: async (options) => {
        const session: BrowserSession = {
          id: `session-${Date.now()}-${Math.random().toString(36).substr(2, 5)}`,
          type: options.type || 'development',
          owner: options.owner,
          created: new Date(),
          lastActive: new Date(),
          processes: {},
          artifacts: {
            storageDir: `/tmp/test-session-${Date.now()}`,
            logs: { 
              server: ['/tmp/server.log'], 
              client: ['/tmp/browser.log']
            },
            screenshots: [],
            files: [],
            recordings: [],
            devtools: []
          },
          isActive: true,
          shouldAutoCleanup: true,
          cleanupAfterMs: 2 * 60 * 60 * 1000
        };
        
        mockSessions.set(session.id, session);
        return session;
      },
      
      forkSession: async (fromSessionId: string, options) => {
        const sourceSession = mockSessions.get(fromSessionId);
        if (!sourceSession) {
          throw new Error(`Source session ${fromSessionId} not found`);
        }
        
        return mockSessionProvider.createSession({
          ...options,
          context: `forked-from-${fromSessionId}`
        });
      }
    };
    
    orchestrator = new ConnectionOrchestrator(mockSessionProvider);
  });

  describe('Session Preference: Current (Default)', () => {
    it('should join existing session when available', async () => {
      // Create existing session
      const existingSession = await mockSessionProvider.createSession({
        type: 'development',
        owner: 'test-user'
      });

      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: 'current',
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe(existingSession.id);
      expect(result.data?.action).toBe('joined_existing');
      expect(result.data?.launched.newLogFiles).toBe(false);
    });

    it('should create new session when none exists', async () => {
      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: 'current',
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBeDefined();
      expect(result.data?.action).toBe('created_new');
      expect(result.data?.launched.newLogFiles).toBe(true);
    });
  });

  describe('Session Preference: New', () => {
    it('should always create new session', async () => {
      // Create existing session
      await mockSessionProvider.createSession({
        type: 'development',
        owner: 'test-user'
      });

      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: 'new',
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data?.action).toBe('created_new');
      expect(result.data?.launched.newLogFiles).toBe(true);
    });
  });

  describe('Session Preference: Fork', () => {
    it('should fork from existing session', async () => {
      // Create source session
      const sourceSession = await mockSessionProvider.createSession({
        type: 'development',
        owner: 'source-user'
      });

      const request = {
        source: 'cli',
        owner: 'fork-user',
        sessionPreference: `fork:${sourceSession.id}`,
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data?.action).toBe('forked_from');
      expect(result.data?.launched.newLogFiles).toBe(true);
      expect(result.data?.sessionId).not.toBe(sourceSession.id); // New session ID
    });

    it('should fail to fork from non-existent session', async () => {
      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: 'fork:non-existent-session',
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
    });
  });

  describe('Session Preference: Specific ID', () => {
    it('should join specific session by ID', async () => {
      const targetSession = await mockSessionProvider.createSession({
        type: 'development',
        owner: 'test-user'
      });

      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: targetSession.id,
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe(targetSession.id);
      expect(result.data?.action).toBe('joined_existing');
    });

    it('should fail for non-existent session ID', async () => {
      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: 'non-existent-session-id-123',
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
    });
  });

  describe('Result Structure', () => {
    it('should return complete connection result', async () => {
      const request = {
        source: 'cli',
        owner: 'test-user',
        capabilities: ['browser'],
        context: 'test-context',
        type: 'development' as SessionType
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data).toMatchObject({
        sessionId: expect.any(String),
        action: expect.stringMatching(/^(joined_existing|created_new|forked_from)$/),
        launched: {
          browser: true, // Should match capabilities
          webserver: true,
          newLogFiles: expect.any(Boolean)
        },
        logs: {
          browser: expect.any(String),
          server: expect.any(String)
        },
        interface: 'http://localhost:9000',
        screenshots: expect.any(String),
        commands: {
          otherClients: expect.stringContaining('session-clients'),
          stop: expect.stringContaining('session-stop'),
          fork: expect.stringContaining('session-fork'),
          info: expect.stringContaining('session-info')
        }
      });
    });

    it('should handle missing capabilities', async () => {
      const request = {
        source: 'cli',
        owner: 'test-user'
        // No capabilities specified
      };

      const result = await orchestrator.orchestrate(request);
      
      expect(result.success).toBe(true);
      expect(result.data?.launched.browser).toBe(false); // No browser capability
      expect(result.data?.launched.webserver).toBe(true); // Always true
    });
  });

  describe('Error Handling', () => {
    it('should handle session creation failures', async () => {
      // Mock session provider that fails
      const failingProvider: SessionProvider = {
        ...mockSessionProvider,
        createSession: async () => {
          throw new Error('Session creation failed');
        }
      };

      const failingOrchestrator = new ConnectionOrchestrator(failingProvider);
      
      const request = {
        source: 'cli',
        owner: 'test-user',
        sessionPreference: 'new'
      };

      const result = await failingOrchestrator.orchestrate(request);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Connection orchestration failed');
    });
  });
});