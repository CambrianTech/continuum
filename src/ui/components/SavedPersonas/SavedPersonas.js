/**
 * Saved Personas Widget
 * Modular persona management widget extending SidebarWidget
 */

// Import sidebar widget functionality
import('../shared/SidebarWidget.js');

if (!customElements.get('saved-personas')) {

class SavedPersonas extends SidebarWidget {
  constructor() {
    super();
    
    // Widget metadata
    this.widgetName = 'SavedPersonas';
    this.widgetIcon = '👤';
    this.widgetCategory = 'User Interface';
    
    // Personas-specific state
    this.personas = [];
    this.selectedPersona = null;
  }

  async render() {
    const headerTitle = this.getAttribute('title') || 'Saved Personas';
    
    const content = `
      <div class="persona-list" id="persona-list">
        ${this.renderPersonaList()}
      </div>
      <!-- Auto-refresh via WebSocket, no manual button needed -->
    `;
    
    // Load CSS content first
    const cssContent = await this.loadCSSViaWebSocket();
    
    this.shadowRoot.innerHTML = `
      <style>
        ${this.getHeaderStyle()}
        ${cssContent}
      </style>
      
      ${this.renderSidebarStructure(headerTitle, content)}
    `;
    
    // Setup event listeners after DOM is ready
    await this.setupAsyncEventListeners();
  }

  async setupAsyncEventListeners() {
    // Wait a tick to ensure DOM is ready
    await new Promise(resolve => setTimeout(resolve, 10));
    
    const personaList = this.shadowRoot.getElementById('persona-list');
    
    if (personaList) {
      console.log('🎛️ SavedPersonas: Setting up event listeners for persona list');
      personaList.addEventListener('click', (e) => this.handlePersonaClick(e));
      personaList.addEventListener('mousedown', (e) => this.handleThresholdDragStart(e));
      
      // Verify threshold markers are present
      const markers = this.shadowRoot.querySelectorAll('.threshold-marker');
      console.log(`🎛️ SavedPersonas: Found ${markers.length} threshold markers`);
    } else {
      console.warn('🎛️ SavedPersonas: Could not find persona-list element for event listeners');
    }
  }

  async loadCSSViaWebSocket() {
    try {
      // Wait for WebSocket to be ready
      
      // Wait for WebSocket connection or timeout
      const waitForWebSocket = () => {
        return new Promise((resolve) => {
          if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            resolve(true);
            return;
          }
          let attempts = 0;
          const maxAttempts = 20; // 2 seconds max wait
          
          const checkWebSocket = () => {
            attempts++;
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
              resolve(true);
            } else if (attempts >= maxAttempts) {
              resolve(false);
            } else {
              setTimeout(checkWebSocket, 100);
            }
          };
          
          checkWebSocket();
        });
      };
      
      const wsReady = await waitForWebSocket();
      
      if (wsReady && window.ws && window.ws.readyState === WebSocket.OPEN) {
        
        return new Promise((resolve) => {
          let resolved = false;
          
          // Set up one-time listener for the response
          const responseHandler = (event) => {
            try {
              const data = JSON.parse(event.data);
              
              if (data.type === 'component_css_response' && data.component === 'SavedPersonas') {
                if (!resolved) {
                  resolved = true;
                  console.log('📄 SavedPersonas: CSS loaded via WebSocket, length:', data.css?.length);
                  window.ws.removeEventListener('message', responseHandler);
                  clearTimeout(timeoutId);
                  
                  if (data.css) {
                    resolve(data.css);
                  } else {
                    console.warn('📄 SavedPersonas: CSS loading failed:', data.error);
                    resolve(this.getMinimalStyles());
                  }
                }
              }
            } catch (parseError) {
              // Ignore non-JSON messages
            }
          };
          
          window.ws.addEventListener('message', responseHandler);
          
          // Send CSS request
          const cssRequest = {
            type: 'get_component_css',
            component: 'SavedPersonas',
            path: '/ui/components/SavedPersonas/SavedPersonas.css'
          };
          
          window.ws.send(JSON.stringify(cssRequest));
          
          // Timeout fallback
          const timeoutId = setTimeout(() => {
            if (!resolved) {
              resolved = true;
              console.warn('📄 SavedPersonas: CSS request timeout, using fallback');
              window.ws.removeEventListener('message', responseHandler);
              resolve(this.getMinimalStyles());
            }
          }, 3000);
        });
      }
    } catch (error) {
      console.warn('WebSocket CSS loading failed:', error);
    }
    
    // Fallback: use minimal styles
    return this.getMinimalStyles();
  }

  getMinimalStyles() {
    return `
      /* Minimal fallback styles - all main styles should be in SavedPersonas.css */
      .persona-card { 
        background: #1a1a1a; 
        border: 1px solid #333; 
        border-radius: 4px; 
        padding: 8px; 
        margin-bottom: 8px; 
      }
    `;
  }





  renderPersonaList() {
    console.log('🎭 SavedPersonas: Rendering personas:', this.personas);
    
    if (this.isLoading) {
      return `<div class="loading">${this.loadingMessage || 'Loading personas...'}</div>`;
    }
    
    if (this.hasError) {
      return `<div class="error">${this.errorMessage}</div>`;
    }
    
    if (this.personas.length === 0) {
      return `
        <div class="empty-state">
          <div class="empty-icon">👤</div>
          <div>No personas found</div>
        </div>
      `;
    }
    
    return this.personas.map(persona => `
      <div class="persona-card ${persona.id === this.selectedPersona?.id ? 'selected' : ''}" 
           data-persona-id="${persona.id}">
        <div class="persona-header">
          <div class="persona-name">${this.formatPersonaName(persona.name)}</div>
          <div class="persona-status ${persona.status || 'unknown'}" 
               ${persona.status === 'training' ? 'data-action="goto-academy" data-persona-id="' + persona.id + '" title="Go to Academy"' : ''}>
            ${this.formatStatus(persona.status)}
          </div>
        </div>
        
        <div class="persona-specialization">${(persona.specialization || 'general').replace(/_/g, ' ')}</div>
        
        ${this.renderAcademyProgress(persona)}
        
        <div class="persona-actions">
          <button class="persona-action deploy" data-action="deploy" data-persona-id="${persona.id}">✅ DEPLOY</button>
          <button class="persona-action retrain" data-action="retrain" data-persona-id="${persona.id}">🔄 RETRAIN</button>
          <button class="persona-action share" data-action="share" data-persona-id="${persona.id}">🔗 SHARE ORG</button>
        </div>
      </div>
    `).join('');
  }

  formatPersonaName(name) {
    // Clean up ugly auto-generated names
    if (name.includes('fine-tune-test-')) {
      return 'Fine-Tune Test';
    }
    if (name.includes('test-lawyer-')) {
      return 'Legal Test';
    }
    // Truncate very long names
    if (name.length > 20) {
      return name.substring(0, 17) + '...';
    }
    return name;
  }

  formatStatus(status) {
    if (status === 'training') {
      return 'IN ACADEMY »';
    }
    if (status === 'failed') {
      return 'FAILED ⚠️';
    }
    return (status || 'unknown').toUpperCase();
  }

  normalizeScore(score) {
    // Handle cases where score might be 92 (meaning 92%) or 0.92 (meaning 92%)
    if (score > 1) {
      return Math.min(100, score).toFixed(1);
    } else {
      return (score * 100).toFixed(1);
    }
  }

  getThresholdColor(threshold, isDark = false) {
    const thresholdValue = parseFloat(threshold);
    
    if (thresholdValue >= 85) {
      return isDark ? 'rgba(76, 175, 80, 0.6)' : 'rgba(76, 175, 80, 0.3)'; // Green
    } else if (thresholdValue >= 75) {
      return isDark ? 'rgba(139, 195, 74, 0.6)' : 'rgba(139, 195, 74, 0.3)'; // Green-yellow
    } else if (thresholdValue >= 65) {
      return isDark ? 'rgba(255, 193, 7, 0.6)' : 'rgba(255, 193, 7, 0.3)'; // Yellow
    } else if (thresholdValue >= 50) {
      return isDark ? 'rgba(255, 152, 0, 0.6)' : 'rgba(255, 152, 0, 0.3)'; // Orange
    } else {
      return isDark ? 'rgba(244, 67, 54, 0.6)' : 'rgba(244, 67, 54, 0.3)'; // Red
    }
  }

  getThresholdQuality(threshold) {
    const thresholdValue = parseFloat(threshold);
    
    if (thresholdValue >= 85) {
      return 'Excellent';
    } else if (thresholdValue >= 75) {
      return 'Good';
    } else if (thresholdValue >= 65) {
      return 'Fair';
    } else if (thresholdValue >= 50) {
      return 'Poor';
    } else if (thresholdValue >= 25) {
      return 'Weak';
    } else {
      return 'Awful';
    }
  }

  getThresholdTextColor(threshold) {
    const thresholdValue = parseFloat(threshold);
    
    if (thresholdValue >= 85) {
      return '#4CAF50'; // Green
    } else if (thresholdValue >= 75) {
      return '#8BC34A'; // Green-yellow
    } else if (thresholdValue >= 65) {
      return '#FFC107'; // Yellow
    } else if (thresholdValue >= 50) {
      return '#FF9800'; // Orange
    } else {
      return '#F44336'; // Red
    }
  }

  renderAcademyProgress(persona) {
    const isInAcademy = ['training', 'in_academy', 'learning'].includes(persona.status);
    
    console.log('Persona status:', persona.status, 'isInAcademy:', isInAcademy);
    
    if (isInAcademy) {
      return this.renderActiveAcademyProgress(persona);
    } else if (persona.graduationScore !== undefined) {
      return this.renderCompletedAcademyProgress(persona);
    }
    
    return '';
  }

  renderActiveAcademyProgress(persona) {
    const currentScore = this.normalizeScore(persona.graduationScore || persona.currentScore || 0);
    const threshold = this.normalizeScore(persona.threshold || persona.graduationThreshold || 75);
    const currentIteration = persona.currentIteration || 1;
    const totalIterations = persona.totalIterations || 10;
    const iterationProgress = (currentIteration / totalIterations) * 100;

    return `
      <div class="academy-progress-container">
        <!-- Current Score vs Threshold -->
        <div class="score-progress">
          <div class="progress-header">
            <span class="score-label">Score: ${currentScore}%</span>
            <span class="threshold-label" data-persona-id="${persona.id}" style="color: ${this.getThresholdTextColor(threshold)};">Target: ${threshold}% (${this.getThresholdQuality(threshold)})</span>
          </div>
          <div class="progress-bar score-bar" data-persona-id="${persona.id}">
            <div class="progress-fill" style="width: ${currentScore}%;"></div>
            <div class="threshold-background" style="background-color: ${this.getThresholdColor(threshold, true)}; width: ${threshold}%;"></div>
            <div class="threshold-marker" 
                 style="left: ${threshold}%" 
                 data-threshold="${threshold}"
                 data-persona-id="${persona.id}"
                 title="Drag to adjust threshold">
              ↔
            </div>
          </div>
        </div>
        
        <!-- Training Iterations Progress -->
        <div class="iteration-progress">
          <div class="progress-bar iteration-bar">
            <div class="progress-fill iteration-fill" style="width: ${iterationProgress}%"></div>
          </div>
          <div class="iteration-text">Iteration ${currentIteration}/${totalIterations}</div>
        </div>
        
        <!-- Quick Controls -->
        <div class="academy-controls">
          <button class="academy-control-btn retrain-needed ${this.hasThresholdChanged(persona) ? 'visible' : 'hidden'}" 
                  data-action="retrain-needed" data-persona-id="${persona.id}" 
                  title="Threshold changed - retrain needed">
            🔄 RETRAIN NEEDED
          </button>
        </div>
      </div>
    `;
  }

  calculateTotalAcademyProgress(persona) {
    // Combine iteration progress and score achievement for overall academy progress
    const currentIteration = persona.currentIteration || 1;
    const totalIterations = persona.totalIterations || 10;
    const currentScore = this.normalizeScore(persona.graduationScore || persona.currentScore || 0);
    const threshold = this.normalizeScore(persona.threshold || persona.graduationThreshold || 75);
    
    const iterationProgress = (currentIteration / totalIterations) * 100;
    const scoreProgress = Math.min(100, (parseFloat(currentScore) / parseFloat(threshold)) * 100);
    
    // Weight: 60% iteration completion, 40% score achievement
    return Math.round((iterationProgress * 0.6) + (scoreProgress * 0.4));
  }

  renderCompletedAcademyProgress(persona) {
    const finalScore = this.normalizeScore(persona.graduationScore);
    const threshold = this.normalizeScore(persona.threshold || persona.graduationThreshold || 75);
    const passed = parseFloat(finalScore) >= parseFloat(threshold);
    
    // Special handling for failed personas
    if (persona.status === 'failed') {
      return `
        <div class="progress-container">
          <div class="progress-header">
            <span class="score-label">Score: ${finalScore}%</span>
            <span class="threshold-label" data-persona-id="${persona.id}" style="color: ${this.getThresholdTextColor(threshold)};">Target: ${threshold}% (${this.getThresholdQuality(threshold)})</span>
          </div>
          <div class="progress-bar score-bar" data-persona-id="${persona.id}">
            <div class="progress-fill" style="width: ${finalScore}%; background: #F44336;"></div>
            <div class="threshold-background" style="background-color: ${this.getThresholdColor(threshold, true)}; width: ${threshold}%;"></div>
            <div class="threshold-marker" 
                 style="left: ${threshold}%" 
                 data-threshold="${threshold}"
                 data-persona-id="${persona.id}"
                 title="Drag to adjust threshold for retry">
              ↔
            </div>
          </div>
          ${persona.failureReason ? `<div class="failure-reason">${persona.failureReason}</div>` : ''}
        </div>
      `;
    }

    return `
      <div class="progress-container">
        <div class="progress-bar">
          <div class="progress-fill" style="width: ${finalScore}%; background: ${passed ? '#00d4ff' : '#FFC107'};"></div>
        </div>
        <div class="progress-text">${finalScore}% Academy Score</div>
      </div>
    `;
  }

  setupEventListeners() {
    super.setupEventListeners(); // CRITICAL: Enable collapse functionality
    
    const personaList = this.shadowRoot.getElementById('persona-list');
    
    if (personaList) {
      personaList.addEventListener('click', (e) => this.handlePersonaClick(e));
      personaList.addEventListener('mousedown', (e) => this.handleThresholdDragStart(e));
    }
    
    // Listen for WebSocket events
    this.setupWebSocketListeners();
  }
  
  setupWebSocketListeners() {
    // Connect to continuum WebSocket for real-time updates
    if (typeof window !== 'undefined' && window.ws) {
      window.ws.addEventListener('message', (event) => {
        try {
          const data = JSON.parse(event.data);
          if (data.type === 'personas_updated' || data.type === 'persona_added' || data.type === 'persona_deleted') {
            console.log('🎛️ SavedPersonas: WebSocket update received', data.type);
            this.refresh(); // Auto-refresh when personas change
          }
        } catch (error) {
          // Ignore non-JSON messages
        }
      });
    }
  }

  handlePersonaClick(event) {
    const personaItem = event.target.closest('.persona-item');
    const actionBtn = event.target.closest('.persona-action');
    
    if (actionBtn) {
      event.stopPropagation();
      const action = actionBtn.dataset.action;
      const personaId = actionBtn.dataset.personaId;
      this.handlePersonaAction(action, personaId);
      return;
    }
    
    if (personaItem) {
      const personaId = personaItem.dataset.personaId;
      this.selectPersona(personaId);
    }
  }

  selectPersona(personaId) {
    const persona = this.personas.find(p => p.id === personaId);
    if (persona) {
      this.selectedPersona = persona;
      this.updatePersonaList();
      
      // Dispatch custom event for parent components
      this.dispatchEvent(new CustomEvent('persona-selected', {
        detail: { persona },
        bubbles: true
      }));
    }
  }

  async handlePersonaAction(action, personaId) {
    try {
      switch (action) {
        case 'share':
          await this.sharePersona(personaId);
          break;
        case 'deploy':
          await this.deployPersona(personaId);
          break;
        case 'delete':
          await this.deletePersona(personaId);
          break;
      }
    } catch (error) {
      console.error(`Persona action ${action} failed:`, error);
      this.setError(true, `Failed to ${action} persona`);
    }
  }

  async sharePersona(personaId) {
    const response = await this.apiCall(`/api/personas/${personaId}/share`, {
      method: 'POST'
    });
    console.log('Persona shared:', response);
  }

  async deployPersona(personaId) {
    const response = await this.apiCall(`/api/personas/${personaId}/deploy`, {
      method: 'POST'
    });
    console.log('Persona deployed:', response);
  }

  async deletePersona(personaId) {
    if (confirm('Are you sure you want to delete this persona?')) {
      await this.apiCall(`/api/personas/${personaId}`, {
        method: 'DELETE'
      });
      await this.refresh(); // Refresh list after deletion
    }
  }

  handleThresholdDragStart(event) {
    console.log('🎛️ SavedPersonas: handleThresholdDragStart triggered', event.target);
    const thresholdMarker = event.target.closest('.threshold-marker');
    if (!thresholdMarker) {
      console.log('🎛️ SavedPersonas: No threshold marker found');
      return;
    }

    console.log('🎛️ SavedPersonas: Starting threshold drag', thresholdMarker.dataset.personaId);
    event.preventDefault();
    const personaId = thresholdMarker.dataset.personaId;
    const progressBar = thresholdMarker.closest('.score-bar');
    
    this.dragState = {
      isDragging: true,
      personaId: personaId,
      progressBar: progressBar,
      thresholdMarker: thresholdMarker,
      startX: event.clientX,
      barRect: progressBar.getBoundingClientRect()
    };

    thresholdMarker.classList.add('dragging');
    
    // Add global listeners
    document.addEventListener('mousemove', this.handleThresholdDrag.bind(this));
    document.addEventListener('mouseup', this.handleThresholdDragEnd.bind(this));
  }

  handleThresholdDrag(event) {
    if (!this.dragState?.isDragging) return;

    const { progressBar, thresholdMarker, barRect, personaId } = this.dragState;
    const mouseX = event.clientX;
    const relativeX = mouseX - barRect.left;
    const percentage = Math.max(0, Math.min(100, (relativeX / barRect.width) * 100));

    // Update visual position
    thresholdMarker.style.left = `${percentage}%`;
    thresholdMarker.dataset.threshold = percentage.toFixed(1);
    
    // Update threshold background color and width (darker version)
    const thresholdBackground = progressBar.querySelector('.threshold-background');
    if (thresholdBackground) {
      thresholdBackground.style.backgroundColor = this.getThresholdColor(percentage.toFixed(1), true);
      thresholdBackground.style.width = `${percentage}%`;
    }
    
    // Update threshold label with color and quality text
    const scoreProgress = progressBar.closest('.score-progress');
    if (scoreProgress) {
      const thresholdLabel = scoreProgress.querySelector('.threshold-label');
      if (thresholdLabel) {
        thresholdLabel.textContent = `Target: ${percentage.toFixed(1)}% (${this.getThresholdQuality(percentage.toFixed(1))})`;
        thresholdLabel.style.color = this.getThresholdTextColor(percentage.toFixed(1));
      }
    }
    
    // Also try selecting by persona ID
    const labelByPersonaId = progressBar.closest('.persona-card').querySelector(`.threshold-label[data-persona-id="${personaId}"]`);
    if (labelByPersonaId) {
      labelByPersonaId.textContent = `Target: ${percentage.toFixed(1)}% (${this.getThresholdQuality(percentage.toFixed(1))})`;
      labelByPersonaId.style.color = this.getThresholdTextColor(percentage.toFixed(1));
    }
  }

  handleThresholdDragEnd(event) {
    if (!this.dragState?.isDragging) return;

    const { thresholdMarker, personaId, barRect } = this.dragState;
    const mouseX = event.clientX;
    const relativeX = mouseX - barRect.left;
    const newThreshold = Math.max(0, Math.min(100, (relativeX / barRect.width) * 100));

    // Remove dragging state
    thresholdMarker.classList.remove('dragging');
    
    // Update persona threshold
    this.updatePersonaThreshold(personaId, newThreshold);

    // Cleanup
    document.removeEventListener('mousemove', this.handleThresholdDrag.bind(this));
    document.removeEventListener('mouseup', this.handleThresholdDragEnd.bind(this));
    this.dragState = null;
  }

  hasThresholdChanged(persona) {
    const currentThreshold = parseFloat(persona.threshold || 75);
    const originalThreshold = parseFloat(persona.originalThreshold || persona.threshold || 75);
    return Math.abs(currentThreshold - originalThreshold) > 0.1;
  }

  async updatePersonaThreshold(personaId, newThreshold) {
    try {
      // For fake personas, just update locally
      if (personaId.startsWith('fake-')) {
        const persona = this.personas.find(p => p.id === personaId);
        if (persona) {
          persona.threshold = newThreshold;
          console.log(`Updated fake persona threshold to ${newThreshold.toFixed(1)}%`);
          this.updatePersonaList(); // Re-render to show retrain button if needed
        }
        return;
      }

      await this.apiCall(`/api/personas/${personaId}/threshold`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ threshold: newThreshold })
      });
      
      console.log(`Updated threshold for persona ${personaId} to ${newThreshold.toFixed(1)}%`);
    } catch (error) {
      console.error('Failed to update threshold:', error);
      // Revert visual change on error
      await this.refresh();
    }
  }

  async onRefresh() {
    console.log('🔄 SavedPersonas: Refreshing...');
    try {
      const apiPersonas = await this.apiCall('/api/personas');
      console.log('📡 SavedPersonas: Got API personas:', apiPersonas);
      
      // Add fake training persona to test controls
      const fakePersonas = this.getMockPersonas();
      this.personas = [...fakePersonas, ...apiPersonas];
      
      console.log('🎭 SavedPersonas: Combined personas (fake + real):', this.personas.length);
      this.updatePersonaList();
    } catch (error) {
      console.log('❌ SavedPersonas: API failed, using mock data only:', error.message);
      this.personas = this.getMockPersonas();
      this.updatePersonaList();
    }
  }

  getMockPersonas() {
    // Add fake personas to test different UI states
    return [
      {
        id: 'fake-training-test',
        name: 'FAKE - Training Test',
        status: 'training',
        specialization: 'ui_testing', 
        graduationScore: 68,
        currentScore: 68,
        threshold: 75,
        originalThreshold: 75,
        currentIteration: 5,
        totalIterations: 8
      },
      {
        id: 'fake-failed-test',
        name: 'FAKE - Failed Test',
        status: 'failed',
        specialization: 'failure_analysis',
        graduationScore: 45,
        threshold: 75,
        originalThreshold: 75,
        failureReason: 'Low performance on protocol validation',
        attempts: 3
      }
    ];
  }

  updatePersonaList() {
    const personaList = this.shadowRoot.getElementById('persona-list');
    if (personaList) {
      personaList.innerHTML = this.renderPersonaList();
    }
  }

  updateLoadingState() {
    this.updatePersonaList();
  }

  updateErrorState() {
    this.updatePersonaList();
  }

  initializeWidget() {
    // Start auto-refresh every 30 seconds
    this.startAutoRefresh(30000);
    
    // Initial load
    this.refresh();
  }
}

// Register the custom element
customElements.define('saved-personas', SavedPersonas);

// Export for module systems
if (typeof module !== 'undefined' && module.exports) {
  module.exports = SavedPersonas;
} else if (typeof window !== 'undefined') {
  window.SavedPersonas = SavedPersonas;
}

} // End guard