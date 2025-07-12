/**
 * Example: PersonasWidget - Strongly typed server-connected widget
 * Shows how TypeScript + shared types + spread operators = elegant, mistake-proof code
 */

import { BaseWidget } from '../shared/BaseWidget';
import { 
  DataSourceType, 
  DataUpdatedEvent, 
  WidgetCapabilities,
  SessionCreatedEvent
} from '../../../types/shared/WidgetServerTypes';

interface PersonaData {
  readonly id: string;
  readonly name: string;
  readonly type: 'stock' | 'custom';
  readonly capabilities: readonly string[];
  readonly createdAt: string;
  readonly lastUsed?: string;
}

export class PersonasWidget extends BaseWidget {
  private personas: PersonaData[] = [];
  private selectedPersonaId: string | null = null;

  protected override getWidgetCapabilities(): WidgetCapabilities {
    return {
      canFetchData: ['personas'], // TypeScript enforces DataSourceType
      canExecuteCommands: ['persona-create', 'persona-delete', 'persona-activate'],
      respondsToEvents: ['session:created', 'data:updated'], // Type-safe event names
      supportsExport: ['json', 'csv'], // Type-safe export formats
      requiresAuth: false,
      updateFrequency: 'realtime' // Auto-refresh when personas change
    };
  }

  protected override async initializeWidget(): Promise<void> {
    await super.initializeWidget(); // Sets up typed event listeners automatically
    
    // Elegant spread pattern - merge default filters with custom options
    this.fetchServerData('personas', {
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
      // Type assertion with validation - linter ensures this pattern
      this.personas = Array.isArray(data) ? data as PersonaData[] : [];
      this.update(); // Re-render with new data
    }
  }

  // TypeScript enforces correct event type - no more 'any' mistakes
  protected override shouldAutoRefreshOnDataUpdate(event: DataUpdatedEvent): boolean {
    return event.dataSource === 'personas' && event.updateType !== 'deleted';
  }

  // Type-safe session events - linter prevents property access mistakes
  protected override onServerSessionCreated(event: SessionCreatedEvent): void {
    console.log(`New ${event.sessionType} session by ${event.owner} - refreshing personas`);
    this.fetchServerData('personas'); // Type-safe call
  }

  // Widget-specific functionality with type safety
  private createPersona(name: string, type: 'stock' | 'custom'): void {
    // Elegant spread pattern - merge defaults with custom params
    this.executeServerCommand('persona-create', {
      params: { name, type },
      timeout: 10000,
      priority: 'normal' // TypeScript enforces valid priority values
    });
  }

  private activatePersona(personaId: string): void {
    this.selectedPersonaId = personaId;
    
    // Type-safe command with spread pattern
    this.executeServerCommand('persona-activate', {
      params: { personaId },
      ...this.getWidgetCapabilities().requiresAuth && { auth: true } // Conditional spread
    });
  }

  // Type-safe command result processing
  protected override processCommandResult(command: string, result: unknown): void {
    switch (command) {
      case 'persona-create':
        console.log('Persona created:', result);
        this.fetchServerData('personas'); // Refresh list
        break;
      case 'persona-activate':
        console.log('Persona activated:', result);
        this.triggerRefresh({ preserveState: true }); // Use inherited server controls
        break;
    }
  }

  // Type-safe error handling with data source context  
  protected override onDataFetchError(dataSource: DataSourceType, error: string): void {
    if (dataSource === 'personas') {
      this.personas = []; // Fallback to empty state
      this.renderError(new Error(`Failed to load personas: ${error}`));
    }
  }

  protected override renderOwnContent(): string {
    // TypeScript ensures personas is PersonaData[] - no runtime surprises
    return `
      <div class="personas-container">
        <div class="personas-header">
          <h3>Personas (${this.personas.length})</h3>
          <button class="create-persona-btn">Create New</button>
        </div>
        <div class="personas-list">
          ${this.personas.map(persona => `
            <div class="persona-item ${this.selectedPersonaId === persona.id ? 'selected' : ''}"
                 data-persona-id="${persona.id}">
              <span class="persona-name">${persona.name}</span>
              <span class="persona-type">${persona.type}</span>
              <span class="persona-capabilities">${persona.capabilities.length} capabilities</span>
            </div>
          `).join('')}
        </div>
      </div>
    `;
  }

  protected override setupEventListeners(): void {
    super.setupEventListeners(); // Set up server event listeners

    // Type-safe DOM event handling
    this.shadowRoot.addEventListener('click', (event) => {
      const target = event.target as HTMLElement;
      
      if (target.classList.contains('create-persona-btn')) {
        this.createPersona('New Persona', 'custom');
      }
      
      if (target.classList.contains('persona-item')) {
        const personaId = target.dataset.personaId;
        if (personaId) this.activatePersona(personaId);
      }
    });
  }
}

// Type-safe widget registration - linter prevents property mistakes
customElements.define('personas-widget', PersonasWidget);