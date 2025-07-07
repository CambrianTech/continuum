/**
 * UserSelector Widget - Integration Tests
 * Tests integration with UniversalUserSystem and continuum API
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

describe('UserSelector Widget Integration', () => {
  let UserSelectorWidget: any;
  let widget: any;
  let mockUniversalUserSystem: any;

  beforeEach(async () => {
    // Mock DOM environment
    const mockShadowRoot = {
      innerHTML: '',
      querySelector: jest.fn(),
      querySelectorAll: jest.fn(() => []),
      addEventListener: jest.fn()
    };

    (global as any).HTMLElement = class MockHTMLElement {
      shadowRoot = mockShadowRoot;
      attachShadow() { return mockShadowRoot; }
      addEventListener() {}
      classList = { add: jest.fn(), remove: jest.fn() };
    };

    (global as any).customElements = {
      define: jest.fn(),
      get: jest.fn(() => undefined)
    };

    // Mock window with continuum API
    (global as any).window = {
      continuum: {
        isConnected: jest.fn(() => true),
        execute: jest.fn(),
        on: jest.fn(),
        off: jest.fn(),
        send: jest.fn()
      }
    };

    // Mock UniversalUserSystem
    mockUniversalUserSystem = {
      getAllUsers: jest.fn(() => []),
      initializeAIModelNames: jest.fn(() => Promise.resolve()),
      on: jest.fn(),
      emit: jest.fn(),
      addUser: jest.fn(),
      removeUser: jest.fn(),
      updateUser: jest.fn()
    };

    // Mock the import
    jest.doMock('../../../shared/UniversalUserSystem', () => ({
      universalUserSystem: mockUniversalUserSystem
    }));

    const module = await import('../../../UserSelector/UserSelector');
    UserSelectorWidget = module.UserSelectorWidget;
    widget = new UserSelectorWidget();
  });

  describe('System Integration', () => {
    it('should initialize with UniversalUserSystem', async () => {
      await widget.initializeWidget();
      
      expect(mockUniversalUserSystem.initializeAIModelNames).toHaveBeenCalled();
    });

    it('should handle initialization failure gracefully', async () => {
      mockUniversalUserSystem.initializeAIModelNames.mockRejectedValue(new Error('Init failed'));
      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('AI model initialization failed'),
        expect.any(Error)
      );
      
      consoleSpy.mockRestore();
    });

    it('should set up event listeners on initialization', async () => {
      const setupSpy = jest.spyOn(widget, 'setupUniversalUserSystem').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(setupSpy).toHaveBeenCalled();
      setupSpy.mockRestore();
    });
  });

  describe('User System Event Integration', () => {
    it('should register for user system events', () => {
      widget.setupUniversalUserSystem();
      
      expect(mockUniversalUserSystem.on).toHaveBeenCalledWith('user:updated', expect.any(Function));
      expect(mockUniversalUserSystem.on).toHaveBeenCalledWith('user:added', expect.any(Function));
      expect(mockUniversalUserSystem.on).toHaveBeenCalledWith('user:removed', expect.any(Function));
    });

    it('should handle user updated events', () => {
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      const updateCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:updated')[1];
      
      updateCallback({ userId: 'user1', status: 'online' });
      
      expect(widget.render).toHaveBeenCalled();
    });

    it('should handle user added events', () => {
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      const addCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:added')[1];
      
      addCallback({ 
        id: 'newuser', 
        name: 'New User', 
        status: 'online',
        type: 'human' 
      });
      
      expect(widget.render).toHaveBeenCalled();
    });

    it('should handle user removed events', () => {
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      const removeCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:removed')[1];
      
      removeCallback({ userId: 'user1' });
      
      expect(widget.render).toHaveBeenCalled();
    });
  });

  describe('Action Integration', () => {
    it('should emit chat request events through user system', async () => {
      await widget.handleUserAction('chat', 'user1', 'Alice');
      
      expect(mockUniversalUserSystem.emit).toHaveBeenCalledWith('user:chat-requested', {
        userId: 'user1',
        userName: 'Alice'
      });
    });

    it('should emit profile request events through user system', async () => {
      await widget.handleUserAction('profile', 'user1', 'Alice');
      
      expect(mockUniversalUserSystem.emit).toHaveBeenCalledWith('user:profile-requested', {
        userId: 'user1',
        userName: 'Alice'
      });
    });

    it('should emit invite request events through user system', async () => {
      await widget.handleUserAction('invite', 'user1', 'Alice');
      
      expect(mockUniversalUserSystem.emit).toHaveBeenCalledWith('user:invite-requested', {
        userId: 'user1',
        userName: 'Alice'
      });
    });
  });

  describe('DOM Event Integration', () => {
    it('should set up click event listeners', () => {
      const mockButton = { addEventListener: jest.fn() };
      widget.shadowRoot.querySelectorAll = jest.fn().mockReturnValue([mockButton]);
      
      widget.setupEventListeners();
      
      expect(mockButton.addEventListener).toHaveBeenCalledWith('click', expect.any(Function));
    });

    it('should handle user action button clicks', () => {
      const handleActionSpy = jest.spyOn(widget, 'handleUserAction').mockImplementation();
      const mockButton = {
        addEventListener: jest.fn(),
        dataset: { action: 'chat', userId: 'user1', userName: 'Alice' }
      };
      
      widget.shadowRoot.querySelectorAll = jest.fn().mockReturnValue([mockButton]);
      widget.setupEventListeners();
      
      // Get the click callback and trigger it
      const clickCallback = mockButton.addEventListener.mock.calls[0][1];
      const mockEvent = { currentTarget: mockButton, preventDefault: jest.fn() };
      
      clickCallback(mockEvent);
      
      expect(handleActionSpy).toHaveBeenCalledWith('chat', 'user1', 'Alice');
      handleActionSpy.mockRestore();
    });

    it('should prevent default on button clicks', () => {
      const mockButton = {
        addEventListener: jest.fn(),
        dataset: { action: 'chat', userId: 'user1', userName: 'Alice' }
      };
      
      widget.shadowRoot.querySelectorAll = jest.fn().mockReturnValue([mockButton]);
      widget.setupEventListeners();
      
      const clickCallback = mockButton.addEventListener.mock.calls[0][1];
      const mockEvent = { currentTarget: mockButton, preventDefault: jest.fn() };
      
      clickCallback(mockEvent);
      
      expect(mockEvent.preventDefault).toHaveBeenCalled();
    });
  });

  describe('Real-time Updates Integration', () => {
    it('should refresh display when users change', () => {
      const users = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(users);
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      // Simulate user status change
      users[0].status = 'away';
      const updateCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:updated')[1];
      
      updateCallback({ userId: 'user1', status: 'away' });
      
      expect(widget.render).toHaveBeenCalled();
    });

    it('should handle multiple simultaneous user updates', () => {
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      const updateCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:updated')[1];
      
      // Rapid updates should still trigger renders
      updateCallback({ userId: 'user1', status: 'online' });
      updateCallback({ userId: 'user2', status: 'away' });
      updateCallback({ userId: 'user3', status: 'offline' });
      
      expect(widget.render).toHaveBeenCalledTimes(3);
    });
  });

  describe('AI Model Integration', () => {
    it('should initialize AI model names on startup', async () => {
      await widget.initializeWidget();
      
      expect(mockUniversalUserSystem.initializeAIModelNames).toHaveBeenCalled();
    });

    it('should handle AI model initialization in background', async () => {
      // Should not block widget loading if AI model init takes time
      let resolveInit: (value?: any) => void;
      const initPromise = new Promise(resolve => { resolveInit = resolve; });
      
      mockUniversalUserSystem.initializeAIModelNames.mockReturnValue(initPromise);
      
      const initPromiseResult = widget.initializeWidget();
      
      // Widget should initialize immediately
      await expect(initPromiseResult).resolves.toBeUndefined();
      
      // AI model init can complete later
      resolveInit!();
      await initPromise;
    });
  });

  describe('Cross-Widget Communication Integration', () => {
    it('should communicate user selection to other widgets', async () => {
      const mockUser = {
        id: 'user1',
        name: 'Alice',
        status: 'online',
        avatar: '👩',
        type: 'human'
      };
      
      await widget.handleUserAction('chat', mockUser.id, mockUser.name);
      
      expect(mockUniversalUserSystem.emit).toHaveBeenCalledWith('user:chat-requested', {
        userId: mockUser.id,
        userName: mockUser.name
      });
    });

    it('should handle external user selection events', () => {
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      // Simulate external user selection
      const updateCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:updated')[1];
      
      updateCallback({ userId: 'user1', selectedBy: 'another-widget' });
      
      expect(widget.render).toHaveBeenCalled();
    });
  });

  describe('Data Consistency Integration', () => {
    it('should maintain consistent user data across updates', () => {
      const initialUsers = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(initialUsers);
      
      // First render
      const content1 = widget.renderContent();
      expect(content1).toContain('Alice');
      expect(content1).toContain('online');
      
      // Update user data
      initialUsers[0].status = 'away';
      
      // Second render should reflect changes
      const content2 = widget.renderContent();
      expect(content2).toContain('Alice');
      expect(content2).toContain('away');
    });

    it('should handle user data race conditions', () => {
      const users = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(users);
      
      // Rapid successive renders should not break
      expect(() => {
        widget.renderContent();
        users[0].status = 'away';
        widget.renderContent();
        users[0].status = 'offline';
        widget.renderContent();
      }).not.toThrow();
    });
  });
});