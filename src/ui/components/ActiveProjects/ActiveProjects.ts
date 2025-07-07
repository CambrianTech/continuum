/**
 * ActiveProjects Widget - TypeScript Implementation
 * Shows current active projects with progress tracking
 */
import { BaseWidget } from '../shared/BaseWidget';

interface Project {
  id: string;
  name: string;
  status: 'active' | 'paused' | 'completed' | 'planning';
  progress: number;
  lastActivity: string;
  assignedAgents: string[];
  priority: 'high' | 'medium' | 'low';
}

export class ActiveProjectsWidget extends BaseWidget {
  static getOwnCSS(): string[] {
    return ['ActiveProjects.css'];
  }
  private projects: Project[] = [];
  private selectedProject: Project | null = null;

  constructor() {
    super();
    this.widgetName = 'ActiveProjects';
    this.widgetIcon = '📋';
    this.widgetTitle = 'Active Projects';
  }

  async initializeWidget(): Promise<void> {
    await this.loadProjects();
    this.setupContinuumListeners();
  }

  setupContinuumListeners(): void {
    if (this.getContinuumAPI()) {
      this.onContinuumEvent('projects_updated', () => {
        console.log('🎛️ ActiveProjects: projects_updated received');
        this.loadProjects();
      });

      this.onContinuumEvent('project_progress_changed', (data: any) => {
        console.log('🎛️ ActiveProjects: project_progress_changed received', data);
        this.updateProjectProgress(data.projectId, data.progress);
      });

      console.log('🎛️ ActiveProjects: Connected to continuum API');
    } else {
      setTimeout(() => this.setupContinuumListeners(), 1000);
    }
  }

  async loadProjects(): Promise<void> {
    try {
      if (!this.isContinuumConnected()) {
        console.log('🎛️ ActiveProjects: Not connected, using mock data');
        this.loadMockData();
        return;
      }

      const response = await this.executeCommand('projects', { action: 'list_active' });
      if (response && response.projects) {
        this.projects = response.projects;
        console.log(`🎛️ ActiveProjects: Loaded ${this.projects.length} projects`);
      } else {
        this.loadMockData();
      }

      await this.update();
    } catch (error) {
      console.error('🎛️ ActiveProjects: Failed to load projects:', error);
      this.loadMockData();
    }
  }

  loadMockData(): void {
    this.projects = [
      {
        id: 'continuum-os',
        name: 'Continuum OS',
        status: 'active',
        progress: 75,
        lastActivity: '2 minutes ago',
        assignedAgents: ['Claude Sonnet', 'Protocol Sheriff'],
        priority: 'high'
      },
      {
        id: 'widget-system',
        name: 'Widget System',
        status: 'active',
        progress: 45,
        lastActivity: '15 minutes ago',
        assignedAgents: ['Code Specialist'],
        priority: 'medium'
      }
    ];
    this.update();
  }

  updateProjectProgress(projectId: string, progress: number): void {
    const project = this.projects.find(p => p.id === projectId);
    if (project) {
      project.progress = progress;
      this.update();
    }
  }

  renderContent(): string {
    const content = `
      <div class="project-list">
        ${this.projects.length === 0 ? this.renderEmptyState() : this.projects.map(project => this.renderProject(project)).join('')}
      </div>

      <div class="actions">
        <button class="btn btn-primary" data-action="create">New Project</button>
        <button class="btn btn-secondary" data-action="refresh">Refresh</button>
      </div>
    `;

    return this.renderWithCollapseHeader(content);
  }

  renderProject(project: Project): string {
    const isSelected = this.selectedProject?.id === project.id;
    const priorityIcon = this.getPriorityIcon(project.priority);
    const statusIcon = this.getStatusIcon(project.status);
    
    return `
      <div class="project-item ${isSelected ? 'selected' : ''}" data-project-id="${project.id}">
        <div class="project-header">
          <div class="project-name">${project.name}</div>
          <div class="project-indicators">
            <span class="priority-indicator priority-${project.priority}">${priorityIcon}</span>
            <span class="status-indicator status-${project.status}">${statusIcon}</span>
          </div>
        </div>
        
        <div class="project-progress">
          <div class="progress-bar">
            <div class="progress-fill" style="width: ${project.progress}%"></div>
          </div>
          <span class="progress-text">${project.progress}%</span>
        </div>
        
        <div class="project-details">
          <div class="last-activity">${project.lastActivity}</div>
          <div class="assigned-agents">
            ${project.assignedAgents.slice(0, 2).map(agent => `<span class="agent-tag">${agent}</span>`).join('')}
            ${project.assignedAgents.length > 2 ? `<span class="agent-count">+${project.assignedAgents.length - 2}</span>` : ''}
          </div>
        </div>
      </div>
    `;
  }

  getPriorityIcon(priority: string): string {
    switch (priority) {
      case 'high': return '🔴';
      case 'medium': return '🟡';
      case 'low': return '🟢';
      default: return '⚪';
    }
  }

  getStatusIcon(status: string): string {
    switch (status) {
      case 'active': return '▶️';
      case 'paused': return '⏸️';
      case 'completed': return '✅';
      case 'planning': return '📋';
      default: return '❓';
    }
  }

  renderEmptyState(): string {
    return `
      <div class="empty-state">
        No active projects.<br>
        Create your first project to get started!
      </div>
    `;
  }

  setupEventListeners(): void {
    // Project selection and action buttons
    this.shadowRoot.addEventListener('click', (e: Event) => {
      const target = e.target as HTMLElement;
      
      // Project selection
      const projectItem = target.closest('.project-item') as HTMLElement;
      if (projectItem) {
        const projectId = projectItem.dataset.projectId;
        if (projectId) {
          this.selectProject(projectId);
        }
        return;
      }

      // Action buttons
      if (target.matches('.btn')) {
        const action = target.getAttribute('data-action');
        if (action) {
          this.handleAction(action);
        }
      }
    });
  }

  selectProject(projectId: string): void {
    const project = this.projects.find(p => p.id === projectId);
    if (project) {
      this.selectedProject = project;
      console.log('🎛️ ActiveProjects: Selected project:', project.name);
      
      this.sendMessage({
        type: 'project_selected',
        project: project
      });

      this.update();
    }
  }

  async handleAction(action: string): Promise<void> {
    switch (action) {
      case 'create':
        console.log('🎛️ ActiveProjects: Creating new project...');
        try {
          await this.executeCommand('projects', { action: 'create' });
        } catch (error) {
          console.error('🎛️ ActiveProjects: Failed to create project:', error);
        }
        break;

      case 'refresh':
        console.log('🎛️ ActiveProjects: Refreshing projects...');
        await this.loadProjects();
        break;

      default:
        console.log('🎛️ ActiveProjects: Unknown action:', action);
    }
  }
}

// Register the custom element
if (!customElements.get('active-projects')) {
  customElements.define('active-projects', ActiveProjectsWidget);
}