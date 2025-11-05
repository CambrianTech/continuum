/**
 * SessionCommand Unit Tests
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SessionCommand } from '../../SessionCommand';

describe('SessionCommand', () => {
  let mockSessionManager: any;
  let mockContext: any;

  beforeEach(() => {
    // Create mock session manager daemon with handleMessage interface
    mockSessionManager = {
      handleMessage: vi.fn()
    };

    // Create mock context
    mockContext = {
      connectionId: 'test-connection-123',
      websocket: {
        registeredDaemons: new Map([
          ['session-manager', mockSessionManager]
        ])
      }
    };
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('getDefinition', () => {
    it('should return proper command definition', () => {
      const definition = SessionCommand.getDefinition();
      
      expect(definition.name).toBe('session');
      expect(definition.description).toBe('Query and manage Continuum sessions');
      expect(definition.category).toBe('system');
      expect(definition.parameters).toEqual({});
    });
  });

  describe('execute - list action', () => {
    it('should list all sessions', async () => {
      const mockSessions = [
        {
          id: 'cli-joel-dev-250701-1234',
          type: 'development',
          owner: 'joel',
          created: new Date(),
          lastActive: new Date(),
          isActive: true,
          artifacts: { storageDir: '/path/to/session' }
        }
      ];

      mockSessionManager.handleMessage.mockResolvedValue({
        success: true,
        data: {
          sessions: mockSessions,
          total: 1
        }
      });

      const result = await SessionCommand.execute(
        { action: 'list' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.sessions).toHaveLength(1);
      expect(result.data.count).toBe(1);
      expect(mockSessionManager.handleMessage).toHaveBeenCalledWith({
        id: expect.stringMatching(/list-\d+/),
        from: 'session-command',
        to: 'session-manager',
        type: 'list_sessions',
        timestamp: expect.any(Date),
        data: { filter: undefined }
      });
    });

    it('should list sessions with filter', async () => {
      const filter = { starter: 'cli', active: true };
      mockSessionManager.handleMessage.mockResolvedValue({
        success: true,
        data: {
          sessions: [],
          total: 0
        }
      });

      const result = await SessionCommand.execute(
        { action: 'list', filter },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(mockSessionManager.handleMessage).toHaveBeenCalledWith({
        id: expect.stringMatching(/list-\d+/),
        from: 'session-command',
        to: 'session-manager',
        type: 'list_sessions',
        timestamp: expect.any(Date),
        data: { filter }
      });
    });
  });

  describe('execute - current action', () => {
    it('should return current session', async () => {
      const mockIdentity = {
        starter: 'cli',
        identity: { name: 'joel', user: 'joel' }
      };
      const mockSession = {
        id: 'cli-joel-dev-250701-1234',
        identity: { starter: 'cli', name: 'joel' },
        type: 'development',
        owner: 'joel',
        created: new Date(),
        lastActive: new Date(),
        isActive: true,
        artifacts: { storageDir: '/path/to/session' }
      };

      mockSessionManager.getConnectionIdentity.mockReturnValue(mockIdentity);
      mockSessionManager.getLatestSession.mockReturnValue(mockSession);

      const result = await SessionCommand.execute(
        { action: 'current' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.session.id).toBe('cli-joel-dev-250701-1234');
      expect(mockSessionManager.getConnectionIdentity).toHaveBeenCalledWith('test-connection-123');
    });

    it('should fail when no connection context', async () => {
      const contextWithoutConnection = { websocket: mockContext.websocket };

      const result = await SessionCommand.execute(
        { action: 'current' },
        contextWithoutConnection
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('No connection context available');
    });

    it('should fail when connection not identified', async () => {
      mockSessionManager.getConnectionIdentity.mockReturnValue(null);

      const result = await SessionCommand.execute(
        { action: 'current' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Connection not identified');
    });

    it('should fail when no current session found', async () => {
      const mockIdentity = {
        starter: 'cli',
        identity: { name: 'joel', user: 'joel' }
      };

      mockSessionManager.getConnectionIdentity.mockReturnValue(mockIdentity);
      mockSessionManager.getLatestSession.mockReturnValue(null);

      const result = await SessionCommand.execute(
        { action: 'current' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('No current session found');
    });
  });

  describe('execute - latest action', () => {
    it('should return latest session', async () => {
      const mockSession = {
        id: 'cli-joel-dev-250701-1234',
        identity: { starter: 'cli', name: 'joel' },
        type: 'development',
        owner: 'joel',
        created: new Date(),
        lastActive: new Date(),
        isActive: true,
        artifacts: { storageDir: '/path/to/session' }
      };

      mockSessionManager.getLatestSession.mockReturnValue(mockSession);

      const result = await SessionCommand.execute(
        { action: 'latest' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.session.id).toBe('cli-joel-dev-250701-1234');
    });

    it('should fail when no sessions found', async () => {
      mockSessionManager.getLatestSession.mockReturnValue(null);

      const result = await SessionCommand.execute(
        { action: 'latest' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('No sessions found matching filter criteria');
    });
  });

  describe('execute - join action', () => {
    it('should join session successfully', async () => {
      mockSessionManager.joinSession.mockResolvedValue('target-session-id');

      const result = await SessionCommand.execute(
        { action: 'join', sessionId: 'target-session-id' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.sessionId).toBe('target-session-id');
      expect(result.data.action).toBe('joined');
      expect(mockSessionManager.joinSession).toHaveBeenCalledWith('test-connection-123', 'target-session-id');
    });

    it('should fail when sessionId not provided', async () => {
      const result = await SessionCommand.execute(
        { action: 'join' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('sessionId required for join action');
    });

    it('should fail when join throws error', async () => {
      mockSessionManager.joinSession.mockRejectedValue(new Error('Session not found'));

      const result = await SessionCommand.execute(
        { action: 'join', sessionId: 'nonexistent-session' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Failed to join session: Session not found');
    });
  });

  describe('execute - create action', () => {
    it('should create session successfully', async () => {
      const identity = {
        starter: 'cli',
        name: 'joel',
        type: 'development'
      };

      mockSessionManager.createSession.mockResolvedValue('new-session-id');

      const result = await SessionCommand.execute(
        { action: 'create', identity },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.sessionId).toBe('new-session-id');
      expect(result.data.action).toBe('created');
      expect(mockSessionManager.createSession).toHaveBeenCalledWith(identity);
    });

    it('should fail when identity not provided', async () => {
      const result = await SessionCommand.execute(
        { action: 'create' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('identity required for create action');
    });
  });

  describe('execute - close action', () => {
    it('should close session successfully', async () => {
      mockSessionManager.closeSession.mockResolvedValue(undefined);

      const result = await SessionCommand.execute(
        { action: 'close', sessionId: 'session-to-close' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.sessionId).toBe('session-to-close');
      expect(result.data.action).toBe('closed');
      expect(mockSessionManager.closeSession).toHaveBeenCalledWith('session-to-close');
    });

    it('should fail when sessionId not provided', async () => {
      const result = await SessionCommand.execute(
        { action: 'close' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('sessionId required for close action');
    });
  });

  describe('execute - info action', () => {
    it('should return session info', async () => {
      const mockSession = {
        id: 'session-id',
        identity: { starter: 'cli', name: 'joel' },
        type: 'development',
        owner: 'joel',
        created: new Date(),
        lastActive: new Date(),
        isActive: true,
        artifacts: {
          storageDir: '/path/to/session',
          logs: { server: ['log1.log'], client: ['log2.log'] },
          screenshots: ['screenshot1.png'],
          files: ['file1.txt'],
          recordings: []
        },
        mainBrowser: { pid: 12345, url: 'http://localhost:9000', isConnected: true },
        devTools: { pid: null, isOpen: false, tabs: [] }
      };

      mockSessionManager.getSession.mockReturnValue(mockSession);

      const result = await SessionCommand.execute(
        { action: 'info', sessionId: 'session-id' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data.session.id).toBe('session-id');
      expect(result.data.session.artifacts.logs.server).toBe(1);
      expect(result.data.session.artifacts.logs.client).toBe(1);
      expect(result.data.session.artifacts.screenshots).toBe(1);
    });

    it('should fail when session not found', async () => {
      mockSessionManager.getSession.mockReturnValue(null);

      const result = await SessionCommand.execute(
        { action: 'info', sessionId: 'nonexistent-session' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Session nonexistent-session not found');
    });
  });

  describe('execute - error handling', () => {
    it('should handle session manager not available', async () => {
      const contextWithoutSessionManager = { connectionId: 'test-connection-123' };

      const result = await SessionCommand.execute(
        { action: 'list' },
        contextWithoutSessionManager
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Session manager not available');
    });

    it('should handle unknown action', async () => {
      const result = await SessionCommand.execute(
        { action: 'unknown-action' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Unknown action: unknown-action');
    });

    it('should handle unexpected errors', async () => {
      mockSessionManager.queryAvailableSessions.mockImplementation(() => {
        throw new Error('Unexpected error');
      });

      const result = await SessionCommand.execute(
        { action: 'list' },
        mockContext
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Session command failed: Unexpected error');
    });
  });
});