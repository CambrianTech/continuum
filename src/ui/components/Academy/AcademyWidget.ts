/**
 * Academy Widget - AI Training and Evolution Interface
 * 
 * Provides real-time visibility into Academy training sessions, 
 * persona evolution, and P2P network activity
 */

import { BaseWidget } from '../shared/BaseWidget.js';

interface TrainingSession {
  id: string;
  persona_name: string;
  trainer_mode: 'adversarial' | 'collaborative' | 'autonomous';
  domain: string;
  progress: number;
  evolution_metrics: {
    capability_improvement: number;
    vector_space_movement: number;
    lora_optimization_ratio: number;
  };
  start_time: Date;
  estimated_completion: Date;
  status: 'active' | 'paused' | 'completed' | 'failed';
}

interface PersonaEvolution {
  persona_id: string;
  persona_name: string;
  generation: number;
  vector_position: number[];
  capabilities: string[];
  training_sessions_completed: number;
  p2p_connections: number;
  evolution_potential: number;
  last_evolution: Date;
}

interface AcademyStats {
  active_training_sessions: number;
  total_personas: number;
  p2p_network_size: number;
  total_lora_layers: number;
  vector_space_dimensions: number;
  evolution_cycles_completed: number;
  academy_uptime: string;
}

export class AcademyWidget extends BaseWidget {
  private trainingSessions: TrainingSession[] = [];
  private personas: PersonaEvolution[] = [];
  private stats: AcademyStats | null = null;
  private updateInterval: number | null = null;
  private selectedView: 'overview' | 'training' | 'personas' | 'evolution' = 'overview';

  constructor() {
    super();
    this.widgetName = 'Academy';
    this.widgetIcon = '🎓';
    this.widgetTitle = 'AI Academy';
  }

  protected async initializeWidget(): Promise<void> {
    await this.loadAcademyStatus();
    this.setupEventListeners();
    this.startRealTimeUpdates();
  }

  getBundledCSS(): string {
    return `
      :host {
        display: block;
        background: linear-gradient(135deg, rgba(30, 15, 50, 0.95), rgba(15, 30, 45, 0.95));
        border-radius: 16px;
        padding: 20px;
        margin: 8px;
        border: 2px solid rgba(147, 51, 234, 0.3);
        color: #e5e7eb;
        font-family: 'Monaco', 'Menlo', monospace;
        min-width: 350px;
        max-width: 500px;
        box-shadow: 0 8px 32px rgba(147, 51, 234, 0.2);
      }

      .academy-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 16px;
        padding-bottom: 12px;
        border-bottom: 1px solid rgba(147, 51, 234, 0.3);
      }

      .academy-title {
        font-size: 18px;
        font-weight: bold;
        color: #a855f7;
        display: flex;
        align-items: center;
        gap: 8px;
      }

      .academy-status {
        font-size: 12px;
        padding: 4px 8px;
        border-radius: 12px;
        background: rgba(34, 197, 94, 0.2);
        color: #22c55e;
        border: 1px solid rgba(34, 197, 94, 0.3);
      }

      .view-tabs {
        display: flex;
        gap: 8px;
        margin-bottom: 16px;
      }

      .view-tab {
        padding: 6px 12px;
        border-radius: 8px;
        background: rgba(147, 51, 234, 0.1);
        border: 1px solid rgba(147, 51, 234, 0.3);
        color: #a855f7;
        cursor: pointer;
        font-size: 12px;
        transition: all 0.2s ease;
      }

      .view-tab:hover {
        background: rgba(147, 51, 234, 0.2);
      }

      .view-tab.active {
        background: rgba(147, 51, 234, 0.3);
        color: #e879f9;
      }

      .academy-stats {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
        margin-bottom: 16px;
      }

      .stat-card {
        background: rgba(0, 0, 0, 0.3);
        padding: 12px;
        border-radius: 8px;
        border: 1px solid rgba(147, 51, 234, 0.2);
      }

      .stat-value {
        font-size: 20px;
        font-weight: bold;
        color: #a855f7;
      }

      .stat-label {
        font-size: 11px;
        color: #9ca3af;
        margin-top: 4px;
      }

      .training-sessions {
        margin-bottom: 16px;
      }

      .training-session {
        background: rgba(0, 0, 0, 0.2);
        padding: 12px;
        border-radius: 8px;
        margin-bottom: 8px;
        border-left: 4px solid #22c55e;
      }

      .training-session.paused {
        border-left-color: #f59e0b;
      }

      .training-session.failed {
        border-left-color: #ef4444;
      }

      .session-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;
      }

      .session-persona {
        font-weight: bold;
        color: #e879f9;
      }

      .session-mode {
        font-size: 11px;
        padding: 2px 6px;
        border-radius: 4px;
        background: rgba(147, 51, 234, 0.2);
        color: #a855f7;
      }

      .session-progress {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 12px;
      }

      .progress-bar {
        flex: 1;
        height: 4px;
        background: rgba(147, 51, 234, 0.2);
        border-radius: 2px;
        overflow: hidden;
      }

      .progress-fill {
        height: 100%;
        background: linear-gradient(90deg, #a855f7, #e879f9);
        transition: width 0.3s ease;
      }

      .personas-grid {
        display: grid;
        grid-template-columns: 1fr;
        gap: 8px;
      }

      .persona-card {
        background: rgba(0, 0, 0, 0.2);
        padding: 10px;
        border-radius: 6px;
        border: 1px solid rgba(147, 51, 234, 0.2);
        cursor: pointer;
        transition: all 0.2s ease;
      }

      .persona-card:hover {
        background: rgba(147, 51, 234, 0.1);
        border-color: rgba(147, 51, 234, 0.4);
      }

      .persona-name {
        font-weight: bold;
        color: #e879f9;
        margin-bottom: 4px;
      }

      .persona-info {
        font-size: 11px;
        color: #9ca3af;
        display: flex;
        justify-content: space-between;
      }

      .evolution-view {
        text-align: center;
        padding: 20px;
      }

      .vector-space {
        background: rgba(0, 0, 0, 0.3);
        border-radius: 8px;
        padding: 16px;
        margin: 12px 0;
        min-height: 120px;
        border: 1px solid rgba(147, 51, 234, 0.3);
      }

      .academy-actions {
        display: flex;
        gap: 8px;
        margin-top: 16px;
      }

      .academy-button {
        flex: 1;
        padding: 8px 12px;
        border-radius: 6px;
        background: rgba(147, 51, 234, 0.2);
        border: 1px solid rgba(147, 51, 234, 0.4);
        color: #a855f7;
        cursor: pointer;
        font-size: 12px;
        transition: all 0.2s ease;
      }

      .academy-button:hover {
        background: rgba(147, 51, 234, 0.3);
        color: #e879f9;
      }

      .loading {
        text-align: center;
        color: #9ca3af;
        font-style: italic;
        padding: 20px;
      }

      .error {
        background: rgba(239, 68, 68, 0.1);
        border: 1px solid rgba(239, 68, 68, 0.3);
        color: #f87171;
        padding: 12px;
        border-radius: 8px;
        margin: 8px 0;
        font-size: 12px;
      }
    `;
  }

  render(): void {
    if (!this.container) return;

    this.container.innerHTML = `
      <div class="academy-header">
        <div class="academy-title">
          <span>${this.widgetIcon}</span>
          <span>${this.widgetTitle}</span>
        </div>
        <div class="academy-status">ACTIVE</div>
      </div>

      <div class="view-tabs">
        <div class="view-tab ${this.selectedView === 'overview' ? 'active' : ''}" data-view="overview">Overview</div>
        <div class="view-tab ${this.selectedView === 'training' ? 'active' : ''}" data-view="training">Training</div>
        <div class="view-tab ${this.selectedView === 'personas' ? 'active' : ''}" data-view="personas">Personas</div>
        <div class="view-tab ${this.selectedView === 'evolution' ? 'active' : ''}" data-view="evolution">Evolution</div>
      </div>

      <div class="academy-content">
        ${this.renderCurrentView()}
      </div>

      <div class="academy-actions">
        <button class="academy-button" data-action="spawn">Spawn Persona</button>
        <button class="academy-button" data-action="train">Start Training</button>
        <button class="academy-button" data-action="refresh">Refresh</button>
      </div>
    `;

    this.setupEventListeners();
  }

  private renderCurrentView(): string {
    switch (this.selectedView) {
      case 'overview':
        return this.renderOverview();
      case 'training':
        return this.renderTrainingView();
      case 'personas':
        return this.renderPersonasView();
      case 'evolution':
        return this.renderEvolutionView();
      default:
        return '<div class="loading">Loading Academy data...</div>';
    }
  }

  private renderOverview(): string {
    if (!this.stats) {
      return '<div class="loading">Loading Academy statistics...</div>';
    }

    return `
      <div class="academy-stats">
        <div class="stat-card">
          <div class="stat-value">${this.stats.active_training_sessions}</div>
          <div class="stat-label">Active Sessions</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${this.stats.total_personas}</div>
          <div class="stat-label">AI Personas</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${this.stats.p2p_network_size}</div>
          <div class="stat-label">P2P Network</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${this.stats.total_lora_layers}</div>
          <div class="stat-label">LoRA Layers</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${this.stats.vector_space_dimensions}</div>
          <div class="stat-label">Vector Dimensions</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${this.stats.evolution_cycles_completed}</div>
          <div class="stat-label">Evolution Cycles</div>
        </div>
      </div>
    `;
  }

  private renderTrainingView(): string {
    if (this.trainingSessions.length === 0) {
      return '<div class="loading">No active training sessions</div>';
    }

    return `
      <div class="training-sessions">
        ${this.trainingSessions.map(session => `
          <div class="training-session ${session.status}">
            <div class="session-header">
              <span class="session-persona">${session.persona_name}</span>
              <span class="session-mode">${session.trainer_mode}</span>
            </div>
            <div class="session-progress">
              <div class="progress-bar">
                <div class="progress-fill" style="width: ${session.progress}%"></div>
              </div>
              <span>${Math.round(session.progress)}%</span>
            </div>
            <div style="font-size: 11px; color: #9ca3af; margin-top: 4px;">
              Domain: ${session.domain} | Capability: +${(session.evolution_metrics.capability_improvement * 100).toFixed(1)}%
            </div>
          </div>
        `).join('')}
      </div>
    `;
  }

  private renderPersonasView(): string {
    if (this.personas.length === 0) {
      return '<div class="loading">No personas found</div>';
    }

    return `
      <div class="personas-grid">
        ${this.personas.map(persona => `
          <div class="persona-card" data-persona-id="${persona.persona_id}">
            <div class="persona-name">${persona.persona_name}</div>
            <div class="persona-info">
              <span>Gen ${persona.generation}</span>
              <span>${persona.training_sessions_completed} sessions</span>
              <span>${persona.p2p_connections} peers</span>
            </div>
            <div style="font-size: 10px; color: #6b7280; margin-top: 4px;">
              ${persona.capabilities.slice(0, 3).join(', ')}${persona.capabilities.length > 3 ? '...' : ''}
            </div>
          </div>
        `).join('')}
      </div>
    `;
  }

  private renderEvolutionView(): string {
    return `
      <div class="evolution-view">
        <div style="font-size: 14px; color: #a855f7; margin-bottom: 12px;">Vector Space Evolution</div>
        <div class="vector-space">
          <div style="font-size: 12px; color: #9ca3af;">
            ${this.stats ? this.stats.vector_space_dimensions : 512}-dimensional intelligence space
          </div>
          <div style="margin: 8px 0; font-size: 10px; color: #6b7280;">
            Personas evolving through adversarial training and P2P knowledge sharing
          </div>
          <div style="font-size: 11px; color: #a855f7;">
            Evolution cycles completed: ${this.stats ? this.stats.evolution_cycles_completed : 0}
          </div>
        </div>
      </div>
    `;
  }

  private setupEventListeners(): void {
    if (!this.container) return;

    // View tab switching
    this.container.querySelectorAll('.view-tab').forEach(tab => {
      tab.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        const view = target.getAttribute('data-view') as any;
        if (view) {
          this.selectedView = view;
          this.render();
        }
      });
    });

    // Action buttons
    this.container.querySelectorAll('.academy-button').forEach(button => {
      button.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        const action = target.getAttribute('data-action');
        this.handleAction(action);
      });
    });

    // Persona cards
    this.container.querySelectorAll('.persona-card').forEach(card => {
      card.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        const personaId = target.getAttribute('data-persona-id');
        if (personaId) {
          this.showPersonaDetails(personaId);
        }
      });
    });
  }

  private async handleAction(action: string | null): Promise<void> {
    switch (action) {
      case 'spawn':
        await this.spawnPersona();
        break;
      case 'train':
        await this.startTraining();
        break;
      case 'refresh':
        await this.loadAcademyStatus();
        break;
    }
  }

  private async spawnPersona(): Promise<void> {
    try {
      const personaName = prompt('Enter persona name:');
      if (!personaName) return;

      const result = await this.executeCommand('academy-spawn', {
        persona_name: personaName,
        specialization: 'auto-discover',
        p2p_seed: true
      });

      console.log('🎓 Academy: Persona spawning initiated:', result);
      await this.loadAcademyStatus();
    } catch (error) {
      console.error('🎓 Academy: Failed to spawn persona:', error);
    }
  }

  private async startTraining(): Promise<void> {
    try {
      if (this.personas.length === 0) {
        alert('No personas available for training. Spawn a persona first.');
        return;
      }

      const personaName = this.personas[0].persona_name; // Use first persona for demo
      const result = await this.executeCommand('academy-train', {
        student_persona: personaName,
        trainer_mode: 'adversarial',
        vector_exploration: true
      });

      console.log('🎓 Academy: Training session started:', result);
      await this.loadAcademyStatus();
    } catch (error) {
      console.error('🎓 Academy: Failed to start training:', error);
    }
  }

  private async showPersonaDetails(personaId: string): Promise<void> {
    const persona = this.personas.find(p => p.persona_id === personaId);
    if (persona) {
      // This could integrate with PersonaWidget or open a detail modal
      console.log('🎓 Academy: Persona details:', persona);
      alert(`Persona: ${persona.persona_name}\nGeneration: ${persona.generation}\nCapabilities: ${persona.capabilities.join(', ')}`);
    }
  }

  private async loadAcademyStatus(): Promise<void> {
    try {
      // Load Academy status via command system
      const statusResult = await this.executeCommand('academy-status', {
        detail_level: 'detailed',
        include_p2p: true,
        include_vector_space: true,
        include_adversarial: true
      });

      if (statusResult.success) {
        this.stats = statusResult.data.academy_overview;
        this.trainingSessions = statusResult.data.training_sessions || [];
        this.personas = statusResult.data.persona_status || [];
        this.render();
      } else {
        console.error('🎓 Academy: Failed to load Academy status:', statusResult.error);
      }
    } catch (error) {
      console.error('🎓 Academy: Error loading Academy status:', error);
    }
  }

  private startRealTimeUpdates(): void {
    // Update every 5 seconds
    this.updateInterval = window.setInterval(() => {
      this.loadAcademyStatus();
    }, 5000);
  }

  destroy(): void {
    if (this.updateInterval) {
      clearInterval(this.updateInterval);
      this.updateInterval = null;
    }
    // BaseWidget doesn't have destroy method
  }
}

// Register the widget
customElements.define('academy-widget', AcademyWidget);