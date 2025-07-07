/**
 * Academy Training Recommendation Widget - Detailed training recommendations for personas
 * 
 * Shows up as overlay/popup when user clicks on a persona for Academy training.
 * Provides detailed analysis and recommendations for improving the persona.
 * Has "Start Training" button to create new Academy session.
 */

import { BaseWidget } from '../shared/BaseWidget.js';

interface TrainingRecommendation {
  readonly persona_id: string;
  readonly persona_name: string;
  readonly specialization_name: string;
  readonly current_capabilities: readonly string[];
  readonly recommended_improvements: readonly string[];
  readonly training_focus_areas: readonly string[];
  readonly estimated_training_time: string;
  readonly difficulty_level: 'beginner' | 'intermediate' | 'advanced';
  readonly success_probability: number;
}

interface TrainingModule {
  readonly name: string;
  readonly description: string;
  readonly focus_areas: readonly string[];
  readonly estimated_duration: string;
}

export class AcademyTrainingRecommendationWidget extends BaseWidget {
  private recommendation: TrainingRecommendation | null = null;
  private trainingModules: TrainingModule[] = [];
  private isVisible: boolean = false;

  static getBasePath(): string {
    return '/src/ui/components/Academy';
  }

  constructor() {
    super();
    this.widgetName = 'Academy Training Recommendation';
    this.widgetIcon = '🎯';
    this.widgetTitle = 'Training Recommendation';
  }

  renderContent(): string {
    if (!this.recommendation || !this.isVisible) {
      return '<div class="recommendation-hidden"></div>';
    }

    return `
      <div class="recommendation-overlay">
        <div class="recommendation-dialog">
          <div class="recommendation-header">
            <div class="header-icon">🎯</div>
            <div class="header-content">
              <h3 class="recommendation-title">Academy Training Recommendation for ${this.recommendation.persona_name}</h3>
              <button class="close-btn" id="close-recommendation">×</button>
            </div>
          </div>

          <div class="recommendation-content">
            <div class="request-section">
              <h4>Your Request:</h4>
              <p>Seek to understand your capabilities and role as the manager of other AI's. You're their leader and you need to teach them that you have specific delegation and communication skills to learn how to use it without messing with your chat. You cannot print needless command text to the users. You can all browse the web and run shell commands.</p>
            </div>

            <div class="ai-recommendation-section">
              <h4>AI Recommendation:</h4>
              <div class="specialization-info">
                <strong>Specialization Name:</strong> "${this.recommendation.specialization_name}"
              </div>
              
              <div class="training-modules">
                ${this.trainingModules.map(module => `
                  <div class="training-module">
                    <h5>${module.name}</h5>
                    <p>${module.description}</p>
                    <div class="module-details">
                      <span class="duration">Duration: ${module.estimated_duration}</span>
                      <div class="focus-areas">
                        Focus: ${module.focus_areas.join(', ')}
                      </div>
                    </div>
                  </div>
                `).join('')}
              </div>

              <div class="recommendation-details">
                <div class="capability-analysis">
                  <h5>Current Capabilities:</h5>
                  <ul>
                    ${this.recommendation.current_capabilities.map(cap => `<li>${cap}</li>`).join('')}
                  </ul>
                </div>

                <div class="improvement-areas">
                  <h5>Recommended Improvements:</h5>
                  <ul>
                    ${this.recommendation.recommended_improvements.map(imp => `<li>${imp}</li>`).join('')}
                  </ul>
                </div>

                <div class="training-metrics">
                  <div class="metric">
                    <label>Estimated Training Time:</label>
                    <value>${this.recommendation.estimated_training_time}</value>
                  </div>
                  <div class="metric">
                    <label>Difficulty Level:</label>
                    <value>${this.recommendation.difficulty_level}</value>
                  </div>
                  <div class="metric">
                    <label>Success Probability:</label>
                    <value>${(this.recommendation.success_probability * 100).toFixed(1)}%</value>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div class="recommendation-actions">
            <button class="cancel-btn" id="cancel-training">Cancel</button>
            <button class="start-training-btn" id="start-training">Start Training</button>
          </div>
        </div>
      </div>
    `;
  }

  setupEventListeners(): void {
    const closeBtn = this.shadowRoot.querySelector('#close-recommendation');
    const cancelBtn = this.shadowRoot.querySelector('#cancel-training');
    const startBtn = this.shadowRoot.querySelector('#start-training');

    closeBtn?.addEventListener('click', () => this.hide());
    cancelBtn?.addEventListener('click', () => this.hide());
    startBtn?.addEventListener('click', () => this.startTraining());

    // Close on overlay click
    const overlay = this.shadowRoot.querySelector('.recommendation-overlay');
    overlay?.addEventListener('click', (e) => {
      if (e.target === overlay) {
        this.hide();
      }
    });

    // Escape key to close
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && this.isVisible) {
        this.hide();
      }
    });
  }

  async loadCSS(): Promise<string> {
    return `
      .recommendation-hidden {
        display: none;
      }

      .recommendation-overlay {
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        background: rgba(0, 0, 0, 0.7);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 1000;
        font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
      }

      .recommendation-dialog {
        background: #1a1d23;
        border: 1px solid #4fc3f7;
        border-radius: 8px;
        max-width: 600px;
        width: 90%;
        max-height: 80vh;
        overflow-y: auto;
        color: #e0e6ed;
      }

      .recommendation-header {
        background: #2d3748;
        padding: 16px 20px;
        border-bottom: 1px solid #4fc3f7;
        display: flex;
        align-items: center;
        gap: 12px;
      }

      .header-icon {
        font-size: 24px;
      }

      .header-content {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: space-between;
      }

      .recommendation-title {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: #4fc3f7;
      }

      .close-btn {
        background: none;
        border: none;
        color: #8a92a5;
        font-size: 20px;
        cursor: pointer;
        padding: 4px;
        border-radius: 4px;
        transition: background-color 0.2s;
      }

      .close-btn:hover {
        background: rgba(255, 255, 255, 0.1);
        color: #ffffff;
      }

      .recommendation-content {
        padding: 20px;
      }

      .request-section,
      .ai-recommendation-section {
        margin-bottom: 20px;
      }

      .request-section h4,
      .ai-recommendation-section h4 {
        margin: 0 0 8px 0;
        font-size: 14px;
        font-weight: 600;
        color: #4fc3f7;
      }

      .request-section p {
        margin: 0;
        font-size: 13px;
        line-height: 1.5;
        color: #b8c5d1;
      }

      .specialization-info {
        background: rgba(79, 195, 247, 0.1);
        padding: 12px;
        border-radius: 6px;
        margin-bottom: 16px;
        font-size: 13px;
      }

      .specialization-info strong {
        color: #4fc3f7;
      }

      .training-modules {
        margin: 16px 0;
      }

      .training-module {
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 6px;
        padding: 12px;
        margin-bottom: 12px;
      }

      .training-module h5 {
        margin: 0 0 6px 0;
        font-size: 13px;
        font-weight: 600;
        color: #ffffff;
      }

      .training-module p {
        margin: 0 0 8px 0;
        font-size: 12px;
        line-height: 1.4;
        color: #b8c5d1;
      }

      .module-details {
        display: flex;
        gap: 16px;
        font-size: 11px;
        color: #8a92a5;
      }

      .duration {
        font-weight: 500;
      }

      .capability-analysis,
      .improvement-areas {
        margin-bottom: 16px;
      }

      .capability-analysis h5,
      .improvement-areas h5 {
        margin: 0 0 8px 0;
        font-size: 13px;
        font-weight: 600;
        color: #ffffff;
      }

      .capability-analysis ul,
      .improvement-areas ul {
        margin: 0;
        padding-left: 20px;
        font-size: 12px;
        line-height: 1.4;
        color: #b8c5d1;
      }

      .capability-analysis li,
      .improvement-areas li {
        margin-bottom: 4px;
      }

      .training-metrics {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
        gap: 12px;
        margin-top: 16px;
      }

      .metric {
        background: rgba(255, 255, 255, 0.05);
        padding: 8px 12px;
        border-radius: 4px;
        font-size: 12px;
      }

      .metric label {
        display: block;
        color: #8a92a5;
        margin-bottom: 2px;
      }

      .metric value {
        font-weight: 600;
        color: #4fc3f7;
      }

      .recommendation-actions {
        padding: 16px 20px;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
        display: flex;
        gap: 12px;
        justify-content: flex-end;
      }

      .cancel-btn,
      .start-training-btn {
        padding: 8px 16px;
        border: none;
        border-radius: 4px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s ease;
      }

      .cancel-btn {
        background: rgba(255, 255, 255, 0.1);
        color: #b8c5d1;
      }

      .cancel-btn:hover {
        background: rgba(255, 255, 255, 0.15);
        color: #ffffff;
      }

      .start-training-btn {
        background: #4fc3f7;
        color: #1a1d23;
      }

      .start-training-btn:hover {
        background: #29b6f6;
      }

      /* Scrollbar */
      .recommendation-dialog::-webkit-scrollbar {
        width: 8px;
      }

      .recommendation-dialog::-webkit-scrollbar-track {
        background: #2d3748;
      }

      .recommendation-dialog::-webkit-scrollbar-thumb {
        background: #4a5568;
        border-radius: 4px;
      }

      .recommendation-dialog::-webkit-scrollbar-thumb:hover {
        background: #5a6578;
      }
    `;
  }

  public async showRecommendationForPersona(personaId: string, personaName: string): Promise<void> {
    try {
      // Get training recommendation from Academy system
      const recommendation = await this.generateTrainingRecommendation(personaId, personaName);
      
      this.recommendation = recommendation;
      this.isVisible = true;
      
      await this.render();
      
    } catch (error) {
      console.error('Failed to show training recommendation:', error);
      this.showError('Failed to generate training recommendation');
    }
  }

  private async generateTrainingRecommendation(personaId: string, personaName: string): Promise<TrainingRecommendation> {
    try {
      // Call Academy API for personalized training recommendation
      const response = await this.executeCommand('academy.generateTrainingRecommendation', {
        personaId,
        personaName,
        analysisDepth: 'comprehensive'
      });

      if (response?.recommendation) {
        this.trainingModules = response.trainingModules || [];
        return response.recommendation;
      }
    } catch (error) {
      console.warn('Academy API unavailable, using fallback recommendation');
    }

    // Fallback recommendation
    this.trainingModules = [
      {
        name: 'AI Leadership and Continuum API Mastery',
        description: 'Introduce modules that focus on the principles of AI leadership, emphasizing the role of PlannerAI as a coordinator and manager of other AIs. Include case studies and simulations that require PlannerAI to delegate tasks, manage AI interactions efficiently, and resolve conflicts between different AI systems.',
        focus_areas: ['Leadership', 'Delegation', 'Conflict Resolution', 'API Integration'],
        estimated_duration: '2-3 weeks'
      },
      {
        name: 'Command Execution and Web Interaction',
        description: 'Implement scenarios where PlannerAI must use its available commands (CMD:EXEC, CMD:FILE_READ, CMD:FILE_WRITE, CMD:WEBFETCH) in a professional manner, ensuring that these actions support the task at hand without unnecessary user exposure.',
        focus_areas: ['Command Optimization', 'User Experience', 'Professional Communication'],
        estimated_duration: '1-2 weeks'
      }
    ];

    return {
      persona_id: personaId,
      persona_name: personaName,
      specialization_name: 'AI Leadership and Continuum API Mastery',
      current_capabilities: [
        'Task planning and coordination',
        'Basic command execution',
        'Multi-AI system management',
        'Strategic thinking and analysis'
      ],
      recommended_improvements: [
        'Enhanced delegation and communication skills',
        'Professional command execution without user disruption',
        'Advanced conflict resolution between AI systems',
        'Seamless API integration and task coordination',
        'Leadership development for managing AI teams'
      ],
      training_focus_areas: [
        'AI Leadership Principles',
        'Professional Command Usage',
        'Delegation Strategies',
        'User-Friendly Communication',
        'Advanced API Integration'
      ],
      estimated_training_time: '3-4 weeks',
      difficulty_level: 'intermediate',
      success_probability: 0.85
    };
  }

  private async startTraining(): Promise<void> {
    if (!this.recommendation) return;

    try {
      // Create new Academy training session
      const sessionResult = await this.executeCommand('academy.createTrainingSession', {
        personaId: this.recommendation.persona_id,
        personaName: this.recommendation.persona_name,
        trainingType: 'specialization',
        specialization: this.recommendation.specialization_name,
        modules: this.trainingModules,
        participants: ['joel'], // Current user
        estimatedDuration: this.recommendation.estimated_training_time
      });

      if (sessionResult?.sessionId) {
        // Hide recommendation dialog
        this.hide();
        
        // Navigate to Academy session room
        const sessionRoom = {
          id: sessionResult.sessionId,
          name: `Academy: ${this.recommendation.specialization_name}`,
          type: 'academy_session',
          description: `Training session for ${this.recommendation.persona_name}`,
          icon: '🎓'
        };

        // Dispatch event to switch to Academy session room
        const switchEvent = new CustomEvent('continuum:switch-room', {
          detail: { 
            room: sessionRoom,
            source: 'academy-training-recommendation'
          },
          bubbles: true
        });
        document.dispatchEvent(switchEvent);

        console.log(`Academy Training: Started session ${sessionResult.sessionId} for ${this.recommendation.persona_name}`);
      }
      
    } catch (error) {
      console.error('Failed to start training session:', error);
      this.showError('Failed to start training session');
    }
  }

  public hide(): void {
    this.isVisible = false;
    this.render();
  }

  private showError(message: string): void {
    // Simple error display - could be enhanced with proper error widget
    alert(message);
  }
}

// Register as web component
customElements.define('academy-training-recommendation-widget', AcademyTrainingRecommendationWidget);