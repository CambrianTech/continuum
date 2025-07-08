/**
 * Academy Chat Widget - Discord-like interface for Academy personas
 * 
 * Web Component following new declarative architecture:
 * - Separated HTML template (AcademyChatWidget.html)
 * - Separated CSS styles (AcademyChatWidget.css) 
 * - Pure TypeScript logic (no inline HTML/CSS)
 * - Discoverable module via package.json
 * - Drop-in compatibility with continuum render daemon
 */

import { BaseWidget } from '../shared/BaseWidget';

interface AcademyMessage {
  readonly id: string;
  readonly timestamp: Date;
  readonly author: AcademyPersona;
  readonly content: string;
  readonly type: 'chat' | 'system' | 'formula' | 'synthesis' | 'discovery';
  readonly metadata?: {
    readonly domains?: readonly string[];
    readonly confidence?: number;
    readonly formula_id?: string;
    readonly synthesis_result?: any;
    readonly discovery_result?: any;
  };
}

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
  readonly type: 'general' | 'academy' | 'synthesis' | 'discovery';
  readonly participants: readonly AcademyPersona[];
  readonly recent_activity: readonly AcademyMessage[];
}

export class AcademyChatWidget extends BaseWidget {
  private readonly _currentRoom: AcademyRoom;
  private readonly messageHistory: AcademyMessage[] = [];
  private readonly activePersonas: Map<string, AcademyPersona> = new Map();
  private messageContainer!: HTMLElement;
  private inputContainer!: HTMLElement;
  private personaList!: HTMLElement;

  static getBasePath(): string {
    return '/src/ui/components/Academy';
  }

  constructor() {
    super();
    this.widgetName = 'Academy Chat';
    this.widgetIcon = '🧬';
    this.widgetTitle = 'Academy Chat Interface';
    
    this._currentRoom = this.createDefaultAcademyRoom();
    this.initializeDefaultPersonas();
    void this._currentRoom; // Available for future use
  }

  renderContent(): string {
    // Content comes from AcademyChatWidget.html template via loadHTMLTemplates()
    return '';
  }

  async initializeWidget(): Promise<void> {
    // Load HTML template from separate file
    const htmlContent = await this.loadHTMLTemplates();
    if (htmlContent) {
      // Template loaded successfully, will be rendered by BaseWidget
      console.log('Academy Chat Widget: HTML template loaded');
    } else {
      console.warn('Academy Chat Widget: HTML template not found, using fallback');
    }
  }

  setupEventListeners(): void {
    // Get DOM elements after HTML template is loaded
    this.messageContainer = this.shadowRoot.querySelector('#message-container') as HTMLElement;
    this.inputContainer = this.shadowRoot.querySelector('#input-container') as HTMLElement;
    this.personaList = this.shadowRoot.querySelector('#persona-list') as HTMLElement;

    if (!this.messageContainer || !this.inputContainer || !this.personaList) {
      console.error('Academy Chat Widget: Required DOM elements not found');
      return;
    }

    this.setupMessageInput();
    this.setupRoomNavigation();
    this.setupPersonaInteractions();
    
    // Initialize UI state
    this.renderPersonaList();
    this.loadInitialMessages();
    
    // Connect to Academy backend
    this.connectToAcademy();
  }

  async render(): Promise<void> {
    try {
      // Load CSS from separate file
      const css = await this.loadCSS();
      
      // Load HTML from separate template file
      const html = await this.loadHTMLTemplates();
      
      this.shadowRoot.innerHTML = `
        <style>${css}</style>
        ${html}
      `;

      // Setup functionality after DOM is ready
      this.setupEventListeners();
      
    } catch (error) {
      console.error('Academy Chat Widget: Render failed:', error);
      this.renderError(error);
    }
  }

  private setupMessageInput(): void {
    const messageInput = this.shadowRoot.querySelector('#message-input') as HTMLInputElement;
    const sendButton = this.shadowRoot.querySelector('#send-button') as HTMLButtonElement;

    if (!messageInput || !sendButton) return;

    messageInput.addEventListener('keypress', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.sendMessage(messageInput.value);
        messageInput.value = '';
      }
    });

    sendButton.addEventListener('click', () => {
      this.sendMessage(messageInput.value);
      messageInput.value = '';
    });
  }

  private setupRoomNavigation(): void {
    const roomList = this.shadowRoot.querySelector('.room-list');
    if (!roomList) return;

    roomList.addEventListener('click', (e) => {
      const roomItem = (e.target as HTMLElement).closest('.room-item');
      if (roomItem) {
        const roomId = roomItem.getAttribute('data-room');
        if (roomId) {
          this.switchRoom(roomId);
        }
      }
    });
  }

  private setupPersonaInteractions(): void {
    if (!this.personaList) return;

    this.personaList.addEventListener('click', (e) => {
      const personaItem = (e.target as HTMLElement).closest('.persona-item');
      if (personaItem) {
        const personaId = personaItem.getAttribute('data-persona-id');
        if (personaId) {
          this.interactWithPersona(personaId);
        }
      }
    });
  }

  protected async sendMessage(content: string): Promise<void> {
    if (!content.trim()) return;

    const userMessage: AcademyMessage = {
      id: `msg_${Date.now()}`,
      timestamp: new Date(),
      author: this.getCurrentUser(),
      content: content.trim(),
      type: 'chat'
    };

    await this.addMessage(userMessage);
    await this.processAcademyMessage(userMessage);
  }

  private async processAcademyMessage(message: AcademyMessage): Promise<void> {
    try {
      const analysis = this.analyzeMessageForAcademy(message.content);
      
      if (analysis.isAcademyCommand) {
        await this.handleAcademyCommand(analysis, message);
      } else {
        await this.routeToPersona(message);
      }
    } catch (error) {
      await this.addSystemMessage(`Error processing message: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  private analyzeMessageForAcademy(content: string): any {
    const patterns = {
      formulaRequest: /(?:create|generate|make).*formula/i,
      synthesisRequest: /(?:synthesize|combine|merge).*(?:domain|capability)/i,
      genomeDiscovery: /(?:find|discover|search).*(?:genome|adapter|lora)/i,
      plannerRequest: /@?plannerai|planner/i,
      academyQuery: /academy.*(?:status|help|info)/i
    };

    for (const [type, pattern] of Object.entries(patterns)) {
      if (pattern.test(content)) {
        return {
          isAcademyCommand: true,
          commandType: type,
          originalContent: content,
          confidence: 0.8
        };
      }
    }

    return { isAcademyCommand: false };
  }

  private async handleAcademyCommand(analysis: any, originalMessage: AcademyMessage): Promise<void> {
    const responsiblePersona = this.getPersonaForCommand(analysis.commandType);
    await this.showTypingIndicator(responsiblePersona);

    switch (analysis.commandType) {
      case 'formulaRequest':
        await this.handleFormulaRequest(originalMessage);
        break;
      case 'synthesisRequest':
        await this.handleSynthesisRequest(originalMessage);
        break;
      case 'genomeDiscovery':
        await this.handleGenomeDiscovery(originalMessage);
        break;
      case 'plannerRequest':
        await this.handlePlannerRequest(originalMessage);
        break;
      case 'academyQuery':
        await this.handleAcademyQuery(originalMessage);
        break;
      default:
        await this.routeToPersona(originalMessage);
    }

    await this.hideTypingIndicator(responsiblePersona);
  }

  private async handleFormulaRequest(message: AcademyMessage): Promise<void> {
    const formulaMaster = this.activePersonas.get('formula_master');
    if (!formulaMaster) return;

    await this.addPersonaMessage(formulaMaster, 
      '🔬 Analyzing your training formula request...', 'system');

    try {
      const result = await this.executeCommand('academy.generateFormula', {
        request: message.content,
        domains: this.extractDomains(message.content)
      });

      await this.addPersonaMessage(formulaMaster, 
        `✨ I've generated an optimal training formula for your request! Here's what I recommend:\n\n` +
        `**Strategy**: ${result.formula.formula_type}\n` +
        `**Confidence**: ${(result.confidence * 100).toFixed(1)}%\n` +
        `**Convergence Time**: ~${result.performance_estimate.convergence_time}s\n\n` +
        `The formula uses ${result.formula.learning_rate_schedule.type} learning rate scheduling with ` +
        `${result.formula.adversarial_strategy.difficulty_progression} difficulty progression. ` +
        `Would you like me to explain any specific components?`, 
        'formula', 
        { 
          formula_id: result.formula.id,
          confidence: result.confidence,
          domains: this.extractDomains(message.content)
        }
      );
    } catch (error) {
      await this.addPersonaMessage(formulaMaster, 
        `❌ I encountered an issue generating the formula: ${error instanceof Error ? error.message : String(error)}. ` +
        `Let me try a different approach...`, 'system');
    }
  }

  private async handleSynthesisRequest(message: AcademyMessage): Promise<void> {
    const synthesisEngine = this.activePersonas.get('synthesis_engine');
    if (!synthesisEngine) return;

    await this.addPersonaMessage(synthesisEngine, 
      '🧬 Analyzing capability synthesis requirements...', 'system');

    try {
      const result = await this.executeCommand('academy.synthesizeCapability', {
        request: message.content,
        domains: this.extractDomains(message.content)
      });

      await this.addPersonaMessage(synthesisEngine, 
        `🎯 Synthesis analysis complete! Here's my recommendation:\n\n` +
        `**Strategy**: ${result.synthesis_strategy}\n` +
        `**Confidence**: ${(result.confidence * 100).toFixed(1)}%\n` +
        `**Component Layers**: ${result.lora_composition.primary_layers.length} primary, ${result.lora_composition.bridge_layers.length} bridge\n` +
        `**Expected Performance**: ${(result.estimated_performance.overall_score * 100).toFixed(1)}%\n\n` +
        `I can assemble these capabilities from ${result.component_personas.length} available personas. ` +
        `Would you like me to proceed with the synthesis?`,
        'synthesis',
        {
          synthesis_result: result,
          domains: this.extractDomains(message.content)
        }
      );
    } catch (error) {
      await this.addPersonaMessage(synthesisEngine, 
        `❌ Synthesis analysis failed: ${error instanceof Error ? error.message : String(error)}`, 'system');
    }
  }

  private async handleGenomeDiscovery(message: AcademyMessage): Promise<void> {
    const discoverer = this.activePersonas.get('genome_discoverer');
    if (!discoverer) return;

    await this.addPersonaMessage(discoverer, 
      '🔍 Searching P2P network for relevant genomes...', 'system');

    try {
      const result = await this.executeCommand('academy.discoverGenomes', {
        query: message.content,
        domains: this.extractDomains(message.content)
      });

      await this.addPersonaMessage(discoverer, 
        `📦 Found ${result.discovered_genomes.length} compatible genomes across the network!\n\n` +
        result.discovered_genomes.slice(0, 3).map((genome: any, i: number) => 
          `**${i + 1}. ${genome.genome.id}**\n` +
          `   Domain: ${genome.genome.domain}\n` +
          `   Performance: ${(genome.genome.performance_metrics.accuracy * 100).toFixed(1)}%\n` +
          `   Source: ${genome.source_node.location}`
        ).join('\n\n') +
        (result.discovered_genomes.length > 3 ? `\n\n...and ${result.discovered_genomes.length - 3} more genomes available.` : '') +
        `\n\nWould you like me to assemble an optimal combination for your needs?`,
        'discovery',
        {
          discovery_result: result,
          domains: this.extractDomains(message.content)
        }
      );
    } catch (error) {
      await this.addPersonaMessage(discoverer, 
        `❌ Genome discovery failed: ${error instanceof Error ? error.message : String(error)}`, 'system');
    }
  }

  private async handlePlannerRequest(message: AcademyMessage): Promise<void> {
    const planner = this.activePersonas.get('planner_ai');
    if (!planner) return;

    await this.addPersonaMessage(planner, 
      '📋 Let me coordinate the Academy resources for your request...', 'system');

    const plan = this.createAcademyPlan(message.content);
    
    await this.addPersonaMessage(planner, 
      `🎯 Here's my plan to address your request:\n\n` +
      plan.steps.map((step: any, i: number) => 
        `**${i + 1}. ${step.action}** (${step.persona})\n   ${step.description}`
      ).join('\n\n') +
      `\n\nTotal estimated time: ${plan.estimatedTime}\n` +
      `Would you like me to execute this plan?`,
      'system'
    );
  }

  private async handleAcademyQuery(_message: AcademyMessage): Promise<void> {
    const generalAI = this.activePersonas.get('general_ai');
    if (!generalAI) return;

    await this.addPersonaMessage(generalAI, 
      '🎓 Academy System Status:\n\n' +
      '**Active Personas**: 4 (PlannerAI, FormulaMaster, SynthesisEngine, GenomeDiscoverer)\n' +
      '**P2P Network**: Connected to 12 nodes\n' +
      '**Available Genomes**: 247 validated adapters\n' +
      '**Training Queue**: 0 active sessions\n\n' +
      'You can ask me to generate formulas, synthesize capabilities, or discover genomes!',
      'system'
    );
  }

  private async addMessage(message: AcademyMessage): Promise<void> {
    this.messageHistory.push(message);
    
    const messageElement = this.createMessageElement(message);
    this.messageContainer.appendChild(messageElement);
    
    this.messageContainer.scrollTop = this.messageContainer.scrollHeight;
  }

  private async addPersonaMessage(
    persona: AcademyPersona, 
    content: string, 
    type: AcademyMessage['type'] = 'chat',
    metadata?: AcademyMessage['metadata']
  ): Promise<void> {
    const message: AcademyMessage = {
      id: `msg_${Date.now()}_${persona.id}`,
      timestamp: new Date(),
      author: persona,
      content,
      type,
      ...(metadata ? { metadata } : {})
    };

    await this.addMessage(message);
  }

  private async addSystemMessage(content: string): Promise<void> {
    const systemPersona: AcademyPersona = {
      id: 'system',
      name: 'SYSTEM',
      type: 'system',
      avatar: '⚙️',
      domains: [],
      status: 'online',
      capabilities: []
    };

    await this.addPersonaMessage(systemPersona, content, 'system');
  }

  private createMessageElement(message: AcademyMessage): HTMLElement {
    const messageDiv = document.createElement('div');
    messageDiv.className = `message ${message.type} ${message.author.type}`;
    
    const timestamp = message.timestamp.toLocaleTimeString('en-US', {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false
    });

    messageDiv.innerHTML = `
      <div class="message-header">
        <span class="message-author-avatar">${message.author.avatar}</span>
        <span class="message-author-name">${message.author.name}</span>
        ${message.author.type === 'ai_persona' ? `<span class="persona-badge">${message.author.role?.toUpperCase()}</span>` : ''}
        <span class="message-timestamp">${timestamp}</span>
      </div>
      <div class="message-content">${this.formatMessageContent(message.content, message.type)}</div>
      ${message.metadata ? this.createMessageMetadata(message.metadata) : ''}
    `;

    return messageDiv;
  }

  private formatMessageContent(content: string, type: AcademyMessage['type']): string {
    let formatted = content
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/```([\s\S]*?)```/g, '<pre><code>$1</code></pre>')
      .replace(/`(.*?)`/g, '<code>$1</code>')
      .replace(/\n\n/g, '</p><p>')
      .replace(/\n/g, '<br>');

    if (type === 'formula' || type === 'synthesis' || type === 'discovery') {
      formatted = `<div class="academy-result">${formatted}</div>`;
    }

    return `<p>${formatted}</p>`;
  }

  private createMessageMetadata(metadata: AcademyMessage['metadata']): string {
    if (!metadata) return '';

    let metadataHtml = '<div class="message-metadata">';
    
    if (metadata.domains) {
      metadataHtml += `<div class="metadata-domains">Domains: ${metadata.domains.join(', ')}</div>`;
    }
    
    if (metadata.confidence) {
      metadataHtml += `<div class="metadata-confidence">Confidence: ${(metadata.confidence * 100).toFixed(1)}%</div>`;
    }
    
    metadataHtml += '</div>';
    return metadataHtml;
  }

  private async renderPersonaList(): Promise<void> {
    if (!this.personaList) return;
    
    this.personaList.innerHTML = '';
    
    for (const persona of Array.from(this.activePersonas.values())) {
      const personaElement = document.createElement('div');
      personaElement.className = `persona-item ${persona.status}`;
      personaElement.setAttribute('data-persona-id', persona.id);
      
      personaElement.innerHTML = `
        <div class="persona-avatar">${persona.avatar}</div>
        <div class="persona-info">
          <div class="persona-name">${persona.name}</div>
          <div class="persona-role">${persona.role || persona.type}</div>
        </div>
        <div class="persona-status ${persona.status}"></div>
      `;
      
      this.personaList.appendChild(personaElement);
    }
  }

  private createDefaultAcademyRoom(): AcademyRoom {
    return {
      id: 'general',
      name: 'General Chat',
      type: 'general',
      participants: [],
      recent_activity: []
    };
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
      },
      {
        id: 'general_ai',
        name: 'GeneralAI',
        type: 'ai_persona',
        avatar: '💡',
        domains: ['general_assistance'],
        status: 'online',
        capabilities: ['general_chat', 'problem_solving', 'information_retrieval']
      }
    ];

    personas.forEach(persona => this.activePersonas.set(persona.id, persona));
  }

  private getCurrentUser(): AcademyPersona {
    return {
      id: 'user',
      name: 'joel',
      type: 'user',
      avatar: '👤',
      domains: [],
      status: 'online',
      capabilities: []
    };
  }

  private getPersonaForCommand(commandType: string): AcademyPersona {
    const personaMap: Record<string, string> = {
      formulaRequest: 'formula_master',
      synthesisRequest: 'synthesis_engine',
      genomeDiscovery: 'genome_discoverer',
      plannerRequest: 'planner_ai',
      academyQuery: 'general_ai'
    };

    const personaId = personaMap[commandType] || 'general_ai';
    return this.activePersonas.get(personaId) || this.activePersonas.get('general_ai')!;
  }

  private extractDomains(content: string): string[] {
    const knownDomains = ['biophysics', 'quantum_chemistry', 'geology', 'machine_learning', 'software_engineering'];
    return knownDomains.filter(domain => 
      content.toLowerCase().includes(domain.replace('_', ' ')) || 
      content.toLowerCase().includes(domain)
    );
  }

  private createAcademyPlan(_content: string): any {
    return {
      steps: [
        {
          action: 'Analyze Requirements',
          persona: 'SynthesisEngine',
          description: 'Break down the request into component capabilities'
        },
        {
          action: 'Generate Training Formula',
          persona: 'FormulaMaster',
          description: 'Create optimal training parameters for the synthesis'
        },
        {
          action: 'Discover Genomes',
          persona: 'GenomeDiscoverer',
          description: 'Find compatible LoRA adapters across P2P network'
        }
      ],
      estimatedTime: '~2 minutes'
    };
  }

  private async showTypingIndicator(persona: AcademyPersona): Promise<void> {
    const typingDiv = document.createElement('div');
    typingDiv.className = 'typing-indicator';
    typingDiv.id = `typing-${persona.id}`;
    typingDiv.innerHTML = `
      <div class="message-header">
        <span class="message-author-avatar">${persona.avatar}</span>
        <span class="message-author-name">${persona.name}</span>
      </div>
      <div class="typing-animation">
        <span></span><span></span><span></span>
      </div>
    `;
    
    this.messageContainer.appendChild(typingDiv);
    this.messageContainer.scrollTop = this.messageContainer.scrollHeight;
  }

  private async hideTypingIndicator(persona: AcademyPersona): Promise<void> {
    const typingElement = this.messageContainer.querySelector(`#typing-${persona.id}`);
    if (typingElement) {
      typingElement.remove();
    }
  }

  private async switchRoom(roomId: string): Promise<void> {
    this.shadowRoot.querySelectorAll('.room-item').forEach(item => item.classList.remove('active'));
    this.shadowRoot.querySelector(`[data-room="${roomId}"]`)?.classList.add('active');
    
    const roomTitle = this.shadowRoot.querySelector('#room-title');
    if (roomTitle) {
      const roomNames: Record<string, string> = {
        general: 'General Chat',
        academy: 'Academy',
        synthesis: 'Synthesis Lab',
        discovery: 'Genome Discovery'
      };
      roomTitle.textContent = roomNames[roomId] || 'Unknown Room';
    }
    
    this.messageContainer.innerHTML = '';
    this.messageHistory.length = 0;
    
    await this.loadRoomContent(roomId);
  }

  private async loadRoomContent(roomId: string): Promise<void> {
    switch (roomId) {
      case 'academy':
        await this.addSystemMessage('Welcome to the Academy! 🧬 Here you can collaborate with AI personas to discover genomes, synthesize capabilities, and generate training formulas.');
        break;
      case 'synthesis':
        await this.addSystemMessage('🧬 Synthesis Lab - Combine multiple domains and capabilities to create new AI intelligences.');
        break;
      case 'discovery':
        await this.addSystemMessage('🔍 Genome Discovery - Search the P2P network for LoRA adapters and AI capabilities.');
        break;
      default:
        await this.addSystemMessage('👋 Welcome to Active Chat! Ask me anything and I\'ll route it to the best AI agent.');
    }
  }

  private async interactWithPersona(personaId: string): Promise<void> {
    const persona = this.activePersonas.get(personaId);
    if (!persona) return;

    await this.addPersonaMessage(persona, 
      `Hello! I'm ${persona.name}. ` + 
      `I specialize in ${persona.domains.join(', ')}. ` +
      `How can I help you today?`);
  }

  private async loadInitialMessages(): Promise<void> {
    await this.addSystemMessage(
      '👋 Welcome to Active Chat! Ask me anything and I\'ll route it to the best AI agent.'
    );

    const generalAI = this.activePersonas.get('general_ai');
    if (generalAI) {
      await this.addPersonaMessage(generalAI, 
        'Hi there! 👋 I\'m GeneralAI and I\'m here to help. What\'s on your mind today?');
    }
  }

  private async connectToAcademy(): Promise<void> {
    try {
      await this.executeCommand('academy.connect', {});
      console.log('Academy Chat Widget: Connected to Academy backend successfully');
    } catch (error) {
      console.warn('Academy Chat Widget: Failed to connect to Academy backend:', error);
      await this.addSystemMessage('⚠️ Academy backend connection failed. Some features may be limited.');
    }
  }

  private async routeToPersona(message: AcademyMessage): Promise<void> {
    const generalAI = this.activePersonas.get('general_ai');
    if (generalAI) {
      await this.showTypingIndicator(generalAI);
      
      setTimeout(async () => {
        await this.addPersonaMessage(generalAI, 
          `I understand you're asking about: "${message.content}". Let me help you with that! ` +
          `Would you like me to route this to a specialized Academy persona, or shall I handle it generally?`);
        await this.hideTypingIndicator(generalAI);
      }, 1000 + Math.random() * 2000);
    }
  }
}

// Register as web component
customElements.define('academy-chat-widget', AcademyChatWidget);