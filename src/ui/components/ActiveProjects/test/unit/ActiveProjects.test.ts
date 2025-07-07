/**
 * ActiveProjects Widget - Unit Tests
 * Tests core functionality and behavior
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

// Mock DOM environment for testing
const mockShadowRoot = {
  innerHTML: '',
  querySelector: jest.fn(),
  querySelectorAll: jest.fn(() => []),
  addEventListener: jest.fn()
};

const mockWindow = {
  continuum: {
    isConnected: () => true,
    execute: jest.fn(),
    on: jest.fn(),
    off: jest.fn()
  }
};

// Set up global mocks
(global as any).window = mockWindow;
(global as any).HTMLElement = class MockHTMLElement {
  shadowRoot = mockShadowRoot;
  attachShadow() { return mockShadowRoot; }
  addEventListener() {}
  removeEventListener() {}
  setAttribute() {}
  getAttribute() { return null; }
  classList = { add: jest.fn(), remove: jest.fn() };
};
(global as any).customElements = {
  define: jest.fn(),
  get: jest.fn(() => undefined)
};

describe('ActiveProjects Widget', () => {
  let ActiveProjectsWidget: any;

  beforeEach(async () => {
    jest.clearAllMocks();
    mockShadowRoot.innerHTML = '';
    
    // Dynamic import to avoid module loading issues
    const module = await import('../../../ActiveProjects/ActiveProjects');
    ActiveProjectsWidget = module.ActiveProjectsWidget;
  });

  describe('Constructor', () => {
    it('should initialize with correct properties', () => {
      const widget = new ActiveProjectsWidget();
      
      expect(widget.widgetName).toBe('ActiveProjects');
      expect(widget.widgetIcon).toBe('📋');
      expect(widget.widgetTitle).toBe('Active Projects');
    });

    it('should have static CSS declaration', () => {
      expect(ActiveProjectsWidget.getOwnCSS()).toEqual(['ActiveProjects.css']);
    });
  });

  describe('Mock Data Loading', () => {
    it('should load mock data when not connected', async () => {
      const widget = new ActiveProjectsWidget();
      
      // Mock disconnected state
      mockWindow.continuum.isConnected = () => false;
      
      widget.loadMockData();
      
      expect(widget.projects).toBeDefined();
      expect(widget.projects.length).toBeGreaterThan(0);
      expect(widget.projects[0]).toHaveProperty('id');
      expect(widget.projects[0]).toHaveProperty('name');
      expect(widget.projects[0]).toHaveProperty('status');
    });

    it('should include expected project properties', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      const project = widget.projects[0];
      expect(project).toHaveProperty('id');
      expect(project).toHaveProperty('name');
      expect(project).toHaveProperty('status');
      expect(project).toHaveProperty('progress');
      expect(project).toHaveProperty('lastActivity');
      expect(project).toHaveProperty('assignedAgents');
      expect(project).toHaveProperty('priority');
    });
  });

  describe('Rendering', () => {
    it('should render content with projects', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      const content = widget.renderContent();
      
      expect(content).toContain('project-list');
      expect(content).toContain('New Project');
      expect(content).toContain('Refresh');
    });

    it('should render empty state when no projects', () => {
      const widget = new ActiveProjectsWidget();
      widget.projects = [];
      
      const content = widget.renderContent();
      
      expect(content).toContain('empty-state');
      expect(content).toContain('No active projects');
    });

    it('should render individual projects correctly', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      const project = widget.projects[0];
      const projectHtml = widget.renderProject(project);
      
      expect(projectHtml).toContain(project.name);
      expect(projectHtml).toContain(project.status);
      expect(projectHtml).toContain(`${project.progress}%`);
    });
  });

  describe('Priority and Status Icons', () => {
    it('should return correct priority icons', () => {
      const widget = new ActiveProjectsWidget();
      
      expect(widget.getPriorityIcon('high')).toBe('🔴');
      expect(widget.getPriorityIcon('medium')).toBe('🟡');
      expect(widget.getPriorityIcon('low')).toBe('🟢');
      expect(widget.getPriorityIcon('unknown')).toBe('⚪');
    });

    it('should return correct status icons', () => {
      const widget = new ActiveProjectsWidget();
      
      expect(widget.getStatusIcon('active')).toBe('▶️');
      expect(widget.getStatusIcon('paused')).toBe('⏸️');
      expect(widget.getStatusIcon('completed')).toBe('✅');
      expect(widget.getStatusIcon('planning')).toBe('📋');
      expect(widget.getStatusIcon('unknown')).toBe('❓');
    });
  });

  describe('Project Selection', () => {
    it('should select project by ID', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      const projectId = widget.projects[0].id;
      widget.selectProject(projectId);
      
      expect(widget.selectedProject).toBeDefined();
      expect(widget.selectedProject?.id).toBe(projectId);
    });

    it('should not select non-existent project', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      widget.selectProject('non-existent-id');
      
      expect(widget.selectedProject).toBeNull();
    });
  });

  describe('Project Progress Updates', () => {
    it('should update project progress', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      const projectId = widget.projects[0].id;
      const newProgress = 85;
      
      widget.updateProjectProgress(projectId, newProgress);
      
      const updatedProject = widget.projects.find(p => p.id === projectId);
      expect(updatedProject?.progress).toBe(newProgress);
    });

    it('should not update non-existent project progress', () => {
      const widget = new ActiveProjectsWidget();
      widget.loadMockData();
      
      const originalProjects = [...widget.projects];
      widget.updateProjectProgress('non-existent-id', 85);
      
      expect(widget.projects).toEqual(originalProjects);
    });
  });

  describe('Action Handling', () => {
    it('should handle refresh action', async () => {
      const widget = new ActiveProjectsWidget();
      widget.loadProjects = jest.fn();
      
      await widget.handleAction('refresh');
      
      expect(widget.loadProjects).toHaveBeenCalled();
    });

    it('should handle create action with API call', async () => {
      const widget = new ActiveProjectsWidget();
      mockWindow.continuum.execute.mockResolvedValue({ success: true });
      
      await widget.handleAction('create');
      
      expect(mockWindow.continuum.execute).toHaveBeenCalledWith('projects', { action: 'create' });
    });

    it('should handle unknown action gracefully', async () => {
      const widget = new ActiveProjectsWidget();
      const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
      
      await widget.handleAction('unknown');
      
      expect(consoleSpy).toHaveBeenCalledWith('🎛️ ActiveProjects: Unknown action:', 'unknown');
      consoleSpy.mockRestore();
    });
  });
});