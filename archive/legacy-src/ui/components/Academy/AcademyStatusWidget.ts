/**
 * Academy Status Widget - Real-time Academy system status and progress
 * 
 * Web Component following new declarative architecture:
 * - Shows training progress, P2P network status, genome discovery
 * - Integrates with existing ChatRoom and Personas widgets
 * - Displays Academy-specific metrics and controls
 * - Discoverable module via package.json
 */

import { BaseWidget } from '../shared/BaseWidget';

interface TrainingSession {
  readonly id: string;
  readonly persona_id: string;
  readonly status: 'active' | 'converging' | 'completed' | 'failed';
  readonly progress: number;
  readonly convergence_time: number;
  readonly estimated_completion: Date;
  readonly formula_id: string;
}

interface P2PNetworkNode {
  readonly id: string;
  readonly location: string;
  readonly status: 'online' | 'offline' | 'syncing';
  readonly genome_count: number;
  readonly last_sync: Date;
}

interface GenomeDiscoveryResult {
  readonly genome_id: string;
  readonly domain: string;
  readonly performance: number;
  readonly compatibility_score: number;
  readonly source_node: string;
}

export class AcademyStatusWidget extends BaseWidget {
  private activeSessions: TrainingSession[] = [];
  private p2pNodes: P2PNetworkNode[] = [];
  private recentDiscoveries: GenomeDiscoveryResult[] = [];
  private updateInterval: number | null = null;

  static getBasePath(): string {
    return '/src/ui/components/Academy';
  }

  static async getWidgetFiles(): Promise<string[]> {
    try {
      // Academy widgets use their own package.json files
      const packagePath = `${this.getBasePath()}/AcademyStatusWidget.package.json`;
      
      console.log(`📦 ${this.name}: Fetching package.json from ${packagePath}`);
      const response = await fetch(packagePath);
      if (!response.ok) {
        console.warn(`📦 No package.json found for ${this.name} at ${packagePath}`);
        return [];
      }
      
      const packageData = await response.json();
      return packageData.files || [];
    } catch (error) {
      console.warn(`📦 Failed to read package.json for ${this.name}:`, error);
      return [];
    }
  }

  constructor() {
    super();
    this.widgetName = 'Academy Status';
    this.widgetIcon = '🎓';
    this.widgetTitle = 'Academy System Status';
  }


  setupEventListeners(): void {
    // Academy control buttons
    this.setupAcademyControls();
    
    // Training session interactions
    this.setupTrainingSessionHandlers();
    
    // P2P network interactions
    this.setupP2PNetworkHandlers();
    
    // Genome discovery interactions
    this.setupGenomeDiscoveryHandlers();
    
    // Initial data load
    this.loadAcademyStatus();
  }

  private setupAcademyControls(): void {
    const spawnButton = this.shadowRoot.querySelector('#spawn-persona-btn');
    const trainButton = this.shadowRoot.querySelector('#start-training-btn');
    const syncButton = this.shadowRoot.querySelector('#sync-p2p-btn');
    const discoverButton = this.shadowRoot.querySelector('#discover-genomes-btn');

    spawnButton?.addEventListener('click', () => this.spawnPersona());
    trainButton?.addEventListener('click', () => this.startTraining());
    syncButton?.addEventListener('click', () => this.syncP2PNetwork());
    discoverButton?.addEventListener('click', () => this.discoverGenomes());
  }

  private setupTrainingSessionHandlers(): void {
    const sessionList = this.shadowRoot.querySelector('#training-sessions');
    if (!sessionList) return;

    sessionList.addEventListener('click', (e) => {
      const sessionItem = (e.target as HTMLElement).closest('.training-session');
      if (sessionItem) {
        const sessionId = sessionItem.getAttribute('data-session-id');
        if (sessionId) {
          this.showTrainingDetails(sessionId);
        }
      }
    });
  }

  private setupP2PNetworkHandlers(): void {
    const networkList = this.shadowRoot.querySelector('#p2p-nodes');
    if (!networkList) return;

    networkList.addEventListener('click', (e) => {
      const nodeItem = (e.target as HTMLElement).closest('.p2p-node');
      if (nodeItem) {
        const nodeId = nodeItem.getAttribute('data-node-id');
        if (nodeId) {
          this.showNodeDetails(nodeId);
        }
      }
    });
  }

  private setupGenomeDiscoveryHandlers(): void {
    const discoveryList = this.shadowRoot.querySelector('#genome-discoveries');
    if (!discoveryList) return;

    discoveryList.addEventListener('click', (e) => {
      const discoveryItem = (e.target as HTMLElement).closest('.genome-discovery');
      if (discoveryItem) {
        const genomeId = discoveryItem.getAttribute('data-genome-id');
        if (genomeId) {
          this.showGenomeDetails(genomeId);
        }
      }
    });
  }

  private async loadAcademyStatus(): Promise<void> {
    try {
      // MOCK DATA: Use client-side data until Academy commands are working
      const mockStatus = {
        training_sessions: [
          {
            id: 'session_1',
            persona_id: 'DevMaster',
            status: 'active' as const,
            progress: 67,
            convergence_time: 180,
            estimated_completion: new Date(Date.now() + 3 * 60 * 1000),
            formula_id: 'formula_typescript_optimization'
          },
          {
            id: 'session_2', 
            persona_id: 'CodeAI',
            status: 'converging' as const,
            progress: 23,
            convergence_time: 480,
            estimated_completion: new Date(Date.now() + 8 * 60 * 1000),
            formula_id: 'formula_error_handling'
          }
        ],
        p2p_network: [
          {
            id: 'node_1',
            location: 'Local Network',
            status: 'online' as const,
            genome_count: 847,
            last_sync: new Date(Date.now() - 2 * 60 * 1000)
          },
          {
            id: 'node_2',
            location: 'Academy Network',
            status: 'syncing' as const,
            genome_count: 1203,
            last_sync: new Date(Date.now() - 5 * 60 * 1000)
          }
        ],
        recent_discoveries: [
          {
            genome_id: 'genome_001',
            domain: 'TypeScript Optimization',
            performance: 94.7,
            compatibility_score: 92.1,
            source_node: 'FormulaMaster'
          },
          {
            genome_id: 'genome_002',
            domain: 'Error Handling Patterns', 
            performance: 89.2,
            compatibility_score: 87.5,
            source_node: 'SynthesisEngine'
          }
        ]
      };
      
      this.activeSessions = mockStatus.training_sessions;
      this.p2pNodes = mockStatus.p2p_network;
      this.recentDiscoveries = mockStatus.recent_discoveries;
      
      this.updateTrainingSessions();
      this.updateP2PNetwork();
      this.updateGenomeDiscoveries();
      
      console.log('🎓 Academy Status Widget: Mock data loaded successfully');
    } catch (error) {
      console.warn('Academy Status Widget: Failed to load Academy status:', error);
      this.showOfflineState();
    }
  }

  private updateTrainingSessions(): void {
    const container = this.shadowRoot.querySelector('#training-sessions');
    if (!container) return;

    if (this.activeSessions.length === 0) {
      container.innerHTML = '<div class="empty-state">No active training sessions</div>';
      return;
    }

    container.innerHTML = this.activeSessions.map(session => `
      <div class="training-session ${session.status}" data-session-id="${session.id}">
        <div class="session-header">
          <span class="session-persona">${session.persona_id}</span>
          <span class="session-status status-${session.status}">${session.status}</span>
        </div>
        <div class="session-progress">
          <div class="progress-bar">
            <div class="progress-fill" style="width: ${session.progress}%"></div>
          </div>
          <span class="progress-text">${session.progress.toFixed(1)}%</span>
        </div>
        <div class="session-meta">
          <span class="convergence-time">${session.convergence_time}s convergence</span>
          <span class="estimated-completion">${this.formatTimeRemaining(session.estimated_completion)}</span>
        </div>
      </div>
    `).join('');
  }

  private updateP2PNetwork(): void {
    const container = this.shadowRoot.querySelector('#p2p-nodes');
    if (!container) return;

    if (this.p2pNodes.length === 0) {
      container.innerHTML = '<div class="empty-state">No P2P connections</div>';
      return;
    }

    container.innerHTML = this.p2pNodes.map(node => `
      <div class="p2p-node ${node.status}" data-node-id="${node.id}">
        <div class="node-header">
          <span class="node-location">${node.location}</span>
          <span class="node-status status-${node.status}">●</span>
        </div>
        <div class="node-details">
          <span class="genome-count">${node.genome_count} genomes</span>
          <span class="last-sync">Synced ${this.formatTimeAgo(node.last_sync)}</span>
        </div>
      </div>
    `).join('');
  }

  private updateGenomeDiscoveries(): void {
    const container = this.shadowRoot.querySelector('#genome-discoveries');
    if (!container) return;

    if (this.recentDiscoveries.length === 0) {
      container.innerHTML = '<div class="empty-state">No recent discoveries</div>';
      return;
    }

    container.innerHTML = this.recentDiscoveries.slice(0, 5).map(discovery => `
      <div class="genome-discovery" data-genome-id="${discovery.genome_id}">
        <div class="discovery-header">
          <span class="genome-id">${discovery.genome_id}</span>
          <span class="performance-score">${(discovery.performance * 100).toFixed(1)}%</span>
        </div>
        <div class="discovery-details">
          <span class="domain">${discovery.domain}</span>
          <span class="compatibility">compat: ${(discovery.compatibility_score * 100).toFixed(1)}%</span>
        </div>
        <div class="source-node">from ${discovery.source_node}</div>
      </div>
    `).join('');
  }


  private async spawnPersona(): Promise<void> {
    try {
      const result = await this.executeCommand('academy.spawnPersona', {
        domains: ['general_intelligence'],
        capabilities: ['problem_solving', 'creative_thinking']
      });
      
      if (result.success) {
        this.showNotification(`✨ Spawned new persona: ${result.persona_id}`, 'success');
        this.loadAcademyStatus(); // Refresh status
      }
    } catch (error) {
      this.showNotification(`❌ Failed to spawn persona: ${error}`, 'error');
    }
  }

  private async startTraining(): Promise<void> {
    try {
      const result = await this.executeCommand('academy.startTraining', {
        training_type: 'adversarial',
        difficulty_progression: 'adaptive'
      });
      
      if (result.success) {
        this.showNotification(`🎯 Training started: ${result.session_id}`, 'success');
        this.loadAcademyStatus(); // Refresh status
      }
    } catch (error) {
      this.showNotification(`❌ Failed to start training: ${error}`, 'error');
    }
  }

  private async syncP2PNetwork(): Promise<void> {
    try {
      const result = await this.executeCommand('academy.syncP2P', {});
      
      if (result.success) {
        this.showNotification(`🔄 P2P sync initiated: ${result.nodes_contacted} nodes`, 'success');
        this.loadAcademyStatus(); // Refresh status
      }
    } catch (error) {
      this.showNotification(`❌ P2P sync failed: ${error}`, 'error');
    }
  }

  private async discoverGenomes(): Promise<void> {
    try {
      const result = await this.executeCommand('academy.discoverGenomes', {
        domains: ['machine_learning', 'problem_solving'],
        min_performance: 0.7
      });
      
      if (result.success) {
        this.showNotification(`🔍 Discovered ${result.genome_count} new genomes`, 'success');
        this.loadAcademyStatus(); // Refresh status
      }
    } catch (error) {
      this.showNotification(`❌ Genome discovery failed: ${error}`, 'error');
    }
  }

  private showTrainingDetails(sessionId: string): void {
    const session = this.activeSessions.find(s => s.id === sessionId);
    if (!session) return;

    // Dispatch event for other widgets to handle
    const detailEvent = new CustomEvent('academy:training-details', {
      detail: { session },
      bubbles: true
    });
    document.dispatchEvent(detailEvent);
  }

  private showNodeDetails(nodeId: string): void {
    const node = this.p2pNodes.find(n => n.id === nodeId);
    if (!node) return;

    // Dispatch event for other widgets to handle
    const detailEvent = new CustomEvent('academy:node-details', {
      detail: { node },
      bubbles: true
    });
    document.dispatchEvent(detailEvent);
  }

  private showGenomeDetails(genomeId: string): void {
    const genome = this.recentDiscoveries.find(g => g.genome_id === genomeId);
    if (!genome) return;

    // Dispatch event for other widgets to handle
    const detailEvent = new CustomEvent('academy:genome-details', {
      detail: { genome },
      bubbles: true
    });
    document.dispatchEvent(detailEvent);
  }

  private showNotification(message: string, type: 'success' | 'error' | 'info'): void {
    const notification = document.createElement('div');
    notification.className = `academy-notification ${type}`;
    notification.textContent = message;
    
    // Add to widget temporarily
    this.shadowRoot.appendChild(notification);
    
    // Auto-remove after 3 seconds
    setTimeout(() => {
      if (notification.parentNode) {
        notification.parentNode.removeChild(notification);
      }
    }, 3000);
  }

  private showOfflineState(): void {
    const statusElement = this.shadowRoot.querySelector('.academy-status');
    if (statusElement) {
      statusElement.textContent = 'Academy Offline';
      statusElement.className = 'academy-status offline';
    }
  }

  private formatTimeRemaining(completionTime: Date): string {
    const now = new Date();
    const diff = completionTime.getTime() - now.getTime();
    const minutes = Math.floor(diff / 60000);
    const seconds = Math.floor((diff % 60000) / 1000);
    
    if (minutes > 0) {
      return `${minutes}m ${seconds}s`;
    } else {
      return `${seconds}s`;
    }
  }

  private formatTimeAgo(date: Date): string {
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 60000);
    
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }


  cleanup(): void {
    if (this.updateInterval) {
      clearInterval(this.updateInterval);
      this.updateInterval = null;
    }
  }
}

// Register as web component
customElements.define('academy-status-widget', AcademyStatusWidget);