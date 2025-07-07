/**
 * Academy Sidebar Widget - Academy personas and room navigation
 * 
 * Web Component following new declarative architecture:
 * - Separated HTML template (AcademySidebarWidget.html)
 * - Separated CSS styles (AcademySidebarWidget.css) 
 * - Pure TypeScript logic (no inline HTML/CSS)
 * - Discoverable module via package.json
 * - Works alongside regular ChatWidget
 */

import { BaseWidget } from '../shared/BaseWidget.js';

interface AcademyPersona {
  readonly id: string;
  readonly name: string;
  readonly type: 'user' | 'ai_persona' | 'system';
  readonly role?: 'planner' | 'formula_master' | 'synthesis_engine' | 'genome_discoverer';
  readonly avatar: string;
  readonly domains: readonly string[];
  readonly status: 'online' | 'working' | 'offline';
  readonly capabilities: readonly string[];
}

interface AcademyRoom {
  readonly id: string;
  readonly name: string;
  readonly icon: string;
  readonly type: 'general' | 'academy' | 'synthesis' | 'discovery';
  readonly description: string;
}

export class AcademySidebarWidget extends BaseWidget {
  private readonly activePersonas: Map<string, AcademyPersona> = new Map();
  private readonly academyRooms: AcademyRoom[] = [];
  private currentRoomId: string = 'general';
  private personaListElement!: HTMLElement;
  private roomListElement!: HTMLElement;

  static getBasePath(): string {
    return '/src/ui/components/Academy';
  }

  constructor() {
    super();
    this.widgetName = 'Academy Sidebar';
    this.widgetIcon = '🧬';
    this.widgetTitle = 'Academy Navigation';
    
    this.initializeAcademyRooms();
    this.initializeDefaultPersonas();
  }

  renderContent(): string {
    // Content comes from AcademySidebarWidget.html template
    return '';
  }

  async initializeWidget(): Promise<void> {
    const htmlContent = await this.loadHTMLTemplates();
    if (htmlContent) {
      console.log('Academy Sidebar Widget: HTML template loaded');
    } else {
      console.warn('Academy Sidebar Widget: HTML template not found, using fallback');
    }
  }

  async render(): Promise<void> {
    try {
      const css = await this.loadCSS();
      const html = await this.loadHTMLTemplates();
      
      this.shadowRoot.innerHTML = `
        <style>${css}</style>
        ${html}
      `;

      this.setupEventListeners();
      
    } catch (error) {
      console.error('Academy Sidebar Widget: Render failed:', error);
      this.renderError(error);
    }
  }

  setupEventListeners(): void {
    this.personaListElement = this.shadowRoot.querySelector('#persona-list') as HTMLElement;
    this.roomListElement = this.shadowRoot.querySelector('#room-list') as HTMLElement;

    if (!this.personaListElement || !this.roomListElement) {
      console.error('Academy Sidebar Widget: Required DOM elements not found');
      return;
    }

    this.setupRoomNavigation();
    this.setupPersonaInteractions();
    this.setupSessionInfo();
    
    // Initialize UI state
    this.renderPersonaList();
    this.renderRoomList();
    
    // Connect to Academy backend
    this.connectToAcademy();
  }

  private setupRoomNavigation(): void {
    if (!this.roomListElement) return;

    this.roomListElement.addEventListener('click', (e) => {
      const roomItem = (e.target as HTMLElement).closest('.room-item');
      if (roomItem) {
        const roomId = roomItem.getAttribute('data-room-id');
        if (roomId) {
          this.switchRoom(roomId);
        }
      }
    });
  }

  private setupPersonaInteractions(): void {
    if (!this.personaListElement) return;

    this.personaListElement.addEventListener('click', (e) => {
      const personaItem = (e.target as HTMLElement).closest('.persona-item');
      if (personaItem) {
        const personaId = personaItem.getAttribute('data-persona-id');
        if (personaId) {
          this.interactWithPersona(personaId);
        }
      }
    });
  }

  private setupSessionInfo(): void {
    const sessionInfo = this.shadowRoot.querySelector('.session-costs');
    if (sessionInfo) {
      // Update session costs periodically
      this.updateSessionCosts();
      setInterval(() => this.updateSessionCosts(), 30000); // Every 30 seconds
    }
  }

  private async switchRoom(roomId: string): Promise<void> {
    if (this.currentRoomId === roomId) return;

    // Update UI active state
    this.shadowRoot.querySelectorAll('.room-item').forEach(item => item.classList.remove('active'));
    this.shadowRoot.querySelector(`[data-room-id="${roomId}"]`)?.classList.add('active');
    
    this.currentRoomId = roomId;

    // Notify ChatWidget about room change
    const roomChangeEvent = new CustomEvent('continuum:room-changed', {
      detail: { 
        room: this.academyRooms.find(r => r.id === roomId),
        source: 'academy-sidebar'
      },
      bubbles: true
    });
    document.dispatchEvent(roomChangeEvent);

    console.log(`Academy Sidebar: Switched to room ${roomId}`);
  }

  private async interactWithPersona(personaId: string): Promise<void> {
    const persona = this.activePersonas.get(personaId);
    if (!persona) return;

    // Focus chat on this persona
    const personaFocusEvent = new CustomEvent('continuum:persona-focus', {
      detail: { 
        persona,
        source: 'academy-sidebar'
      },
      bubbles: true
    });
    document.dispatchEvent(personaFocusEvent);

    console.log(`Academy Sidebar: Focused on persona ${persona.name}`);
  }

  private async renderPersonaList(): Promise<void> {
    if (!this.personaListElement) return;
    
    this.personaListElement.innerHTML = '';
    
    for (const persona of Array.from(this.activePersonas.values())) {
      const personaElement = document.createElement('div');
      personaElement.className = `persona-item ${persona.status}`;
      personaElement.setAttribute('data-persona-id', persona.id);
      
      personaElement.innerHTML = `
        <div class="persona-avatar">${persona.avatar}</div>
        <div class="persona-info">
          <div class="persona-name">${persona.name}</div>
          <div class="persona-role">${persona.role || persona.type}</div>
          <div class="persona-domains">${persona.domains.slice(0, 2).join(', ')}</div>
        </div>
        <div class="persona-status ${persona.status}"></div>
      `;
      
      this.personaListElement.appendChild(personaElement);
    }
  }

  private async renderRoomList(): Promise<void> {
    if (!this.roomListElement) return;
    
    this.roomListElement.innerHTML = '';
    
    for (const room of this.academyRooms) {
      const roomElement = document.createElement('div');
      roomElement.className = `room-item ${room.id === this.currentRoomId ? 'active' : ''}`;
      roomElement.setAttribute('data-room-id', room.id);
      roomElement.title = room.description;
      
      roomElement.innerHTML = `
        <span class="room-icon">${room.icon}</span>
        <span class="room-name">${room.name}</span>
      `;
      
      this.roomListElement.appendChild(roomElement);
    }
  }

  private async updateSessionCosts(): Promise<void> {
    try {
      const result = await this.executeCommand('session.getCosts', {});
      
      const costAmount = this.shadowRoot.querySelector('.cost-amount');
      const costDetails = this.shadowRoot.querySelector('.cost-details');
      
      if (costAmount && costDetails && result) {
        costAmount.textContent = `$${result.totalCost.toFixed(4)}`;
        costDetails.textContent = `${result.requestCount} Requests • Cost`;
      }
    } catch (error) {
      console.warn('Academy Sidebar: Failed to update session costs:', error);
    }
  }

  private initializeAcademyRooms(): void {
    this.academyRooms.push(
      {
        id: 'general',
        name: 'General Chat',
        icon: '#',
        type: 'general',
        description: 'General conversation and AI assistance'
      },
      {
        id: 'academy',
        name: 'Academy',
        icon: '🧪',
        type: 'academy',
        description: 'AI training and evolution workspace'
      },
      {
        id: 'synthesis',
        name: 'Synthesis Lab',
        icon: '🧬',
        type: 'synthesis',
        description: 'Multi-domain capability synthesis'
      },
      {
        id: 'discovery',
        name: 'Genome Discovery',
        icon: '🔍',
        type: 'discovery',
        description: 'P2P genome search and assembly'
      }
    );
  }

  private initializeDefaultPersonas(): void {
    const personas: AcademyPersona[] = [
      {
        id: 'planner_ai',
        name: 'PlannerAI',
        type: 'ai_persona',
        role: 'planner',
        avatar: '📋',
        domains: ['planning', 'coordination'],
        status: 'online',
        capabilities: ['task_planning', 'resource_coordination', 'academy_orchestration']
      },
      {
        id: 'formula_master',
        name: 'FormulaMaster',
        type: 'ai_persona',
        role: 'formula_master',
        avatar: '🧙‍♂️',
        domains: ['optimization', 'training_formulas'],
        status: 'online',
        capabilities: ['formula_generation', 'training_optimization', 'mathematical_analysis']
      },
      {
        id: 'synthesis_engine',
        name: 'SynthesisEngine',
        type: 'ai_persona',
        role: 'synthesis_engine',
        avatar: '🧬',
        domains: ['capability_synthesis', 'multi_domain_integration'],
        status: 'online',
        capabilities: ['capability_composition', 'lora_integration', 'performance_estimation']
      },
      {
        id: 'genome_discoverer',
        name: 'GenomeDiscoverer',
        type: 'ai_persona',
        role: 'genome_discoverer',
        avatar: '🔍',
        domains: ['p2p_discovery', 'genome_analysis'],
        status: 'online',
        capabilities: ['p2p_search', 'genome_validation', 'compatibility_analysis']
      }
    ];

    personas.forEach(persona => this.activePersonas.set(persona.id, persona));
  }

  private async connectToAcademy(): Promise<void> {
    try {
      await this.executeCommand('academy.connect', {});
      console.log('Academy Sidebar Widget: Connected to Academy backend successfully');
      
      // Update status indicator
      const statusIndicator = this.shadowRoot.querySelector('.status-indicator');
      const statusText = this.shadowRoot.querySelector('.status-text');
      if (statusIndicator && statusText) {
        statusIndicator.className = 'status-indicator online';
        statusText.textContent = 'Academy Ready';
      }
    } catch (error) {
      console.warn('Academy Sidebar Widget: Failed to connect to Academy backend:', error);
      
      // Update status indicator
      const statusIndicator = this.shadowRoot.querySelector('.status-indicator');
      const statusText = this.shadowRoot.querySelector('.status-text');
      if (statusIndicator && statusText) {
        statusIndicator.className = 'status-indicator offline';
        statusText.textContent = 'Academy Offline';
      }
    }
  }
}

// Register as web component
customElements.define('academy-sidebar-widget', AcademySidebarWidget);