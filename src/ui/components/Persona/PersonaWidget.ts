/**
 * Persona Widget - Individual AI Persona Component
 * Represents a single AI persona with its capabilities and status
 */

import { BaseWidget } from '../shared/BaseWidget';

interface PersonaConfig {
  id: string;
  name: string;
  specialization: string;
  status: 'active' | 'training' | 'graduated' | 'offline';
  avatar: string;
  accuracy?: number;
  description: string;
  capabilities: string[];
  lastActive?: Date;
}

export class PersonaWidget extends BaseWidget {

  private config: PersonaConfig | null = null;
  private isInteracting: boolean = false;

  constructor() {
    super();
    this.widgetName = 'Persona';
    this.widgetIcon = '🤖';
    this.widgetTitle = 'AI Persona';
  }

  // Public method to configure the persona
  public setPersona(config: PersonaConfig): void {
    this.config = config;
    this.widgetTitle = config.name;
    this.render();
  }


  renderContent(): string {
    if (!this.config) {
      return `
        <div class="persona-header">
          <div class="persona-avatar">❓</div>
          <div class="persona-info">
            <div class="persona-name">No Persona Configured</div>
          </div>
        </div>
        <div class="persona-description">
          Use setPersona() to configure this component.
        </div>
      `;
    }

    const lastActiveStr = this.config.lastActive 
      ? this.config.lastActive.toLocaleDateString()
      : 'Never';

    return `
      ${this.isInteracting ? '<div class="interaction-indicator"></div>' : ''}
      
      <div class="persona-header">
        <div class="persona-avatar">${this.config.avatar}</div>
        <div class="persona-info">
          <div class="persona-name">${this.config.name}</div>
          <div class="persona-status status-${this.config.status}">${this.config.status}</div>
        </div>
      </div>

      <div class="persona-specialization">${this.config.specialization}</div>
      
      <div class="persona-description">${this.config.description}</div>

      <div class="persona-capabilities">
        ${this.config.capabilities.map(cap => `<span class="capability-tag">${cap}</span>`).join('')}
      </div>

      <div class="persona-metrics">
        ${this.config.accuracy ? `<span class="accuracy">${this.config.accuracy}% accuracy</span>` : '<span></span>'}
        <span class="last-active">Last: ${lastActiveStr}</span>
      </div>
    `;
  }

  setupEventListeners(): void {
    this.addEventListener('click', () => {
      if (this.config) {
        console.log(`🤖 Persona: Clicked on ${this.config.name}`);
        this.handlePersonaClick();
      }
    });
  }

  private handlePersonaClick(): void {
    if (!this.config) return;

    // Send persona selection event
    this.sendMessage({
      type: 'persona_selected',
      personaId: this.config.id,
      persona: this.config
    });

    // Show interaction indicator
    this.setInteracting(true);
    setTimeout(() => this.setInteracting(false), 2000);
  }

  public setInteracting(isInteracting: boolean): void {
    if (this.isInteracting !== isInteracting) {
      this.isInteracting = isInteracting;
      this.render();
    }
  }

  // Static method to create persona from data
  static create(config: PersonaConfig): PersonaWidget {
    const widget = new PersonaWidget();
    widget.setPersona(config);
    return widget;
  }
}

// Register the custom element - only register once per constructor
if (!customElements.get('persona-widget')) {
  customElements.define('persona-widget', PersonaWidget);
  console.log('✅ PersonaWidget: Registered as persona-widget');
}