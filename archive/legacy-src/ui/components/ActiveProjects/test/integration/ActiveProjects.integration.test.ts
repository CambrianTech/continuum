/**
 * ActiveProjects Widget - Integration Tests
 * Tests widget integration with continuum API and DOM
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

describe('ActiveProjects Widget Integration', () => {
  let ActiveProjectsWidget: any;
  let widget: any;

  beforeEach(async () => {
    // Set up DOM mocks
    const mockElement = {
      shadowRoot: {
        innerHTML: '',
        querySelector: jest.fn(),
        querySelectorAll: jest.fn(() => []),
        addEventListener: jest.fn()
      },
      attachShadow: jest.fn().mockReturnValue({
        innerHTML: '',
        querySelector: jest.fn(),
        querySelectorAll: jest.fn(() => []),
        addEventListener: jest.fn()
      }),
      addEventListener: jest.fn(),
      classList: { add: jest.fn(), remove: jest.fn() }
    };

    (global as any).HTMLElement = class MockHTMLElement {
      shadowRoot = mockElement.shadowRoot;
      attachShadow() { return mockElement.shadowRoot; }
      addEventListener() {}
      classList = mockElement.classList;
    };

    (global as any).customElements = {
      define: jest.fn(),
      get: jest.fn(() => undefined)
    };

    // Mock window.continuum API
    (global as any).window = {
      continuum: {
        isConnected: jest.fn(() => true),
        execute: jest.fn(),
        on: jest.fn(),
        off: jest.fn(),
        send: jest.fn()
      }
    };

    const module = await import('../../../ActiveProjects/ActiveProjects');
    ActiveProjectsWidget = module.ActiveProjectsWidget;
    widget = new ActiveProjectsWidget();
  });

  describe('API Integration', () => {
    it('should load projects from continuum API', async () => {
      const mockProjects = [
        {
          id: 'test-project',
          name: 'Test Project',
          status: 'active',
          progress: 50,
          lastActivity: '1 hour ago',
          assignedAgents: ['Agent1'],
          priority: 'high'
        }
      ];

      (global as any).window.continuum.execute.mockResolvedValue({
        projects: mockProjects
      });

      await widget.loadProjects();

      expect(widget.projects).toEqual(mockProjects);
      expect((global as any).window.continuum.execute).toHaveBeenCalledWith('projects', { action: 'list_active' });
    });

    it('should fall back to mock data when API fails', async () => {
      (global as any).window.continuum.execute.mockRejectedValue(new Error('API Error'));
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();

      await widget.loadProjects();

      expect(widget.projects.length).toBeGreaterThan(0);
      expect(widget.projects[0]).toHaveProperty('id');
      expect(consoleSpy).toHaveBeenCalled();
      
      consoleSpy.mockRestore();
    });

    it('should fall back to mock data when not connected', async () => {
      (global as any).window.continuum.isConnected.mockReturnValue(false);

      await widget.loadProjects();

      expect(widget.projects.length).toBeGreaterThan(0);
      expect(widget.projects[0].name).toBe('Continuum OS');
    });
  });

  describe('Event Handling Integration', () => {
    it('should set up continuum event listeners', () => {
      widget.setupContinuumListeners();

      expect((global as any).window.continuum.on).toHaveBeenCalledWith('projects_updated', expect.any(Function));
      expect((global as any).window.continuum.on).toHaveBeenCalledWith('project_progress_changed', expect.any(Function));
    });

    it('should handle projects_updated event', () => {
      widget.loadProjects = jest.fn();
      widget.setupContinuumListeners();

      // Get the callback function that was registered
      const projectsUpdatedCallback = (global as any).window.continuum.on.mock.calls
        .find((call: any) => call[0] === 'projects_updated')[1];

      projectsUpdatedCallback();

      expect(widget.loadProjects).toHaveBeenCalled();
    });

    it('should handle project_progress_changed event', () => {
      widget.updateProjectProgress = jest.fn();
      widget.setupContinuumListeners();

      // Get the callback function that was registered
      const progressChangedCallback = (global as any).window.continuum.on.mock.calls
        .find((call: any) => call[0] === 'project_progress_changed')[1];

      const testData = { projectId: 'test-id', progress: 75 };
      progressChangedCallback(testData);

      expect(widget.updateProjectProgress).toHaveBeenCalledWith('test-id', 75);
    });
  });

  describe('DOM Integration', () => {
    it('should initialize widget properly', async () => {
      const initSpy = jest.spyOn(widget, 'loadProjects').mockResolvedValue(undefined);
      const listenerSpy = jest.spyOn(widget, 'setupContinuumListeners').mockImplementation();

      await widget.initializeWidget();

      expect(initSpy).toHaveBeenCalled();
      expect(listenerSpy).toHaveBeenCalled();
      
      initSpy.mockRestore();
      listenerSpy.mockRestore();
    });

    it('should send project selection events', () => {
      widget.loadMockData();
      const projectId = widget.projects[0].id;

      widget.selectProject(projectId);

      expect((global as any).window.continuum.send).toHaveBeenCalledWith({
        type: 'project_selected',
        project: widget.projects[0]
      });
    });
  });

  describe('Command Execution Integration', () => {
    it('should execute create project command', async () => {
      (global as any).window.continuum.execute.mockResolvedValue({ success: true });

      await widget.handleAction('create');

      expect((global as any).window.continuum.execute).toHaveBeenCalledWith('projects', { action: 'create' });
    });

    it('should handle command execution errors', async () => {
      (global as any).window.continuum.execute.mockRejectedValue(new Error('Command failed'));
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();

      await widget.handleAction('create');

      expect(consoleSpy).toHaveBeenCalledWith('🎛️ ActiveProjects: Failed to create project:', expect.any(Error));
      consoleSpy.mockRestore();
    });
  });

  describe('Lifecycle Integration', () => {
    it('should handle connection retry logic', () => {
      (global as any).window.continuum = null;
      const timeoutSpy = jest.spyOn(global, 'setTimeout').mockImplementation((fn: any) => fn());

      widget.setupContinuumListeners();

      expect(timeoutSpy).toHaveBeenCalledWith(expect.any(Function), 1000);
      timeoutSpy.mockRestore();
    });

    it('should retry connection setup when API not available', () => {
      (global as any).window.continuum = null;
      const setupSpy = jest.spyOn(widget, 'setupContinuumListeners');
      
      widget.setupContinuumListeners();

      expect(setupSpy).toHaveBeenCalled();
      setupSpy.mockRestore();
    });
  });
});