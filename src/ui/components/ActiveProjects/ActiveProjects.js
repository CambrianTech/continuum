/**
 * Active Projects Widget
 * Modular project management widget extending SidebarWidget
 */

// Import sidebar widget functionality
import('../shared/SidebarWidget.js');

if (!customElements.get('active-projects')) {

class ActiveProjects extends SidebarWidget {
  constructor() {
    super();
    
    // Widget metadata
    this.widgetName = 'ActiveProjects';
    this.widgetIcon = '📁';
    this.widgetCategory = 'User Interface';
    
    // Start collapsed by default
    this.isCollapsed = true;
    
    // Projects-specific state
    this.projects = [];
    this.selectedProject = null;
    this.filter = 'all'; // all, active, completed, archived
  }

  render() {
    const headerTitle = this.getAttribute('title') || 'Active Projects';
    
    const content = `
      <div class="workspace-info" id="workspace-info">
        ${this.renderWorkspaceInfo()}
      </div>
    `;
    
    this.shadowRoot.innerHTML = `
      <style>
        ${this.getHeaderStyle()}
        
        .workspace-card {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 15px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: 8px;
          border: 1px solid rgba(79, 195, 247, 0.3);
        }
        
        .workspace-icon {
          font-size: 24px;
          color: #4FC3F7;
        }
        
        .workspace-details {
          flex: 1;
          min-width: 0;
        }
        
        .workspace-name {
          color: #e0e6ed;
          font-size: 14px;
          font-weight: 500;
          margin-bottom: 4px;
        }
        
        .workspace-path {
          color: #8a92a5;
          font-size: 12px;
          font-family: monospace;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      </style>
      
      ${this.renderSidebarStructure(headerTitle, content)}
    `;
  }

  renderWorkspaceInfo() {
    const workspaceName = this.getCurrentWorkspaceName();
    const workspacePath = this.getCurrentWorkspacePath();
    
    return `
      <div class="workspace-card">
        <div class="workspace-icon">📁</div>
        <div class="workspace-details">
          <div class="workspace-name">${workspaceName}</div>
          <div class="workspace-path">${workspacePath}</div>
        </div>
      </div>
    `;
  }

  getFilteredProjects() {
    if (this.filter === 'all') return this.projects;
    return this.projects.filter(project => project.status === this.filter);
  }

  setupEventListeners() {
    super.setupEventListeners(); // CRITICAL: Enable collapse functionality
    
    // Listen for WebSocket events
    this.setupWebSocketListeners();
  }
  
  setupWebSocketListeners() {
    // Connect to continuum WebSocket for real-time updates
    if (typeof window !== 'undefined' && window.ws) {
      window.ws.addEventListener('message', (event) => {
        try {
          const data = JSON.parse(event.data);
          if (data.type === 'projects_updated' || data.type === 'project_added' || data.type === 'project_deleted' || data.type === 'project_status_changed') {
            console.log('📁 ActiveProjects: WebSocket update received', data.type);
            this.refresh(); // Auto-refresh when projects change
          }
        } catch (error) {
          // Ignore non-JSON messages
        }
      });
    }
  }

  setFilter(newFilter) {
    this.filter = newFilter;
    this.render();
    this.setupEventListeners();
  }

  handleProjectClick(event) {
    const projectItem = event.target.closest('.project-item');
    const actionBtn = event.target.closest('.project-action');
    
    if (actionBtn) {
      event.stopPropagation();
      const action = actionBtn.dataset.action;
      const projectId = actionBtn.dataset.projectId;
      this.handleProjectAction(action, projectId);
      return;
    }
    
    if (projectItem) {
      const projectId = projectItem.dataset.projectId;
      this.selectProject(projectId);
    }
  }

  selectProject(projectId) {
    const project = this.projects.find(p => p.id === projectId);
    if (project) {
      this.selectedProject = project;
      this.updateProjectList();
      
      // Dispatch custom event for parent components
      this.dispatchEvent(new CustomEvent('project-selected', {
        detail: { project },
        bubbles: true
      }));
    }
  }

  async handleProjectAction(action, projectId) {
    try {
      switch (action) {
        case 'open':
          await this.openProject(projectId);
          break;
        case 'reveal':
          await this.revealProject(projectId);
          break;
        case 'archive':
          await this.archiveProject(projectId);
          break;
        case 'delete':
          await this.deleteProject(projectId);
          break;
      }
    } catch (error) {
      console.error(`Project action ${action} failed:`, error);
      this.setError(true, `Failed to ${action} project`);
    }
  }

  async openProject(projectId) {
    // Dispatch event to parent to handle project opening
    this.dispatchEvent(new CustomEvent('project-open', {
      detail: { projectId },
      bubbles: true
    }));
  }

  async archiveProject(projectId) {
    const response = await this.apiCall(`/api/projects/${projectId}/archive`, {
      method: 'POST'
    });
    await this.refresh(); // Refresh list after archiving
  }

  async revealProject(projectId) {
    const project = this.projects.find(p => p.id === projectId);
    if (project && project.path) {
      // Dispatch event to parent to reveal directory in system file manager
      this.dispatchEvent(new CustomEvent('directory-reveal', {
        detail: { path: project.path },
        bubbles: true
      }));
    }
  }

  async deleteProject(projectId) {
    if (confirm('Are you sure you want to delete this project?')) {
      await this.apiCall(`/api/projects/${projectId}`, {
        method: 'DELETE'
      });
      await this.refresh(); // Refresh list after deletion
    }
  }

  async createProject() {
    // In continuum context, "creating project" means opening a directory
    // This should trigger a file picker to select a directory to open continuum in
    this.dispatchEvent(new CustomEvent('directory-open', {
      bubbles: true
    }));
  }

  getCurrentWorkspaceName() {
    if (typeof process !== 'undefined' && process.cwd) {
      return process.cwd().split('/').pop();
    }
    return 'continuum';
  }

  getCurrentWorkspacePath() {
    if (typeof process !== 'undefined' && process.cwd) {
      const fullPath = process.cwd();
      // Show shortened path for readability
      const parts = fullPath.split('/');
      if (parts.length > 3) {
        return '.../' + parts.slice(-2).join('/');
      }
      return fullPath;
    }
    return '/current/workspace';
  }

  async onRefresh() {
    // Just update the workspace info display
    this.updateWorkspaceInfo();
  }

  getMockProjects() {
    // In continuum context, "projects" are directories where continuum is running
    // Each directory represents a workspace with its own .continuum folder
    const currentDir = process.cwd ? process.cwd().split('/').pop() : 'continuum';
    
    return [
      {
        id: 'current',
        name: currentDir,
        status: 'active',
        progress: 0,
        path: process.cwd ? process.cwd() : '/current/directory',
        tasks: { completed: 0, total: 0 },
        isCurrentProject: true
      }
    ];
  }

  updateWorkspaceInfo() {
    const workspaceInfo = this.shadowRoot.getElementById('workspace-info');
    if (workspaceInfo) {
      workspaceInfo.innerHTML = this.renderWorkspaceInfo();
    }
  }

  updateLoadingState() {
    this.updateWorkspaceInfo();
  }

  updateErrorState() {
    this.updateWorkspaceInfo();
  }

  initializeWidget() {
    // Initial load - just show current workspace info
    this.refresh();
  }
}

// Register the custom element
customElements.define('active-projects', ActiveProjects);

// Export for module systems
if (typeof module !== 'undefined' && module.exports) {
  module.exports = ActiveProjects;
} else if (typeof window !== 'undefined') {
  window.ActiveProjects = ActiveProjects;
}

} // End guard