/**
 * Interactive Persona Component
 * Personas become clickable, responsive UI elements that provide real-time feedback
 */

import { BaseWidget } from './BaseWidget.js';

export interface PersonaState {
  id: string;
  name: string;
  status: 'active' | 'thinking' | 'working' | 'idle' | 'offline';
  avatar: string;
  currentTask?: string;
  mood?: 'helpful' | 'focused' | 'creative' | 'analytical' | 'frustrated';
  responseTime?: number;
}

export class InteractivePersona extends BaseWidget {
  private persona: PersonaState;
  private isInteracting: boolean = false;


  constructor(personaData: PersonaState) {
    super();
    this.persona = personaData;
    this.widgetName = `InteractivePersona-${personaData.name}`;
    this.widgetIcon = personaData.avatar;
    this.widgetTitle = personaData.name;
  }

  protected async initializeWidget(): Promise<void> {
    this.setupPersonaEventListeners();
    this.connectToPersonaStream();
  }

  renderContent(): string {
    const statusEmoji = this.getStatusEmoji();
    const moodColor = this.getMoodColor();
    
    return `
      <div class="persona-container ${this.persona.status}" data-persona-id="${this.persona.id}">
        <div class="persona-avatar-section">
          <div class="persona-avatar" style="border-color: ${moodColor}">
            ${this.persona.avatar}
            <div class="status-indicator ${this.persona.status}">
              ${statusEmoji}
            </div>
          </div>
          <div class="persona-pulse ${this.isInteracting ? 'active' : ''}"></div>
        </div>
        
        <div class="persona-info">
          <div class="persona-name-row">
            <span class="persona-name">${this.persona.name}</span>
            <span class="persona-status-text">${this.getStatusText()}</span>
          </div>
          
          ${this.persona.currentTask ? `
            <div class="persona-current-task">
              <span class="task-icon">⚡</span>
              <span class="task-text">${this.persona.currentTask}</span>
            </div>
          ` : ''}
          
          <div class="persona-interaction-area">
            <button class="persona-action-btn" data-action="ask">
              💬 Ask ${this.persona.name}
            </button>
            <button class="persona-action-btn" data-action="assign">
              📋 Assign Task
            </button>
            <button class="persona-action-btn" data-action="feedback">
              👍 Give Feedback
            </button>
          </div>
        </div>
        
        <div class="persona-feedback-area" style="display: none;">
          <div class="feedback-input">
            <textarea placeholder="Tell ${this.persona.name} what you think..." class="feedback-text"></textarea>
            <div class="feedback-actions">
              <button class="send-feedback">Send</button>
              <button class="cancel-feedback">Cancel</button>
            </div>
          </div>
        </div>
      </div>
    `;
  }

  setupEventListeners(): void {
    // Persona click interactions
    this.shadowRoot?.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      const action = target.dataset.action;
      
      if (action) {
        this.handlePersonaAction(action, e);
      }
    });

    // Real-time persona feedback
    this.addEventListener('persona:response', this.handlePersonaResponse.bind(this));
    this.addEventListener('persona:status-change', this.handleStatusChange.bind(this));
  }

  private setupPersonaEventListeners(): void {
    // Listen for user interactions that personas should know about
    document.addEventListener('widget:screenshot', (e) => {
      this.notifyPersona('user-screenshot', e.detail);
    });

    document.addEventListener('widget:refresh', (e) => {
      this.notifyPersona('user-refresh', e.detail);
    });

    // Listen for menu clicks, navigation, etc.
    document.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      if (target.classList.contains('room-tab') || target.classList.contains('menu-item')) {
        this.notifyPersona('user-navigation', {
          element: target.tagName,
          text: target.textContent,
          timestamp: Date.now()
        });
      }
    });
  }

  private async handlePersonaAction(action: string, event: Event): Promise<void> {
    this.isInteracting = true;
    this.render();

    try {
      switch (action) {
        case 'ask':
          await this.handleAskPersona(event);
          break;
        case 'assign':
          await this.handleAssignTask(event);
          break;
        case 'feedback':
          await this.handleFeedbackMode(event);
          break;
      }
    } finally {
      this.isInteracting = false;
      this.render();
    }
  }

  private async handleAskPersona(_event: Event): Promise<void> {
    // Trigger server control to start conversation with specific persona
    this.dispatchEvent(new CustomEvent('persona:start-conversation', {
      detail: {
        personaId: this.persona.id,
        personaName: this.persona.name,
        context: 'direct-ask',
        timestamp: Date.now()
      },
      bubbles: true
    }));

    // Visual feedback
    this.showPersonaFeedback(`💬 Starting conversation with ${this.persona.name}...`);
  }

  private async handleAssignTask(_event: Event): Promise<void> {
    // Open task assignment interface
    this.dispatchEvent(new CustomEvent('persona:assign-task', {
      detail: {
        personaId: this.persona.id,
        personaName: this.persona.name,
        capabilities: await this.getPersonaCapabilities()
      },
      bubbles: true
    }));

    this.showPersonaFeedback(`📋 Task assignment mode for ${this.persona.name}`);
  }

  private async handleFeedbackMode(_event: Event): Promise<void> {
    // Show feedback input area
    const feedbackArea = this.shadowRoot?.querySelector('.persona-feedback-area') as HTMLElement;
    if (feedbackArea) {
      feedbackArea.style.display = 'block';
      
      // Setup feedback submission
      const sendBtn = feedbackArea.querySelector('.send-feedback');
      const cancelBtn = feedbackArea.querySelector('.cancel-feedback');
      const textArea = feedbackArea.querySelector('.feedback-text') as HTMLTextAreaElement;

      sendBtn?.addEventListener('click', async () => {
        const feedback = textArea.value.trim();
        if (feedback) {
          await this.sendPersonaFeedback(feedback);
          feedbackArea.style.display = 'none';
          textArea.value = '';
        }
      });

      cancelBtn?.addEventListener('click', () => {
        feedbackArea.style.display = 'none';
        textArea.value = '';
      });

      textArea.focus();
    }
  }

  private async sendPersonaFeedback(feedback: string): Promise<void> {
    // Send feedback directly to persona via server control
    this.dispatchEvent(new CustomEvent('persona:receive-feedback', {
      detail: {
        personaId: this.persona.id,
        feedback: feedback,
        context: 'direct-feedback',
        timestamp: Date.now(),
        source: 'user-interface'
      },
      bubbles: true
    }));

    this.showPersonaFeedback(`👍 Feedback sent to ${this.persona.name}!`);
  }

  private notifyPersona(eventType: string, eventData: any): void {
    // Notify persona of user actions in real-time
    this.dispatchEvent(new CustomEvent('persona:observe-action', {
      detail: {
        personaId: this.persona.id,
        eventType: eventType,
        eventData: eventData,
        timestamp: Date.now()
      },
      bubbles: true
    }));
  }

  private handlePersonaResponse(event: Event): void {
    const customEvent = event as CustomEvent;
    // Handle real-time responses from persona
    const { response, type } = customEvent.detail;
    
    if (type === 'thinking') {
      this.persona.status = 'thinking';
      this.persona.currentTask = 'Processing your request...';
    } else if (type === 'response') {
      this.showPersonaFeedback(response);
    }
    
    this.render();
  }

  private handleStatusChange(event: CustomEvent): void {
    // Real-time persona status updates
    const { status, task, mood } = event.detail;
    
    this.persona.status = status;
    this.persona.currentTask = task;
    this.persona.mood = mood;
    
    this.render();
  }

  private connectToPersonaStream(): void {
    // Connect to real-time persona updates via WebSocket
    const continuum = (window as any).continuum;
    if (continuum) {
      continuum.on(`persona:${this.persona.id}:status`, (data: any) => {
        this.handleStatusChange(new CustomEvent('persona:status-change', { detail: data }));
      });

      continuum.on(`persona:${this.persona.id}:response`, (data: any) => {
        this.handlePersonaResponse(new CustomEvent('persona:response', { detail: data }));
      });
    }
  }

  private async getPersonaCapabilities(): Promise<string[]> {
    // Query persona's current capabilities
    try {
      const continuum = (window as any).continuum;
      const result = await continuum.execute('persona:get-capabilities', {
        personaId: this.persona.id
      });
      return result?.capabilities || [];
    } catch (error) {
      return ['general-assistance', 'code-review', 'documentation'];
    }
  }

  private showPersonaFeedback(message: string): void {
    // Show temporary feedback message
    console.log(`🤖 ${this.persona.name}: ${message}`);
    
    // Could also show toast notification or update UI
    this.dispatchEvent(new CustomEvent('show-notification', {
      detail: {
        type: 'persona-feedback',
        message: message,
        personaName: this.persona.name,
        avatar: this.persona.avatar
      },
      bubbles: true
    }));
  }

  private getStatusEmoji(): string {
    switch (this.persona.status) {
      case 'active': return '🟢';
      case 'thinking': return '🤔';
      case 'working': return '⚡';
      case 'idle': return '😴';
      case 'offline': return '⚫';
      default: return '🔵';
    }
  }

  private getStatusText(): string {
    switch (this.persona.status) {
      case 'active': return 'Ready to help';
      case 'thinking': return 'Thinking...';
      case 'working': return 'Working on task';
      case 'idle': return 'Available';
      case 'offline': return 'Offline';
      default: return 'Unknown';
    }
  }

  private getMoodColor(): string {
    switch (this.persona.mood) {
      case 'helpful': return '#4CAF50';
      case 'focused': return '#2196F3';
      case 'creative': return '#FF9800';
      case 'analytical': return '#9C27B0';
      case 'frustrated': return '#F44336';
      default: return '#757575';
    }
  }
}

// Auto-register the component
if (!customElements.get('interactive-persona')) {
  customElements.define('interactive-persona', InteractivePersona);
}