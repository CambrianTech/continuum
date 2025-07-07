/**
 * UserSelector Widget - Unit Tests
 * Tests user management and selection functionality
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

// Mock DOM environment
const mockShadowRoot = {
  innerHTML: '',
  querySelector: jest.fn(),
  querySelectorAll: jest.fn(() => []),
  addEventListener: jest.fn()
};

const mockUniversalUserSystem = {
  getAllUsers: jest.fn(() => []),
  initializeAIModelNames: jest.fn(() => Promise.resolve()),
  on: jest.fn(),
  emit: jest.fn()
};

// Set up global mocks
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

(global as any).window = {
  continuum: {
    isConnected: () => true,
    execute: jest.fn(),
    on: jest.fn(),
    off: jest.fn()
  }
};

describe('UserSelector Widget', () => {
  let UserSelectorWidget: any;
  let universalUserSystem: any;

  beforeEach(async () => {
    jest.clearAllMocks();
    mockShadowRoot.innerHTML = '';
    
    // Mock the universalUserSystem import
    jest.doMock('../../../shared/UniversalUserSystem', () => ({
      universalUserSystem: mockUniversalUserSystem
    }));
    
    const module = await import('../../../UserSelector/UserSelector');
    UserSelectorWidget = module.UserSelectorWidget;
  });

  describe('Constructor', () => {
    it('should initialize with correct properties', () => {
      const widget = new UserSelectorWidget();
      
      expect(widget.widgetName).toBe('User Selector');
      expect(widget.widgetIcon).toBe('👥');
      expect(widget.widgetTitle).toBe('Connected Users');
    });

    it('should have static CSS declaration', () => {
      expect(UserSelectorWidget.getOwnCSS()).toEqual(['UserSelector.css']);
    });
  });

  describe('User Management', () => {
    it('should get all users from universal user system', () => {
      const mockUsers = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' },
        { id: 'user2', name: 'Bob', status: 'offline', avatar: '👨', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const users = widget.getAllUsers();
      
      expect(users).toEqual(mockUsers);
      expect(mockUniversalUserSystem.getAllUsers).toHaveBeenCalled();
    });

    it('should filter online users', () => {
      const mockUsers = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' },
        { id: 'user2', name: 'Bob', status: 'offline', avatar: '👨', type: 'human' },
        { id: 'user3', name: 'Charlie', status: 'online', avatar: '🧑', type: 'ai' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const onlineUsers = widget.getOnlineUsers();
      
      expect(onlineUsers).toHaveLength(2);
      expect(onlineUsers.every(user => user.status === 'online')).toBe(true);
    });

    it('should filter users by type', () => {
      const mockUsers = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' },
        { id: 'user2', name: 'GPT-4', status: 'online', avatar: '🤖', type: 'ai' },
        { id: 'user3', name: 'Charlie', status: 'online', avatar: '🧑', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const humanUsers = widget.getUsersByType('human');
      const aiUsers = widget.getUsersByType('ai');
      
      expect(humanUsers).toHaveLength(2);
      expect(aiUsers).toHaveLength(1);
      expect(humanUsers.every(user => user.type === 'human')).toBe(true);
      expect(aiUsers.every(user => user.type === 'ai')).toBe(true);
    });
  });

  describe('Rendering', () => {
    it('should render empty state when no users', () => {
      mockUniversalUserSystem.getAllUsers.mockReturnValue([]);
      
      const widget = new UserSelectorWidget();
      const content = widget.renderContent();
      
      expect(content).toContain('empty-state');
      expect(content).toContain('No users connected');
    });

    it('should render user list when users available', () => {
      const mockUsers = [
        { id: 'user1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' },
        { id: 'user2', name: 'Bob', status: 'offline', avatar: '👨', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const content = widget.renderContent();
      
      expect(content).toContain('user-list');
      expect(content).toContain('Alice');
      expect(content).toContain('Bob');
    });

    it('should render individual user correctly', () => {
      const mockUser = {
        id: 'user1',
        name: 'Alice',
        status: 'online',
        avatar: '👩',
        type: 'human'
      };
      
      const widget = new UserSelectorWidget();
      const userHtml = widget.renderUser(mockUser);
      
      expect(userHtml).toContain('Alice');
      expect(userHtml).toContain('👩');
      expect(userHtml).toContain('online');
      expect(userHtml).toContain('user1');
    });

    it('should include action buttons for users', () => {
      const mockUser = {
        id: 'user1',
        name: 'Alice',
        status: 'online',
        avatar: '👩',
        type: 'human'
      };
      
      const widget = new UserSelectorWidget();
      const userHtml = widget.renderUser(mockUser);
      
      expect(userHtml).toContain('Chat');
      expect(userHtml).toContain('Profile');
      expect(userHtml).toContain('data-action');
    });
  });

  describe('User Categories', () => {
    it('should categorize users correctly', () => {
      const mockUsers = [
        { id: 'human1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' },
        { id: 'ai1', name: 'GPT-4', status: 'online', avatar: '🤖', type: 'ai' },
        { id: 'persona1', name: 'DevBot', status: 'online', avatar: '👨‍💻', type: 'persona' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const categories = widget.categorizeUsers();
      
      expect(categories.humans).toHaveLength(1);
      expect(categories.aiModels).toHaveLength(1);
      expect(categories.personas).toHaveLength(1);
    });

    it('should render categories with headers', () => {
      const mockUsers = [
        { id: 'human1', name: 'Alice', status: 'online', avatar: '👩', type: 'human' },
        { id: 'ai1', name: 'GPT-4', status: 'online', avatar: '🤖', type: 'ai' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const content = widget.renderContent();
      
      expect(content).toContain('Human Users');
      expect(content).toContain('AI Models');
    });
  });

  describe('User Actions', () => {
    it('should handle chat action', async () => {
      const widget = new UserSelectorWidget();
      const emitSpy = jest.spyOn(mockUniversalUserSystem, 'emit');
      
      await widget.handleUserAction('chat', 'user1', 'Alice');
      
      expect(emitSpy).toHaveBeenCalledWith('user:chat-requested', {
        userId: 'user1',
        userName: 'Alice'
      });
    });

    it('should handle profile action', async () => {
      const widget = new UserSelectorWidget();
      const emitSpy = jest.spyOn(mockUniversalUserSystem, 'emit');
      
      await widget.handleUserAction('profile', 'user1', 'Alice');
      
      expect(emitSpy).toHaveBeenCalledWith('user:profile-requested', {
        userId: 'user1',
        userName: 'Alice'
      });
    });

    it('should handle unknown action gracefully', async () => {
      const widget = new UserSelectorWidget();
      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation();
      
      await widget.handleUserAction('unknown', 'user1', 'Alice');
      
      expect(consoleSpy).toHaveBeenCalledWith('Unknown user action: unknown');
      consoleSpy.mockRestore();
    });
  });

  describe('Event Handling', () => {
    it('should set up user system event listeners', () => {
      const widget = new UserSelectorWidget();
      widget.setupUniversalUserSystem();
      
      expect(mockUniversalUserSystem.on).toHaveBeenCalledWith('user:updated', expect.any(Function));
    });

    it('should refresh on user updates', () => {
      const widget = new UserSelectorWidget();
      widget.render = jest.fn();
      widget.setupUniversalUserSystem();
      
      // Get the callback and trigger it
      const updateCallback = mockUniversalUserSystem.on.mock.calls
        .find((call: any) => call[0] === 'user:updated')[1];
      
      updateCallback();
      
      expect(widget.render).toHaveBeenCalled();
    });
  });

  describe('Search and Filtering', () => {
    it('should filter users by search term', () => {
      const mockUsers = [
        { id: 'user1', name: 'Alice Johnson', status: 'online', avatar: '👩', type: 'human' },
        { id: 'user2', name: 'Bob Smith', status: 'online', avatar: '👨', type: 'human' },
        { id: 'user3', name: 'Charlie Alice', status: 'online', avatar: '🧑', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const filteredUsers = widget.filterUsers('Alice');
      
      expect(filteredUsers).toHaveLength(2);
      expect(filteredUsers.every(user => user.name.includes('Alice'))).toBe(true);
    });

    it('should handle case-insensitive search', () => {
      const mockUsers = [
        { id: 'user1', name: 'Alice Johnson', status: 'online', avatar: '👩', type: 'human' }
      ];
      
      mockUniversalUserSystem.getAllUsers.mockReturnValue(mockUsers);
      
      const widget = new UserSelectorWidget();
      const filteredUsers = widget.filterUsers('alice');
      
      expect(filteredUsers).toHaveLength(1);
      expect(filteredUsers[0].name).toBe('Alice Johnson');
    });
  });

  describe('Status Management', () => {
    it('should get status indicator for different statuses', () => {
      const widget = new UserSelectorWidget();
      
      expect(widget.getStatusIndicator('online')).toBe('🟢');
      expect(widget.getStatusIndicator('away')).toBe('🟡');
      expect(widget.getStatusIndicator('busy')).toBe('🔴');
      expect(widget.getStatusIndicator('offline')).toBe('⚫');
      expect(widget.getStatusIndicator('unknown')).toBe('⚪');
    });

    it('should render status correctly in user display', () => {
      const mockUser = {
        id: 'user1',
        name: 'Alice',
        status: 'online',
        avatar: '👩',
        type: 'human'
      };
      
      const widget = new UserSelectorWidget();
      const userHtml = widget.renderUser(mockUser);
      
      expect(userHtml).toContain('🟢');
      expect(userHtml).toContain('status-online');
    });
  });
});