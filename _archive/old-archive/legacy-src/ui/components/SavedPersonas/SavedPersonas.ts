/**
 * SavedPersonas Widget - Clean TypeScript Web Component
 * Proper separation of HTML, CSS, and TypeScript
 */

import { BaseWidget } from '../shared/BaseWidget';

interface Persona {
  id: string;
  name: string;
  specialization: string;
  status: 'active' | 'training' | 'graduated' | 'failed';
  accuracy?: number;
  created: string;
  lastUsed?: string;
}

export class SavedPersonasWidget extends BaseWidget {

  private personas: Persona[] = [];
  private selectedPersona: Persona | null = null;

  constructor() {
    super();
    this.widgetName = 'SavedPersonas';
    this.widgetIcon = '👤';
    this.widgetTitle = 'Saved Personas';
  }

  async connectedCallback() {
    await super.connectedCallback();
    await this.loadPersonas();
    this.setupContinuumListeners();
  }


  private setupContinuumListeners(): void {
    if (this.getContinuumAPI()) {
      this.notifySystem('personas_updated', () => {
        console.log('🎛️ SavedPersonas: personas_updated received');
        this.loadPersonas();
      });
      
      this.notifySystem('persona_added', (data: any) => {
        console.log('🎛️ SavedPersonas: persona_added received', data);
        this.loadPersonas();
      });
      
      this.notifySystem('persona_deleted', (data: any) => {
        console.log('🎛️ SavedPersonas: persona_deleted received', data);
        this.loadPersonas();
      });
      
      console.log('🎛️ SavedPersonas: Connected to continuum API');
    } else {
      setTimeout(() => this.setupContinuumListeners(), 1000);
    }
  }

  private async loadPersonas(): Promise<void> {
    try {
      if (!this.isContinuumConnected()) {
        console.log('🎛️ SavedPersonas: Not connected, using mock data');
        this.loadMockData();
        return;
      }

      const response = await this.executeCommand('personas', { action: 'list' });
      
      if (response && response.personas) {
        this.personas = response.personas;
        console.log(`🎛️ SavedPersonas: Loaded ${this.personas.length} personas`);
      } else {
        this.loadMockData();
      }
      
      this.update();
    } catch (error) {
      console.error('🎛️ SavedPersonas: Failed to load personas:', error);
      this.loadMockData();
    }
  }

  private loadMockData(): void {
    this.personas = [
      {
        id: 'sheriff-001',
        name: 'Protocol Sheriff',
        specialization: 'protocol_enforcement',
        status: 'graduated',
        accuracy: 96.7,
        created: '2025-01-20',
        lastUsed: '2025-01-26'
      },
      {
        id: 'coder-001', 
        name: 'Code Specialist',
        specialization: 'code_analysis',
        status: 'active',
        accuracy: 94.2,
        created: '2025-01-22',
        lastUsed: '2025-01-25'
      }
    ];
    this.update();
  }

  renderContent(): string {
    const content = `
      <div class="persona-list">
        ${this.personas.length === 0 ? this.renderEmptyState() : this.personas.map(persona => this.renderPersona(persona)).join('')}
      </div>

      <div class="actions">
        <button class="btn btn-primary" data-action="create">Create Persona</button>
        <button class="btn btn-secondary" data-action="refresh">Refresh</button>
      </div>
    `;

    return this.renderWithCollapseHeader(content);
  }

  private renderPersona(persona: Persona): string {
    const isSelected = this.selectedPersona?.id === persona.id;
    
    return `
      <div class="persona-item ${isSelected ? 'selected' : ''}" data-persona-id="${persona.id}">
        <div class="persona-info">
          <div class="persona-name">${persona.name}</div>
          <div class="persona-details">${persona.specialization}</div>
        </div>
        <div class="persona-status">
          ${persona.accuracy ? `<div class="accuracy">${persona.accuracy}%</div>` : ''}
          <div class="status-badge status-${persona.status}">${persona.status}</div>
        </div>
      </div>
    `;
  }

  private renderEmptyState(): string {
    return `
      <div class="empty-state">
        No saved personas yet.<br>
        Create your first AI persona to get started!
      </div>
    `;
  }

  setupEventListeners(): void {
    // Persona selection
    this.shadowRoot!.querySelectorAll('.persona-item').forEach(item => {
      item.addEventListener('click', (e) => {
        const personaId = (e.currentTarget as HTMLElement).dataset.personaId!;
        this.selectPersona(personaId);
      });
    });

    // Action buttons
    this.shadowRoot!.querySelectorAll('.btn').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const action = (e.currentTarget as HTMLElement).dataset.action!;
        this.handleAction(action);
      });
    });
  }

  private selectPersona(personaId: string): void {
    const persona = this.personas.find(p => p.id === personaId);
    if (persona) {
      this.selectedPersona = persona;
      console.log('🎛️ SavedPersonas: Selected persona:', persona.name);
      
      this.sendMessage({
        type: 'persona_selected',
        persona: persona
      });
      
      this.update();
    }
  }

  private async handleAction(action: string): Promise<void> {
    switch (action) {
      case 'create':
        console.log('🎛️ SavedPersonas: Creating new persona...');
        try {
          await this.executeCommand('personas', { action: 'create' });
        } catch (error) {
          console.error('🎛️ SavedPersonas: Failed to create persona:', error);
        }
        break;
        
      case 'refresh':
        console.log('🎛️ SavedPersonas: Refreshing personas...');
        await this.loadPersonas();
        break;
        
      default:
        console.log('🎛️ SavedPersonas: Unknown action:', action);
    }
  }
}

// Register the custom element
if (!customElements.get('saved-personas')) {
  customElements.define('saved-personas', SavedPersonasWidget);
}