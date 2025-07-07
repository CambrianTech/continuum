/**
 * UserSelector Widget - TypeScript Implementation
 * Provides agent and user selection functionality with search
 * Integrated with UniversalUserSystem for action buttons
 */
import { BaseWidget } from '../shared/BaseWidget';
import { universalUserSystem } from '../shared/UniversalUserSystem';

interface Agent {
  id: string;
  name: string;
  type: 'human' | 'ai';
  status: 'online' | 'offline' | 'busy';
  avatar?: string;
  lastSeen?: string;
  capabilities?: string[];
}

export class UserSelectorWidget extends BaseWidget {
  private agents: Agent[] = [];
  private selectedAgent: Agent | null = null;
  private searchQuery: string = '';
  private searchTimeout: number = 0;

  static getBasePath(): string {
    return '/src/ui/components/UserSelector';
  }

  static getOwnCSS(): string[] {
    return ['UserSelector.css'];
  }

  constructor() {
    super();
    this.widgetName = 'UserSelector';
    this.widgetIcon = '👥';
    this.widgetTitle = 'Users & Agents';
  }

  async initializeWidget(): Promise<void> {
    await this.loadAgents();
    this.setupContinuumListeners();
  }

  setupContinuumListeners(): void {
    if (this.getContinuumAPI()) {
      this.onContinuumEvent('agents_updated', () => {
        console.log('🎛️ UserSelector: agents_updated received');
        this.loadAgents();
      });

      this.onContinuumEvent('agent_status_changed', (data: any) => {
        console.log('🎛️ UserSelector: agent_status_changed received', data);
        this.updateAgentStatus(data.agentId, data.status);
      });

      console.log('🎛️ UserSelector: Connected to continuum API');
    } else {
      setTimeout(() => this.setupContinuumListeners(), 1000);
    }
  }

  async loadAgents(): Promise<void> {
    try {
      if (!this.isContinuumConnected()) {
        console.log('🎛️ UserSelector: Not connected, using mock data');
        this.loadMockData();
        return;
      }

      const response = await this.executeCommand('agents', { action: 'list' });
      if (response && response.agents) {
        this.agents = response.agents;
        console.log(`🎛️ UserSelector: Loaded ${this.agents.length} agents`);
      } else {
        this.loadMockData();
      }
      
      await this.update();
    } catch (error) {
      console.error('🎛️ UserSelector: Failed to load agents:', error);
      this.loadMockData();
    }
  }

  loadMockData(): void {
    this.agents = [
      {
        id: 'human-1',
        name: 'Joel (You)',
        type: 'human',
        status: 'online',
        avatar: '👤'
      },
      {
        id: 'claude-1',
        name: 'Claude Sonnet',
        type: 'ai',
        status: 'online',
        avatar: '🤖',
        capabilities: ['coding', 'analysis', 'writing']
      },
      {
        id: 'protocol-sheriff',
        name: 'Protocol Sheriff',
        type: 'ai',
        status: 'busy',
        avatar: '🛡️',
        capabilities: ['protocol enforcement', 'security']
      }
    ];
  }

  updateAgentStatus(agentId: string, status: string): void {
    const agent = this.agents.find(a => a.id === agentId);
    if (agent) {
      agent.status = status as 'online' | 'offline' | 'busy';
      this.update();
    }
  }

  renderContent(): string {
    const content = `
      <div class="search-container">
        <input type="text" class="search-input" placeholder="Search users and agents..." value="${this.searchQuery}">
      </div>

      <div class="agent-list">
        ${universalUserSystem.generateConnectedUsersHTML()}
      </div>

      <div class="actions">
        <button class="btn btn-primary" data-action="refresh">Refresh</button>
        <button class="btn btn-secondary" data-action="invite">Invite User</button>
      </div>
    `;

    return this.renderWithCollapseHeader(content);
  }

  getFilteredAgents(): Agent[] {
    if (!this.searchQuery.trim()) {
      return this.agents;
    }

    return this.agents.filter(agent =>
      agent.name.toLowerCase().includes(this.searchQuery.toLowerCase()) ||
      (agent.capabilities && agent.capabilities.some(cap => 
        cap.toLowerCase().includes(this.searchQuery.toLowerCase())
      ))
    );
  }

  renderAgent(agent: Agent): string {
    const isSelected = this.selectedAgent?.id === agent.id;
    const statusIcon = this.getStatusIcon(agent.status);
    
    return `
      <div class="agent-item ${isSelected ? 'selected' : ''}" data-agent-id="${agent.id}">
        <div class="agent-avatar">${agent.avatar || '👤'}</div>
        <div class="agent-info">
          <div class="agent-name">${agent.name}</div>
          <div class="agent-details">
            ${agent.type} ${statusIcon}
            ${agent.capabilities ? `• ${agent.capabilities.slice(0, 2).join(', ')}` : ''}
          </div>
        </div>
        <div class="agent-status">
          <div class="status-indicator status-${agent.status}"></div>
        </div>
      </div>
    `;
  }

  getStatusIcon(status: string): string {
    switch (status) {
      case 'online': return '🟢';
      case 'busy': return '🟡';
      case 'offline': return '🔴';
      default: return '⚪';
    }
  }

  setupEventListeners(): void {
    // Setup action button listeners for chat/train buttons
    const agentListContainer = this.shadowRoot?.querySelector('.agent-list');
    if (agentListContainer) {
      universalUserSystem.setupActionButtonListeners(agentListContainer as HTMLElement);
    }

    // Agent selection
    this.shadowRoot.addEventListener('click', (e: Event) => {
      const target = e.target as HTMLElement;
      const agentItem = target.closest('.user-badge') as HTMLElement;
      
      if (agentItem && agentItem.classList.contains('clickable')) {
        const userId = agentItem.dataset.userId;
        if (userId) {
          universalUserSystem.handleUserClick(userId, {
            source: 'user-selector-widget'
          });
        }
      }

      // Action buttons
      if (target.matches('.btn')) {
        const action = target.getAttribute('data-action');
        if (action) {
          this.handleAction(action);
        }
      }
    });

    // Search input listener with debouncing
    this.shadowRoot.addEventListener('input', (e: Event) => {
      const target = e.target as HTMLInputElement;
      if (target.matches('.search-input')) {
        clearTimeout(this.searchTimeout);
        this.searchTimeout = window.setTimeout(() => {
          this.searchQuery = target.value?.toLowerCase() || '';
          this.filterAndRender();
        }, 300);
      }
    });
  }

  selectAgent(agentId: string): void {
    const agent = this.agents.find(a => a.id === agentId);
    if (agent) {
      this.selectedAgent = agent;
      console.log('🎛️ UserSelector: Selected agent:', agent.name);
      
      this.sendMessage({
        type: 'agent_selected',
        agent: agent
      });

      this.update();
    }
  }

  async filterAndRender(): Promise<void> {
    await this.update();
  }

  async handleAction(action: string): Promise<void> {
    switch (action) {
      case 'refresh':
        console.log('🎛️ UserSelector: Refreshing agents...');
        await this.loadAgents();
        break;
      
      case 'invite':
        console.log('🎛️ UserSelector: Inviting user...');
        try {
          await this.executeCommand('agents', { action: 'invite' });
        } catch (error) {
          console.error('🎛️ UserSelector: Failed to invite user:', error);
        }
        break;
      
      default:
        console.log('🎛️ UserSelector: Unknown action:', action);
    }
  }
}

// Register the custom element
if (!customElements.get('user-selector')) {
  customElements.define('user-selector', UserSelectorWidget);
}