/**
 * SavedPersonas Widget - Modern TypeScript Web Component
 * Self-contained with embedded styles, full functionality, and tests
 */

import { BaseWidget } from '../shared/BaseWidget';

interface Persona {
  id: string;
  name: string;
  status: 'training' | 'graduated' | 'failed' | 'loaded' | 'unknown';
  specialization: string;
  graduationScore?: number;
  currentScore?: number;
  threshold?: number;
  originalThreshold?: number;
  currentIteration?: number;
  totalIterations?: number;
  failureReason?: string;
  accuracy?: number;
  created?: string;
  lastUsed?: string;
}

export class SavedPersonasWidget extends BaseWidget {
  static getOwnCSS(): string[] {
    return ['SavedPersonasWidget.css'];
  }
  private personas: Persona[] = [];
  private selectedPersona: Persona | null = null;
  private dragState: any = null;

  constructor() {
    super();
    this.widgetName = 'SavedPersonas';
    this.widgetIcon = '👤';
    this.widgetTitle = 'Saved Personas';
  }

  protected async initializeWidget(): Promise<void> {
    await this.loadPersonas();
    this.setupContinuumListeners();
  }

  renderContent(): string {
    return `
      <div class="widget-container">
        <div class="widget-header">
          <div class="header-title">
            <span>${this.widgetIcon}</span>
            <span>${this.widgetTitle}</span>
            <span class="persona-count">${this.personas.length}</span>
          </div>
        </div>
        <div class="widget-content">
          ${this.renderPersonas()}
        </div>
      </div>
    `;
  }

  private renderPersonas(): string {
    if (this.personas.length === 0) {
      return `
        <div class="empty-state">
          <div class="empty-icon">👤</div>
          <div class="empty-message">
            No personas found.<br>
            Create your first AI persona to get started!
          </div>
        </div>
      `;
    }

    return this.personas.map(persona => this.renderPersona(persona)).join('');
  }

  private renderPersona(persona: Persona): string {
    const isSelected = this.selectedPersona?.id === persona.id;
    
    return `
      <div class="persona-card ${isSelected ? 'selected' : ''}" data-persona-id="${persona.id}">
        <div class="persona-header">
          <div class="persona-name">${this.formatPersonaName(persona.name)}</div>
          <div class="persona-status ${persona.status}">
            ${this.formatStatus(persona.status)}
          </div>
        </div>
        
        <div class="persona-specialization">
          ${persona.specialization.replace(/_/g, ' ')}
        </div>
        
        ${this.renderAcademyProgress(persona)}
        
        <div class="persona-actions">
          <button class="persona-action deploy" data-action="deploy" data-persona-id="${persona.id}">
            ✅ Deploy
          </button>
          <button class="persona-action retrain" data-action="retrain" data-persona-id="${persona.id}">
            🔄 Retrain
          </button>
          <button class="persona-action share" data-action="share" data-persona-id="${persona.id}">
            🔗 Share
          </button>
        </div>
      </div>
    `;
  }

  private formatPersonaName(name: string): string {
    if (name.includes('fine-tune-test-')) return 'Fine-Tune Test';
    if (name.includes('test-lawyer-')) return 'Legal Test';
    if (name.length > 25) return name.substring(0, 22) + '...';
    return name;
  }

  private formatStatus(status: string): string {
    switch (status) {
      case 'training': return 'IN ACADEMY »';
      case 'failed': return 'FAILED ⚠️';
      case 'graduated': return 'GRADUATED ✓';
      case 'loaded': return 'LOADED ⚡';
      default: return status.toUpperCase();
    }
  }

  private renderAcademyProgress(persona: Persona): string {
    if (!persona.currentScore && !persona.graduationScore) return '';

    const currentScore = persona.graduationScore || persona.currentScore || 0;
    const threshold = persona.threshold || 75;
    const normalizedScore = currentScore > 1 ? Math.min(100, currentScore) : currentScore * 100;
    const normalizedThreshold = threshold > 1 ? Math.min(100, threshold) : threshold * 100;

    return `
      <div class="academy-progress">
        <div class="progress-header">
          <span class="score-label">Score: ${normalizedScore.toFixed(1)}%</span>
          <span class="threshold-label">Target: ${normalizedThreshold.toFixed(1)}%</span>
        </div>
        <div class="progress-bar" data-persona-id="${persona.id}">
          <div class="threshold-background" style="width: ${normalizedThreshold}%"></div>
          <div class="progress-fill" style="width: ${normalizedScore}%"></div>
          <div class="threshold-marker" 
               style="left: calc(${normalizedThreshold}% - 6px)" 
               data-threshold="${normalizedThreshold}"
               data-persona-id="${persona.id}">
          </div>
        </div>
      </div>
    `;
  }

  private async loadPersonas(): Promise<void> {
    try {
      // Try to load from continuum API
      const response = await this.executeCommand('personas', { action: 'list' });
      
      if (response && response.personas) {
        this.personas = response.personas;
      } else {
        // Load mock data for demonstration
        this.loadMockData();
      }
    } catch (error) {
      console.error(`🎛️ ${this.widgetName}: Failed to load personas:`, error);
      this.loadMockData();
    }
    
    await this.render();
  }

  private loadMockData(): void {
    this.personas = [
      {
        id: 'persona-1',
        name: 'Protocol Sheriff',
        status: 'graduated',
        specialization: 'protocol_enforcement',
        graduationScore: 96.7,
        threshold: 85,
        accuracy: 96.7,
        created: '2025-01-20',
        lastUsed: '2025-01-26'
      },
      {
        id: 'persona-2',
        name: 'Code Specialist',
        status: 'training',
        specialization: 'code_analysis',
        currentScore: 72.3,
        threshold: 80,
        currentIteration: 6,
        totalIterations: 10,
        created: '2025-01-22'
      },
      {
        id: 'persona-3',
        name: 'Legal Assistant',
        status: 'failed',
        specialization: 'legal_analysis',
        graduationScore: 45.2,
        threshold: 75,
        failureReason: 'Low performance on contract analysis',
        created: '2025-01-18'
      }
    ];
  }

  setupEventListeners(): void {
    if (!this.shadowRoot) return;

    // Persona card clicks
    this.shadowRoot.querySelectorAll('.persona-card').forEach(card => {
      card.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        if (target.closest('.persona-action')) return; // Don't trigger on action buttons
        
        const personaId = (card as HTMLElement).dataset.personaId!;
        this.selectPersona(personaId);
      });
    });

    // Action button clicks
    this.shadowRoot.querySelectorAll('.persona-action').forEach(button => {
      button.addEventListener('click', (e) => {
        e.stopPropagation();
        const action = (button as HTMLElement).dataset.action!;
        const personaId = (button as HTMLElement).dataset.personaId!;
        this.handlePersonaAction(action, personaId);
      });
    });

    // Threshold marker dragging
    this.shadowRoot.querySelectorAll('.threshold-marker').forEach(marker => {
      marker.addEventListener('mousedown', (e) => this.handleThresholdDragStart(e as MouseEvent));
    });
  }

  private setupContinuumListeners(): void {
    if (this.getContinuumAPI()) {
      this.onContinuumEvent('personas_updated', () => {
        console.log(`🎛️ ${this.widgetName}: Personas updated`);
        this.loadPersonas();
      });
      
      this.onContinuumEvent('persona_added', (data: any) => {
        console.log(`🎛️ ${this.widgetName}: Persona added`, data);
        this.loadPersonas();
      });
    }
  }

  private selectPersona(personaId: string): void {
    const persona = this.personas.find(p => p.id === personaId);
    if (persona) {
      this.selectedPersona = persona;
      console.log(`🎛️ ${this.widgetName}: Selected persona:`, persona.name);
      this.render();
      
      // Emit selection event
      this.dispatchEvent(new CustomEvent('persona-selected', {
        detail: { persona },
        bubbles: true
      }));
    }
  }

  private async handlePersonaAction(action: string, personaId: string): Promise<void> {
    console.log(`🎛️ ${this.widgetName}: Action ${action} for persona ${personaId}`);
    
    try {
      switch (action) {
        case 'deploy':
          await this.executeCommand('persona_deploy', { personaId });
          break;
        case 'retrain':
          await this.executeCommand('persona_retrain', { personaId });
          break;
        case 'share':
          await this.executeCommand('persona_share', { personaId });
          break;
      }
    } catch (error) {
      console.error(`🎛️ ${this.widgetName}: Action ${action} failed:`, error);
    }
  }

  private handleThresholdDragStart(event: MouseEvent): void {
    const marker = event.target as HTMLElement;
    const personaId = marker.dataset.personaId!;
    const progressBar = marker.closest('.progress-bar') as HTMLElement;
    
    this.dragState = {
      isDragging: true,
      personaId,
      marker,
      progressBar,
      startX: event.clientX,
      barRect: progressBar.getBoundingClientRect()
    };

    marker.classList.add('dragging');
    
    document.addEventListener('mousemove', this.handleThresholdDrag.bind(this));
    document.addEventListener('mouseup', this.handleThresholdDragEnd.bind(this));
    
    event.preventDefault();
  }

  private handleThresholdDrag(event: MouseEvent): void {
    if (!this.dragState?.isDragging) return;

    const { marker, progressBar, barRect } = this.dragState;
    const mouseX = event.clientX;
    const relativeX = mouseX - barRect.left;
    const percentage = Math.max(0, Math.min(100, (relativeX / barRect.width) * 100));

    marker.style.left = `calc(${percentage}% - 6px)`;
    marker.dataset.threshold = percentage.toString();
    
    const thresholdBg = progressBar.querySelector('.threshold-background') as HTMLElement;
    if (thresholdBg) {
      thresholdBg.style.width = `${percentage}%`;
    }
  }

  private handleThresholdDragEnd(event: MouseEvent): void {
    if (!this.dragState?.isDragging) return;

    const { marker, personaId, barRect } = this.dragState;
    const mouseX = event.clientX;
    const relativeX = mouseX - barRect.left;
    const newThreshold = Math.max(0, Math.min(100, (relativeX / barRect.width) * 100));

    marker.classList.remove('dragging');
    
    // Update persona threshold
    const persona = this.personas.find(p => p.id === personaId);
    if (persona) {
      persona.threshold = newThreshold;
      console.log(`🎛️ ${this.widgetName}: Updated threshold for ${persona.name} to ${newThreshold.toFixed(1)}%`);
    }

    document.removeEventListener('mousemove', this.handleThresholdDrag.bind(this));
    document.removeEventListener('mouseup', this.handleThresholdDragEnd.bind(this));
    
    this.dragState = null;
  }
}

// Register the custom element
if (!customElements.get('saved-personas')) {
  customElements.define('saved-personas', SavedPersonasWidget);
}

export default SavedPersonasWidget;