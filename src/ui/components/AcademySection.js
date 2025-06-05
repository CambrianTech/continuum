/**
 * Academy Section Web Component
 * Shows Academy training progress and controls
 */

class AcademySection extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.academyStatus = {
      activeTraining: [],
      completed: [],
      stats: {
        totalPersonas: 0,
        activeTraining: 0,
        graduated: 0,
        failed: 0
      }
    };
    this.onSendSheriff = null;
    this.onTrainCustom = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  setupEventListeners() {
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.send-sheriff-btn')) {
        this.handleSendSheriff();
      }
      
      if (e.target.closest('.train-custom-btn')) {
        this.handleTrainCustom();
      }
      
      if (e.target.closest('.training-persona-name')) {
        const personaName = e.target.closest('.training-persona-name').dataset.personaName;
        this.toggleTrainingDetails(personaName);
      }
    });
  }

  handleSendSheriff() {
    if (this.onSendSheriff) {
      this.onSendSheriff();
    }
    
    this.dispatchEvent(new CustomEvent('send-sheriff', {
      bubbles: true
    }));
  }

  handleTrainCustom() {
    if (this.onTrainCustom) {
      this.onTrainCustom();
    }
    
    this.dispatchEvent(new CustomEvent('train-custom', {
      bubbles: true
    }));
  }

  toggleTrainingDetails(personaName) {
    const details = this.shadowRoot.getElementById(`details-${personaName}`);
    if (details) {
      details.style.display = details.style.display === 'none' ? 'block' : 'none';
    }
  }

  generateTrainingPersonaHTML(session) {
    return `
      <div class="training-persona">
        <div class="persona-header">
          <span class="training-persona-name" data-persona-name="${session.personaName}">
            ${session.personaName}
          </span>
          <span class="persona-status">${session.status.replace(/_/g, ' ')}</span>
        </div>
        
        <div class="progress-container">
          <div class="progress-bar" style="width: ${session.progress}%;"></div>
        </div>
        
        <div class="persona-stats">
          <span>Round ${session.currentRound}/${session.totalRounds}</span>
          <span>Accuracy: ${(session.graduationScore * 100).toFixed(1)}%</span>
        </div>
        
        ${session.intensityMultiplier && session.intensityMultiplier > 1 ? `
          <div class="intensity-info">
            <span>${session.trainingIntensity.replace('_', ' ').toUpperCase()} (${session.intensityMultiplier}x)</span>
            <span>Effective: ${session.effectiveRounds?.toLocaleString() || 'N/A'} rounds</span>
          </div>
        ` : ''}
        
        <div class="training-details" id="details-${session.personaName}">
          <div class="detail-item">
            <strong>Specialization:</strong> ${session.specialization}
          </div>
          <div class="detail-item">
            <strong>Started:</strong> ${session.startTime.toLocaleTimeString()}
          </div>
          <div class="detail-item">
            <strong>Recent Activity:</strong>
          </div>
          <div class="activity-log">
            ${session.logs.slice(-3).map(log => `<div class="log-entry">${log}</div>`).join('')}
          </div>
        </div>
      </div>
    `;
  }

  generateCompletedPersonaHTML(session) {
    const isGraduated = session.status === 'graduated';
    return `
      <div class="completed-persona ${isGraduated ? 'graduated' : 'failed'}">
        <div class="completed-header">
          <span class="completed-name">${session.personaName}</span>
          <span class="completed-status">
            ${isGraduated ? '🎓 Graduated' : '❌ Failed'}
          </span>
        </div>
        <div class="completed-details">
          ${session.specialization} • ${(session.graduationScore * 100).toFixed(1)}% accuracy
          ${session.fineTuneId ? ` • LoRA: ${session.fineTuneId.substring(0, 20)}...` : ''}
        </div>
      </div>
    `;
  }

  render() {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          margin-top: 30px;
          border-top: 1px solid #333;
          padding-top: 20px;
        }

        .academy-header {
          display: flex;
          align-items: center;
          margin-bottom: 20px;
        }

        .academy-title {
          color: #4CAF50;
          margin: 0;
          margin-right: 15px;
          font-size: 1.5em;
          font-weight: 600;
        }

        .academy-stats {
          font-size: 0.9em;
          color: #888;
        }

        .training-persona {
          background: #2a2a2a;
          border-radius: 8px;
          padding: 15px;
          margin-bottom: 10px;
          border-left: 4px solid #FF9800;
        }

        .persona-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 10px;
        }

        .training-persona-name {
          font-weight: bold;
          color: #e0e0e0;
          text-decoration: underline;
          cursor: pointer;
        }

        .persona-status {
          color: #FF9800;
          font-size: 0.9em;
        }

        .progress-container {
          background: #1a1a1a;
          border-radius: 10px;
          height: 8px;
          margin-bottom: 10px;
          overflow: hidden;
        }

        .progress-bar {
          background: #FF9800;
          height: 100%;
          border-radius: 10px;
          transition: width 0.3s ease;
        }

        .persona-stats {
          display: flex;
          justify-content: space-between;
          font-size: 0.85em;
          color: #888;
        }

        .intensity-info {
          display: flex;
          justify-content: space-between;
          font-size: 0.8em;
          color: #FF9800;
          margin-top: 5px;
        }

        .training-details {
          display: none;
          margin-top: 15px;
          padding-top: 15px;
          border-top: 1px solid #444;
        }

        .detail-item {
          font-size: 0.85em;
          color: #ccc;
          margin-bottom: 5px;
        }

        .activity-log {
          max-height: 100px;
          overflow-y: auto;
          margin-top: 5px;
        }

        .log-entry {
          margin: 2px 0;
          color: #aaa;
          font-size: 0.8em;
        }

        .empty-state {
          text-align: center;
          color: #666;
          padding: 20px;
          font-style: italic;
        }

        .completed-section {
          margin-top: 20px;
        }

        .completed-title {
          color: #888;
          font-size: 1em;
          margin-bottom: 10px;
        }

        .completed-persona {
          background: #2a2a2a;
          border-radius: 6px;
          padding: 10px;
          margin-bottom: 8px;
        }

        .completed-persona.graduated {
          border-left: 4px solid #4CAF50;
        }

        .completed-persona.failed {
          border-left: 4px solid #f44336;
        }

        .completed-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .completed-name {
          font-weight: bold;
          color: #e0e0e0;
        }

        .completed-status {
          font-size: 0.85em;
        }

        .graduated .completed-status {
          color: #4CAF50;
        }

        .failed .completed-status {
          color: #f44336;
        }

        .completed-details {
          font-size: 0.8em;
          color: #888;
          margin-top: 5px;
        }

        .academy-actions {
          margin-top: 20px;
          text-align: center;
        }

        .action-btn {
          background: #4CAF50;
          border: none;
          color: white;
          border-radius: 6px;
          padding: 10px 16px;
          font-size: 14px;
          cursor: pointer;
          transition: all 0.2s ease;
          margin: 0 5px;
        }

        .action-btn:hover {
          transform: translateY(-1px);
          box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
        }

        .train-custom-btn {
          background: #2196F3;
        }
      </style>

      <div class="academy-header">
        <h2 class="academy-title">🎓 Academy</h2>
        <div class="academy-stats">
          ${this.academyStatus.stats.activeTraining} training • 
          ${this.academyStatus.stats.graduated} graduated • 
          ${this.academyStatus.stats.failed} failed
        </div>
      </div>
      
      <div class="active-training">
        ${this.academyStatus.activeTraining.length > 0 ? 
          this.academyStatus.activeTraining.map(session => this.generateTrainingPersonaHTML(session)).join('') :
          '<div class="empty-state">No personas currently training. Send a sheriff to the Academy!</div>'
        }
      </div>
      
      ${this.academyStatus.completed.length > 0 ? `
        <div class="completed-section">
          <h3 class="completed-title">Recent Graduates</h3>
          ${this.academyStatus.completed.map(session => this.generateCompletedPersonaHTML(session)).join('')}
        </div>
      ` : ''}
      
      <div class="academy-actions">
        <button class="action-btn send-sheriff-btn">
          🛡️ Send Sheriff to Academy
        </button>
        <button class="action-btn train-custom-btn">
          🎓 Train Custom Persona
        </button>
      </div>
    `;
  }

  // Public API
  updateAcademyStatus(status) {
    this.academyStatus = status;
    this.render();
  }

  setOnSendSheriff(callback) {
    this.onSendSheriff = callback;
  }

  setOnTrainCustom(callback) {
    this.onTrainCustom = callback;
  }
}

// Register the custom element
if (typeof customElements !== 'undefined') {
  customElements.define('academy-section', AcademySection);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = AcademySection;
}