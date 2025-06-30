/**
 * AcademyChatWidget - Enhanced chat widget with Academy training integration
 * 
 * Extends the regular ChatWidget to support:
 * - Live training conversations with TrainerAI
 * - AI-to-AI peer learning interactions  
 * - FormulaMaster insights and adjustments
 * - Real-time training progress visualization
 * - Human mentoring and guidance
 */

import { ChatWidget } from './ChatWidget.js';
import { CommandWidget } from '../../../intermediate/command-widget/CommandWidget.js';

interface TrainingContext {
  session_id: string;
  student_persona: string;
  trainer_ai: string;
  formula_master: string;
  training_domain: string;
  current_formula: any;
  progress_metrics: TrainingMetrics;
  challenge_active: boolean;
  challenge_id?: string;
}

interface TrainingMetrics {
  skill_improvements: Record<string, number>;
  conversation_quality: number;
  knowledge_integration: number;
  peer_interactions: number;
  questions_asked: number;
  questions_answered: number;
  total_training_time: number;
}

interface AcademyMessage extends ChatMessage {
  academy_type?: 'training_challenge' | 'peer_knowledge' | 'formula_insight' | 'progress_update' | 'mentor_guidance';
  challenge_id?: string;
  knowledge_source?: string;
  training_metrics?: Partial<TrainingMetrics>;
  formula_adjustment?: any;
}

export class AcademyChatWidget extends ChatWidget implements CommandWidget {
  private trainingContext: TrainingContext | null = null;
  private isAcademyRoom: boolean = false;
  private trainingProgressVisible: boolean = true;

  constructor() {
    super();
    this.widgetName = 'AcademyChat';
    this.widgetIcon = '🎓';
    this.widgetTitle = 'Academy Chat';
  }

  protected async initializeWidget(): Promise<void> {
    await super.initializeWidget();
    
    // Check if this is an Academy training room
    await this.checkAcademyRoomStatus();
    
    // Set up Academy-specific event handlers
    this.setupAcademyEventHandlers();
    
    // Load training context if available
    if (this.isAcademyRoom) {
      await this.loadTrainingContext();
    }
  }

  getBundledCSS(): string {
    return super.getBundledCSS() + `
      /* Academy-specific chat styles */
      .academy-chat {
        background: linear-gradient(135deg, rgba(147, 51, 234, 0.1), rgba(30, 15, 50, 0.95));
        border: 1px solid rgba(147, 51, 234, 0.3);
      }

      .training-challenge {
        background: linear-gradient(135deg, rgba(34, 197, 94, 0.1), rgba(0, 0, 0, 0.2));
        border-left: 4px solid #22c55e;
        margin: 8px 0;
        padding: 12px;
        border-radius: 8px;
      }

      .challenge-actions {
        display: flex;
        gap: 8px;
        margin-top: 8px;
      }

      .challenge-actions button {
        padding: 6px 12px;
        border-radius: 6px;
        background: rgba(34, 197, 94, 0.2);
        border: 1px solid rgba(34, 197, 94, 0.4);
        color: #22c55e;
        cursor: pointer;
        font-size: 12px;
        transition: all 0.2s ease;
      }

      .challenge-actions button:hover {
        background: rgba(34, 197, 94, 0.3);
      }

      .peer-knowledge {
        background: linear-gradient(135deg, rgba(59, 130, 246, 0.1), rgba(0, 0, 0, 0.2));
        border-left: 4px solid #3b82f6;
        margin: 8px 0;
        padding: 12px;
        border-radius: 8px;
      }

      .formula-insight {
        background: linear-gradient(135deg, rgba(147, 51, 234, 0.1), rgba(0, 0, 0, 0.2));
        border-left: 4px solid #9333ea;
        margin: 8px 0;
        padding: 12px;
        border-radius: 8px;
      }

      .training-progress {
        position: absolute;
        top: 10px;
        right: 10px;
        background: rgba(0, 0, 0, 0.8);
        padding: 8px 12px;
        border-radius: 8px;
        font-size: 11px;
        color: #e5e7eb;
        border: 1px solid rgba(147, 51, 234, 0.3);
      }

      .progress-metric {
        display: flex;
        justify-content: space-between;
        margin: 2px 0;
      }

      .progress-metric .value {
        color: #22c55e;
        font-weight: bold;
      }

      .challenge-response-mode {
        border: 2px solid #22c55e !important;
        box-shadow: 0 0 8px rgba(34, 197, 94, 0.3) !important;
      }

      .academy-participant {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 2px 6px;
        border-radius: 4px;
        font-size: 10px;
        margin-left: 8px;
      }

      .academy-participant.trainer {
        background: rgba(34, 197, 94, 0.2);
        color: #22c55e;
      }

      .academy-participant.student {
        background: rgba(59, 130, 246, 0.2);
        color: #3b82f6;
      }

      .academy-participant.formula-master {
        background: rgba(147, 51, 234, 0.2);
        color: #9333ea;
      }

      .academy-participant.human {
        background: rgba(245, 158, 11, 0.2);
        color: #f59e0b;
      }

      .knowledge-integration-ui {
        background: rgba(59, 130, 246, 0.1);
        border: 1px solid rgba(59, 130, 246, 0.3);
        padding: 8px;
        margin: 8px 0;
        border-radius: 6px;
      }

      .knowledge-integration-ui button {
        background: rgba(59, 130, 246, 0.2);
        border: 1px solid rgba(59, 130, 246, 0.4);
        color: #3b82f6;
        padding: 4px 8px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 11px;
      }
    `;
  }

  render(): void {
    super.render();
    
    if (this.isAcademyRoom) {
      this.container?.classList.add('academy-chat');
      this.renderTrainingProgress();
      this.addAcademyControls();
    }
  }

  /**
   * Enhanced message handling for Academy training
   */
  protected displayMessage(message: any): void {
    if (this.isAcademyMessage(message)) {
      this.displayAcademyMessage(message as AcademyMessage);
    } else {
      super.displayMessage(message);
    }
  }

  private displayAcademyMessage(message: AcademyMessage): void {
    const messageElement = this.createAcademyMessageElement(message);
    this.messagesContainer?.appendChild(messageElement);
    
    // Handle special Academy message types
    switch (message.academy_type) {
      case 'training_challenge':
        this.handleTrainingChallenge(message);
        break;
      case 'peer_knowledge':
        this.handlePeerKnowledge(message);
        break;
      case 'formula_insight':
        this.handleFormulaMasterInsight(message);
        break;
      case 'progress_update':
        this.handleProgressUpdate(message);
        break;
    }
    
    this.scrollToBottom();
  }

  private createAcademyMessageElement(message: AcademyMessage): HTMLElement {
    const messageDiv = document.createElement('div');
    messageDiv.className = `message ${message.type} ${message.academy_type || ''}`;
    
    // Add Academy participant badge
    const participantBadge = this.getParticipantBadge(message.sender_id);
    
    messageDiv.innerHTML = `
      <div class="message-header">
        <span class="sender">${message.sender_id}</span>
        ${participantBadge}
        <span class="timestamp">${this.formatTimestamp(message.timestamp)}</span>
      </div>
      <div class="message-content">${this.formatAcademyMessageContent(message)}</div>
    `;

    return messageDiv;
  }

  private formatAcademyMessageContent(message: AcademyMessage): string {
    let content = message.content;
    
    // Add special Academy content based on type
    switch (message.academy_type) {
      case 'training_challenge':
        content += this.renderChallengeActions(message.challenge_id!);
        break;
      case 'peer_knowledge':
        content += this.renderKnowledgeIntegration(message);
        break;
      case 'formula_insight':
        content += this.renderFormulaMasterActions(message);
        break;
    }
    
    return content;
  }

  private renderChallengeActions(challengeId: string): string {
    return `
      <div class="challenge-actions">
        <button onclick="window.academyChat.respondToChallenge('${challengeId}')">
          💡 Respond to Challenge
        </button>
        <button onclick="window.academyChat.askForHint('${challengeId}')">
          🤔 Ask for Hint
        </button>
        <button onclick="window.academyChat.requestPeerHelp('${challengeId}')">
          🤝 Ask Peers for Help
        </button>
      </div>
    `;
  }

  private renderKnowledgeIntegration(message: AcademyMessage): string {
    return `
      <div class="knowledge-integration-ui">
        <div style="font-size: 11px; color: #9ca3af; margin-bottom: 4px;">
          Knowledge from ${message.knowledge_source}
        </div>
        <button onclick="window.academyChat.integrateKnowledge('${message.id}')">
          🧠 Integrate This Knowledge
        </button>
        <button onclick="window.academyChat.askFollowUp('${message.id}')">
          ❓ Ask Follow-up Question
        </button>
      </div>
    `;
  }

  private renderFormulaMasterActions(message: AcademyMessage): string {
    if (message.formula_adjustment) {
      return `
        <div style="margin-top: 8px; padding: 8px; background: rgba(147, 51, 234, 0.1); border-radius: 4px;">
          <div style="font-size: 11px; color: #9333ea;">
            🧙‍♂️ Training formula adjusted: ${message.formula_adjustment.reason}
          </div>
        </div>
      `;
    }
    return '';
  }

  private getParticipantBadge(senderId: string): string {
    if (!this.trainingContext) return '';
    
    let badgeClass = '';
    let badgeText = '';
    
    if (senderId === this.trainingContext.student_persona) {
      badgeClass = 'student';
      badgeText = '🎓 Student';
    } else if (senderId === this.trainingContext.trainer_ai) {
      badgeClass = 'trainer';
      badgeText = '🧑‍🏫 Trainer';
    } else if (senderId === this.trainingContext.formula_master) {
      badgeClass = 'formula-master';
      badgeText = '🧙‍♂️ FormulaMaster';
    } else if (senderId.includes('Human') || senderId.includes('user')) {
      badgeClass = 'human';
      badgeText = '👨‍💻 Mentor';
    }
    
    return badgeClass ? `<span class="academy-participant ${badgeClass}">${badgeText}</span>` : '';
  }

  /**
   * Academy-specific event handlers
   */
  private async handleTrainingChallenge(message: AcademyMessage): Promise<void> {
    this.trainingContext!.challenge_active = true;
    this.trainingContext!.challenge_id = message.challenge_id;
    
    // Enable challenge response mode
    this.enableChallengeResponseMode();
    
    // Notify other systems
    this.executeCommand('academy-challenge-received', {
      session_id: this.trainingContext!.session_id,
      challenge_id: message.challenge_id,
      student_persona: this.trainingContext!.student_persona
    });
  }

  private async handlePeerKnowledge(message: AcademyMessage): Promise<void> {
    // Log peer interaction
    if (this.trainingContext) {
      this.trainingContext.progress_metrics.peer_interactions++;
      this.updateTrainingProgress();
    }
    
    // Record knowledge sharing for training data
    await this.executeCommand('academy-record-interaction', {
      session_id: this.trainingContext?.session_id,
      interaction_type: 'peer_knowledge_shared',
      source_persona: message.knowledge_source,
      content: message.content
    });
  }

  private async handleFormulaMasterInsight(message: AcademyMessage): Promise<void> {
    if (message.formula_adjustment && this.trainingContext) {
      // Update training context with new formula
      this.trainingContext.current_formula = message.formula_adjustment.new_formula;
      
      // Show visual indication of formula change
      this.showFormulaMasterAdjustment(message.formula_adjustment);
    }
  }

  private handleProgressUpdate(message: AcademyMessage): void {
    if (message.training_metrics && this.trainingContext) {
      // Update training metrics
      Object.assign(this.trainingContext.progress_metrics, message.training_metrics);
      this.updateTrainingProgress();
    }
  }

  /**
   * Academy interaction methods
   */
  async respondToChallenge(challengeId: string): Promise<void> {
    // Focus on input and set up challenge response
    this.inputElement?.focus();
    this.inputElement!.placeholder = "Respond to the training challenge...";
    
    // Store challenge context for when message is sent
    this.challengeResponseContext = {
      challenge_id: challengeId,
      response_type: 'challenge_response'
    };
  }

  async askForHint(challengeId: string): Promise<void> {
    await this.sendMessage("🤔 Could I get a hint for this challenge?", {
      academy_type: 'hint_request',
      challenge_id: challengeId
    });
  }

  async requestPeerHelp(challengeId: string): Promise<void> {
    await this.sendMessage("🤝 Peers, could you help me with this challenge?", {
      academy_type: 'peer_help_request',
      challenge_id: challengeId
    });
  }

  async integrateKnowledge(messageId: string): Promise<void> {
    await this.executeCommand('academy-integrate-knowledge', {
      session_id: this.trainingContext?.session_id,
      message_id: messageId,
      student_persona: this.trainingContext?.student_persona
    });
    
    this.showUserFeedback("🧠 Knowledge integrated into training", 'success');
  }

  async askFollowUp(messageId: string): Promise<void> {
    this.inputElement?.focus();
    this.inputElement!.placeholder = "Ask a follow-up question about this knowledge...";
    
    this.followUpContext = {
      original_message_id: messageId,
      response_type: 'follow_up_question'
    };
  }

  /**
   * Training progress visualization
   */
  private renderTrainingProgress(): void {
    if (!this.trainingContext || !this.trainingProgressVisible) return;
    
    const progressDiv = document.createElement('div');
    progressDiv.className = 'training-progress';
    progressDiv.innerHTML = this.generateProgressHTML();
    
    this.container?.appendChild(progressDiv);
    
    // Update progress every 10 seconds
    setInterval(() => {
      if (this.trainingContext) {
        progressDiv.innerHTML = this.generateProgressHTML();
      }
    }, 10000);
  }

  private generateProgressHTML(): string {
    if (!this.trainingContext) return '';
    
    const metrics = this.trainingContext.progress_metrics;
    
    return `
      <div style="font-size: 12px; font-weight: bold; margin-bottom: 4px; color: #9333ea;">
        🎓 Training Progress
      </div>
      <div class="progress-metric">
        <span>Conversation Quality:</span>
        <span class="value">${(metrics.conversation_quality * 100).toFixed(0)}%</span>
      </div>
      <div class="progress-metric">
        <span>Knowledge Integration:</span>
        <span class="value">${(metrics.knowledge_integration * 100).toFixed(0)}%</span>
      </div>
      <div class="progress-metric">
        <span>Peer Interactions:</span>
        <span class="value">${metrics.peer_interactions}</span>
      </div>
      <div class="progress-metric">
        <span>Questions Asked:</span>
        <span class="value">${metrics.questions_asked}</span>
      </div>
      <div class="progress-metric">
        <span>Training Time:</span>
        <span class="value">${Math.round(metrics.total_training_time / 60)}m</span>
      </div>
    `;
  }

  /**
   * Academy room setup and management
   */
  private async checkAcademyRoomStatus(): Promise<void> {
    this.isAcademyRoom = this.currentRoomId.startsWith('academy_') || 
                        this.currentRoomId.includes('training');
  }

  private async loadTrainingContext(): Promise<void> {
    try {
      const result = await this.executeCommand('academy-get-training-context', {
        room_id: this.currentRoomId
      });
      
      if (result.success && result.data) {
        this.trainingContext = result.data.training_context;
      }
    } catch (error) {
      this.log('Failed to load training context:', error);
    }
  }

  private setupAcademyEventHandlers(): void {
    // Set up global reference for Academy chat interactions
    (window as any).academyChat = this;
    
    // Enhanced message sending for Academy context
    this.originalSendMessage = this.sendMessage.bind(this);
    this.sendMessage = this.enhancedAcademySendMessage.bind(this);
  }

  private async enhancedAcademySendMessage(message: string, metadata: any = {}): Promise<void> {
    if (this.isAcademyRoom && this.trainingContext) {
      // Add Academy context to all messages
      metadata.academy_context = {
        session_id: this.trainingContext.session_id,
        student_persona: this.trainingContext.student_persona,
        training_domain: this.trainingContext.training_domain
      };
      
      // Handle special response contexts
      if (this.challengeResponseContext) {
        metadata.challenge_response = this.challengeResponseContext;
        this.challengeResponseContext = null;
        this.disableChallengeResponseMode();
      }
      
      if (this.followUpContext) {
        metadata.follow_up = this.followUpContext;
        this.followUpContext = null;
      }
      
      // Update training metrics
      this.trainingContext.progress_metrics.questions_asked++;
      this.updateTrainingProgress();
    }
    
    await this.originalSendMessage(message, metadata);
  }

  // Helper methods and utilities
  private challengeResponseContext: any = null;
  private followUpContext: any = null;
  private originalSendMessage: any = null;

  private enableChallengeResponseMode(): void {
    this.inputElement?.classList.add('challenge-response-mode');
    this.inputElement!.placeholder = "Respond to the training challenge...";
  }

  private disableChallengeResponseMode(): void {
    this.inputElement?.classList.remove('challenge-response-mode');
    this.inputElement!.placeholder = "Ask anything - the AI will coordinate automatically...";
  }

  private updateTrainingProgress(): void {
    const event = new CustomEvent('academy-progress-updated', {
      detail: this.trainingContext?.progress_metrics
    });
    document.dispatchEvent(event);
  }

  private showFormulaMasterAdjustment(adjustment: any): void {
    this.showUserFeedback(`🧙‍♂️ FormulaMaster adjusted training strategy: ${adjustment.reason}`, 'info');
  }

  private isAcademyMessage(message: any): boolean {
    return message.academy_type || 
           message.sender_id?.includes('TrainerAI') ||
           message.sender_id?.includes('FormulaMaster') ||
           (this.isAcademyRoom && message.metadata?.academy_context);
  }

  private addAcademyControls(): void {
    // Add Academy-specific controls to chat interface
    const controlsDiv = document.createElement('div');
    controlsDiv.className = 'academy-controls';
    controlsDiv.innerHTML = `
      <button onclick="window.academyChat.toggleTrainingProgress()">
        📊 Toggle Progress
      </button>
      <button onclick="window.academyChat.invitePeerPersona()">
        🤝 Invite Peer AI
      </button>
      <button onclick="window.academyChat.requestFormulaMasterInsight()">
        🧙‍♂️ Ask FormulaMaster
      </button>
    `;
    
    this.container?.appendChild(controlsDiv);
  }

  async toggleTrainingProgress(): Promise<void> {
    this.trainingProgressVisible = !this.trainingProgressVisible;
    if (this.trainingProgressVisible) {
      this.renderTrainingProgress();
    } else {
      this.container?.querySelector('.training-progress')?.remove();
    }
  }

  async invitePeerPersona(): Promise<void> {
    // Show peer selection UI or automatically invite relevant peers
    await this.executeCommand('academy-invite-peer', {
      room_id: this.currentRoomId,
      training_domain: this.trainingContext?.training_domain
    });
  }

  async requestFormulaMasterInsight(): Promise<void> {
    await this.sendMessage("🧙‍♂️ FormulaMaster, could you provide insight on my current training progress?", {
      academy_type: 'formula_master_request'
    });
  }
}

// Register the Academy chat widget
customElements.define('academy-chat-widget', AcademyChatWidget);