/**
 * SavedPersonas Widget - Modern TypeScript Web Component
 * Self-contained with embedded styles, full functionality, and tests
 */

import { BaseWidget } from '../shared/BaseWidget';
import { 
  DataSourceType, 
  DataUpdatedEvent, 
  WidgetCapabilities,
  SessionCreatedEvent,
  PersonaData,
  PersonaAction,
  PersonaValidation
} from '../../../types/shared/WidgetServerTypes';

export class SavedPersonasWidget extends BaseWidget {
  private personas: PersonaData[] = [];
  private selectedPersona: PersonaData | null = null;
  private dragState: any = null;

  constructor() {
    super();
    this.widgetName = 'SavedPersonas';
    this.widgetIcon = '👤';
    this.widgetTitle = 'Saved Personas';
  }

  protected override getWidgetCapabilities(): WidgetCapabilities {
    return {
      canFetchData: ['personas'], // Uses dynamic discovery for personas data
      canExecuteCommands: ['persona_deploy', 'persona_retrain', 'persona_share', 'persona_delete', 'persona_update_threshold'], 
      respondsToEvents: ['session:created', 'data:updated'], // Type-safe event names
      supportsExport: ['json'], // Personas can be exported as JSON
      requiresAuth: false,
      updateFrequency: 'realtime' // Auto-refresh when personas change
    };
  }

  protected override async initializeWidget(): Promise<void> {
    await super.initializeWidget(); // Sets up typed event listeners automatically
    
    // Use dynamic discovery to fetch personas data with proper command parameters
    this.fetchServerData('personas', {
      params: {
        action: 'list'
      },
      filters: {
        limit: 50,
        sortBy: 'lastUsed',
        sortOrder: 'desc'
      }
    });
  }

  // TypeScript prevents mistakes - must use DataSourceType, gets typed data
  protected override processServerData(dataSource: DataSourceType, data: unknown): void {
    if (dataSource === 'personas') {
      // Use shared validation logic - works on both server and client
      if (Array.isArray(data)) {
        this.personas = data.filter(PersonaValidation.validatePersonaData);
        console.log(`🎛️ ${this.widgetName}: Loaded ${this.personas.length} validated personas via dynamic discovery`);
      } else {
        this.personas = [];
        console.warn(`🎛️ ${this.widgetName}: Invalid personas data received:`, data);
      }
      this.update(); // Re-render with new data
    }
  }

  // TypeScript enforces correct event type - no more 'any' mistakes
  protected override shouldAutoRefreshOnDataUpdate(event: DataUpdatedEvent): boolean {
    return event.dataSource === 'personas' && event.updateType !== 'deleted';
  }

  // Type-safe session events - linter prevents property access mistakes
  protected override onServerSessionCreated(event: SessionCreatedEvent): void {
    console.log(`🎛️ ${this.widgetName}: New ${event.sessionType} session by ${event.owner} - refreshing personas`);
    this.fetchServerData('personas'); // Type-safe call
  }

  // Type-safe error handling with data source context  
  protected override onDataFetchError(dataSource: DataSourceType, error: string): void {
    if (dataSource === 'personas') {
      this.personas = []; // Fallback to empty state
      console.error(`🎛️ ${this.widgetName}: Failed to load personas: ${error}`);
      this.loadMockData(); // Fallback to mock data for demo
      this.update();
    }
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

  private renderPersona(persona: PersonaData): string {
    const isSelected = this.selectedPersona?.id === persona.id;
    
    return `
      <div class="persona-card ${isSelected ? 'selected' : ''}" data-persona-id="${persona.id}">
        <div class="persona-header">
          <div class="persona-name">${PersonaValidation.getPersonaDisplayName(persona)}</div>
          <div class="persona-status ${persona.status}">
            ${PersonaValidation.getPersonaStatusDisplay(persona.status)}
          </div>
        </div>
        
        <div class="persona-specialization">
          ${PersonaValidation.getPersonaSpecializationDisplay(persona.specialization)}
        </div>
        
        ${this.renderAcademyProgress(persona)}
        
        <div class="persona-actions">
          ${this.renderPersonaActions(persona)}
        </div>
      </div>
    `;
  }

  // Dynamic action rendering based on shared business logic
  private renderPersonaActions(persona: PersonaData): string {
    const availableActions = PersonaValidation.getAvailableActions(persona);
    
    const actionLabels: Record<PersonaAction, string> = {
      deploy: '✅ Deploy',
      retrain: '🔄 Retrain', 
      share: '🔗 Share',
      delete: '🗑️ Delete',
      export: '💾 Export'
    };

    return availableActions
      .map(action => `
        <button class="persona-action ${action}" 
                data-action="${action}" 
                data-persona-id="${persona.id}">
          ${actionLabels[action]}
        </button>
      `)
      .join('');
  }

  // Removed formatPersonaName and formatStatus - using shared PersonaValidation methods instead

  private renderAcademyProgress(persona: PersonaData): string {
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

  // Removed loadPersonas - now using dynamic discovery via fetchServerData in initializeWidget

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
        const action = (button as HTMLElement).dataset.action! as PersonaAction;
        const personaId = (button as HTMLElement).dataset.personaId!;
        this.handlePersonaAction(action, personaId);
      });
    });

    // Threshold marker dragging
    this.shadowRoot.querySelectorAll('.threshold-marker').forEach(marker => {
      marker.addEventListener('mousedown', (e) => this.handleThresholdDragStart(e as MouseEvent));
    });
  }

  // Removed setupContinuumListeners - now using typed event handlers from BaseWidget

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

  // Type-safe command execution with strongly-typed actions and elegant spread pattern
  private async handlePersonaAction(action: PersonaAction, personaId: string): Promise<void> {
    console.log(`🎛️ ${this.widgetName}: Action ${action} for persona ${personaId}`);
    
    try {
      const baseParams = { 
        params: { personaId },
        timeout: 15000,
        priority: 'normal' as const
      };

      // TypeScript ensures only valid PersonaAction values can be passed
      switch (action) {
        case 'deploy':
          this.executeServerCommand('persona_deploy', baseParams);
          break;
        case 'retrain':
          this.executeServerCommand('persona_retrain', { 
            ...baseParams,
            timeout: 30000 // Retrain takes longer
          });
          break;
        case 'share':
          this.executeServerCommand('persona_share', baseParams);
          break;
        case 'delete':
          this.executeServerCommand('persona_delete', {
            ...baseParams,
            priority: 'high' // Delete operations should be prioritized
          });
          break;
        case 'export':
          this.triggerExport('json', { personaId }); // Use inherited server controls
          break;
      }
    } catch (error) {
      console.error(`🎛️ ${this.widgetName}: Action ${action} failed:`, error);
    }
  }

  // Type-safe command result processing
  protected override processCommandResult(command: string, result: unknown): void {
    switch (command) {
      case 'persona_deploy':
        console.log(`🎛️ ${this.widgetName}: Persona deployed:`, result);
        this.fetchServerData('personas'); // Refresh list
        break;
      case 'persona_retrain':
        console.log(`🎛️ ${this.widgetName}: Persona retraining started:`, result);
        this.triggerRefresh({ preserveState: true }); // Use inherited server controls
        break;
      case 'persona_share':
        console.log(`🎛️ ${this.widgetName}: Persona shared:`, result);
        break;
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
    
    // Update persona threshold via server command (maintains data consistency)
    const persona = this.personas.find(p => p.id === personaId);
    if (persona) {
      console.log(`🎛️ ${this.widgetName}: Updating threshold for ${PersonaValidation.getPersonaDisplayName(persona)} to ${newThreshold.toFixed(1)}%`);
      
      // Use server command to maintain shared state
      this.executeServerCommand('persona_update_threshold', {
        params: { 
          personaId, 
          threshold: newThreshold 
        },
        timeout: 5000,
        priority: 'normal' as const
      });
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