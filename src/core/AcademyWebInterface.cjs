/**
 * Academy Web Interface - Shows training progress and Academy status
 */

const Academy = require('./Academy.cjs');
const { ModelRegistry } = require('./AIModel.cjs');
const ModelCaliber = require('./ModelCaliber.cjs');
const LoRAAdapter = require('./LoRAAdapter.cjs');
const { ModelAdapterFactory } = require('./ModelAdapter.cjs');
const PersistentStorage = require('./PersistentStorage.cjs');

class AcademyWebInterface {
  constructor(continuum) {
    this.continuum = continuum;
    this.academy = new Academy(new ModelRegistry(), new ModelCaliber());
    this.trainingPersonas = new Map(); // Track active training sessions
    this.completedPersonas = new Map(); // Track completed personas
    this.storage = new PersistentStorage();
    
    // Load previous Academy sessions on startup
    this.loadAcademyData();
  }
  
  /**
   * Load Academy training data from persistent storage
   */
  loadAcademyData() {
    const data = this.storage.load('academy-sessions.json', {
      defaultValue: {},
      dateFields: ['startTime', 'completedAt'],
      verbose: true
    });
    
    if (data && data.completedPersonas) {
      for (const [key, session] of Object.entries(data.completedPersonas)) {
        this.completedPersonas.set(key, session);
      }
      console.log(`🎓 Restored ${this.completedPersonas.size} Academy training sessions`);
    }
  }
  
  /**
   * Save Academy training data to persistent storage
   */
  saveAcademyData() {
    const data = {
      completedPersonas: Object.fromEntries(this.completedPersonas)
    };
    
    const success = this.storage.save('academy-sessions.json', data, {
      source: 'AcademyWebInterface',
      version: '1.0.0'
    });
    
    if (success) {
      console.log(`🎓 Persisted ${this.completedPersonas.size} Academy training sessions`);
    }
  }

  /**
   * Start Academy training for a persona
   */
  async startAcademyTraining(personaName, specialization = 'protocol_enforcement', options = {}) {
    console.log(`🎓 Starting Academy training for ${personaName}`);
    
    const trainingSession = {
      personaName,
      specialization,
      status: 'enrolling',
      startTime: new Date(),
      progress: 0,
      currentRound: 0,
      totalRounds: options.rounds || 150,
      graduationScore: 0,
      customPrompt: options.customPrompt || null,
      trainingIntensity: options.trainingIntensity || 'normal',
      logs: [
        `🎓 Enrolling ${personaName} in Academy...`,
        ...(options.customPrompt ? [`🎯 Custom training focus: ${options.customPrompt.split('\n')[0]}`] : [])
      ]
    };
    
    this.trainingPersonas.set(personaName, trainingSession);
    
    // Notify all connected clients
    this.broadcastAcademyUpdate(personaName, trainingSession);
    
    // Start training in background
    this.runAcademyTraining(personaName, specialization, options);
    
    return trainingSession;
  }

  /**
   * Run Academy training in background
   */
  async runAcademyTraining(personaName, specialization, options = {}) {
    const session = this.trainingPersonas.get(personaName);
    
    try {
      // Update status to training
      session.status = 'training';
      session.logs.push(`🏋️ Starting adversarial boot camp training...`);
      this.broadcastAcademyUpdate(personaName, session);
      
      // Simulate Academy enrollment (simplified for demo)
      const recruit = { name: personaName, specialization, trainingData: [] };
      session.logs.push(`✅ ${personaName} enrolled for ${specialization} training`);
      this.broadcastAcademyUpdate(personaName, session);
      
      console.log(`🎓 Academy: Starting training simulation for ${personaName}`);
      
      // Run training rounds with progress updates
      const totalRounds = session.totalRounds;
      
      console.log(`🎓 Academy: Starting ${totalRounds} training rounds for ${personaName}`);
      
      for (let round = 1; round <= totalRounds; round++) {
        console.log(`🎓 Academy: Round ${round}/${totalRounds} for ${personaName}`);
        
        session.currentRound = round;
        session.progress = Math.round((round / totalRounds) * 100);
        session.status = `training_round_${round}`;
        // Add intensity-specific training messages
        const intensityMessages = {
          'normal': `🔥 Round ${round}/${totalRounds}: Standard adversarial training...`,
          'high': `⚡ Round ${round}/${totalRounds}: High-intensity training mode...`,
          'extreme': `🚀 Round ${round}/${totalRounds}: EXTREME training protocol activated...`,
          'gpu_low': `🖥️ Round ${round}/${totalRounds}: GPU-accelerated training (Low)...`,
          'gpu_medium': `🖥️ Round ${round}/${totalRounds}: GPU-accelerated training (Medium)...`,
          'gpu_high': `🖥️ Round ${round}/${totalRounds}: GPU-accelerated training (High)...`,
          'gpu_max': `🔥🖥️ Round ${round}/${totalRounds}: MAXIMUM GPU acceleration!...`
        };
        
        session.logs.push(intensityMessages[session.trainingIntensity] || `🔥 Round ${round}/${totalRounds}: Adversarial training in progress...`);
        
        this.broadcastAcademyUpdate(personaName, session);
        
        // Simulate training round with enhanced parameters
        await this.simulateTrainingRound(recruit, round, session);
        
        // Calculate current accuracy
        const accuracy = this.calculateAccuracy(recruit, round);
        session.graduationScore = accuracy;
        // Enhanced progress messages
        const roundData = recruit.trainingData[recruit.trainingData.length - 1];
        session.logs.push(`📊 Round ${round} completed: ${(accuracy * 100).toFixed(1)}% accuracy (${roundData.correctDetections}/${roundData.totalTests} tests passed)`);
        
        this.broadcastAcademyUpdate(personaName, session);
        
        // Dynamic wait time based on intensity (no additional wait for high intensity)
        const isGPUAccelerated = session.trainingIntensity.startsWith('gpu_');
        const waitTime = isGPUAccelerated ? 1000 : Math.max(1000, 3000 - (session.intensityMultiplier * 100));
        await this.sleep(waitTime);
      }
      
      // Check graduation
      const passingScore = options.passingScore || 0.85;
      if (session.graduationScore >= passingScore) {
        await this.graduatePersona(personaName, session);
      } else {
        await this.failPersona(personaName, session);
      }
      
    } catch (error) {
      session.status = 'error';
      session.logs.push(`❌ Training failed: ${error.message}`);
      this.broadcastAcademyUpdate(personaName, session);
    }
  }

  /**
   * Graduate a persona with LoRA adapter creation
   */
  async graduatePersona(personaName, session) {
    session.status = 'creating_adapter';
    session.logs.push(`🎓 ${personaName} graduated! Creating LoRA adapter...`);
    this.broadcastAcademyUpdate(personaName, session);
    
    try {
      // Create LoRA adapter
      if (process.env.OPENAI_API_KEY) {
        const adapter = ModelAdapterFactory.create('openai', process.env.OPENAI_API_KEY);
        
        // Mock training data for demo
        const trainingData = [
          {
            messages: [
              { role: "system", content: "You are a Protocol Sheriff." },
              { role: "user", content: "Validate response" },
              { role: "assistant", content: "Protocol validated successfully" }
            ]
          }
        ];
        
        const result = await adapter.fineTune('gpt-3.5-turbo', trainingData, {
          useLoRA: true,
          rank: 16,
          alpha: 32,
          suffix: personaName
        });
        
        session.fineTuneId = result.fineTuneId;
        session.storageReduction = result.storageReduction;
        session.logs.push(`🔬 LoRA adapter created: ${result.fineTuneId}`);
        session.logs.push(`💾 Storage reduction: ${Math.round(result.storageReduction).toLocaleString()}x`);
      }
      
      session.status = 'graduated';
      session.completedAt = new Date();
      session.logs.push(`🎉 ${personaName} is now a certified Academy graduate!`);
      session.logs.push(`📦 Ready for deployment and sharing`);
      
      // Move to completed personas
      this.completedPersonas.set(personaName, session);
      this.trainingPersonas.delete(personaName);
      
      // Save to persistent storage
      this.saveAcademyData();
      
      this.broadcastAcademyUpdate(personaName, session);
      
    } catch (error) {
      session.status = 'adapter_failed';
      session.logs.push(`⚠️ Adapter creation failed: ${error.message}`);
      session.logs.push(`📚 Persona graduated but needs manual adapter setup`);
      this.broadcastAcademyUpdate(personaName, session);
    }
  }

  /**
   * Handle persona failure
   */
  async failPersona(personaName, session) {
    session.status = 'failed';
    session.completedAt = new Date();
    session.logs.push(`❌ ${personaName} failed to meet graduation requirements`);
    session.logs.push(`📊 Final score: ${(session.graduationScore * 100).toFixed(1)}% (needed 85%)`);
    session.logs.push(`🔄 Can be re-enrolled for additional training`);
    
    // Move to completed personas (even failures are tracked)
    this.completedPersonas.set(personaName, session);
    this.trainingPersonas.delete(personaName);
    
    // Save to persistent storage
    this.saveAcademyData();
    
    this.broadcastAcademyUpdate(personaName, session);
  }

  /**
   * Simulate a training round with intensity multipliers
   */
  async simulateTrainingRound(recruit, round, session) {
    const intensityMultiplier = session?.intensityMultiplier || 1;
    const isGPUAccelerated = session?.trainingIntensity?.startsWith('gpu_');
    
    // Enhanced training algorithm with intensity multipliers
    // Base accuracy improved with more rounds and intensity
    const baseAccuracy = 0.3 + (round * 0.05); // Slower initial growth
    const intensityBonus = Math.log(intensityMultiplier) * 0.1; // Logarithmic intensity bonus
    const gpuBonus = isGPUAccelerated ? 0.15 : 0; // GPU acceleration bonus
    const randomFactor = (Math.random() * 0.1) - 0.05; // ±5% randomness
    
    const accuracy = Math.min(baseAccuracy + intensityBonus + gpuBonus + randomFactor, 0.98);
    
    if (!recruit.trainingData) recruit.trainingData = [];
    
    // More tests per round with higher intensity
    const testsPerRound = Math.max(5, Math.floor(intensityMultiplier / 2));
    const correctDetections = Math.floor(accuracy * testsPerRound);
    
    recruit.trainingData.push({
      round,
      testsGenerated: testsPerRound,
      correctDetections: correctDetections,
      totalTests: testsPerRound,
      accuracy,
      intensityMultiplier,
      isGPUAccelerated,
      timestamp: new Date().toISOString()
    });
    
    // Faster training with GPU acceleration
    const baseDelay = isGPUAccelerated ? 1000 : 2000;
    const delay = Math.max(500, baseDelay - (intensityMultiplier * 50));
    await this.sleep(delay + Math.random() * 500);
  }

  /**
   * Calculate current accuracy from training data
   */
  calculateAccuracy(recruit, currentRound) {
    if (!recruit.trainingData || recruit.trainingData.length === 0) return 0;
    
    const totalCorrect = recruit.trainingData.reduce((sum, round) => sum + round.correctDetections, 0);
    const totalTests = recruit.trainingData.reduce((sum, round) => sum + round.totalTests, 0);
    
    return totalTests > 0 ? totalCorrect / totalTests : 0;
  }

  /**
   * Broadcast Academy updates to all connected clients (INTERRUPT-DRIVEN)
   */
  broadcastAcademyUpdate(personaName, session) {
    if (this.continuum.webSocketServer) {
      // Send individual session update
      const sessionMessage = {
        type: 'academy_update',
        personaName,
        session: {
          ...session,
          logs: session.logs.slice(-10) // Only send last 10 log entries
        }
      };
      
      // Send complete status update (PUSH-BASED, not poll-based)
      const statusMessage = {
        type: 'academy_status_push',
        status: this.getAcademyStatus()
      };
      
      this.continuum.webSocketServer.broadcast(sessionMessage);
      this.continuum.webSocketServer.broadcast(statusMessage);
      console.log('🎓 PUSH: Academy status broadcasted to all clients');
    }
  }

  /**
   * Get Academy status for UI
   */
  getAcademyStatus() {
    console.log('🎓 AcademyWebInterface: getAcademyStatus() called');
    console.log('🎓 Training personas map size:', this.trainingPersonas.size);
    console.log('🎓 Completed personas map size:', this.completedPersonas.size);
    
    const activeTraining = Array.from(this.trainingPersonas.values());
    const completed = Array.from(this.completedPersonas.values());
    
    console.log('🎓 Active training array length:', activeTraining.length);
    console.log('🎓 Completed array length:', completed.length);
    
    if (activeTraining.length > 0) {
      console.log('🎓 Active training sessions:');
      activeTraining.forEach((session, index) => {
        console.log(`🎓   ${index}: ${session.personaName} - ${session.status} - ${session.progress}%`);
      });
    }
    
    const status = {
      activeTraining,
      completed: completed.slice(-5), // Last 5 completed
      stats: {
        totalPersonas: activeTraining.length + completed.length,
        activeTraining: activeTraining.length,
        graduated: completed.filter(p => p.status === 'graduated').length,
        failed: completed.filter(p => p.status === 'failed').length
      }
    };
    
    console.log('🎓 Returning status with:', 
      status.stats.activeTraining, 'active,', 
      status.stats.graduated, 'graduated,', 
      status.stats.failed, 'failed');
    
    return status;
  }

  /**
   * Generate Academy HTML for the web interface
   */
  generateAcademyHTML() {
    const status = this.getAcademyStatus();
    
    return `
      <!-- Academy Section -->
      <div class="academy-section" style="margin-top: 30px; border-top: 1px solid #333; padding-top: 20px;">
        <div class="academy-header" style="display: flex; align-items: center; margin-bottom: 20px;">
          <h2 style="color: #4CAF50; margin: 0; margin-right: 15px;">🎓 Academy</h2>
          <div class="academy-stats" style="font-size: 0.9em; color: #888;">
            ${status.stats.activeTraining} training • 
            ${status.stats.graduated} graduated • 
            ${status.stats.failed} failed
          </div>
        </div>
        
        <!-- Active Training -->
        <div class="active-training" id="active-training">
          ${status.activeTraining.map(session => `
            <div class="training-persona" style="background: #2a2a2a; border-radius: 8px; padding: 15px; margin-bottom: 10px; border-left: 4px solid #FF9800;">
              <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
                <span style="font-weight: bold; color: #e0e0e0; text-decoration: underline; cursor: pointer;" 
                      onclick="toggleTrainingDetails('${session.personaName}')">${session.personaName}</span>
                <span style="color: #FF9800; font-size: 0.9em;">${session.status.replace(/_/g, ' ')}</span>
              </div>
              
              <div class="progress-container" style="background: #1a1a1a; border-radius: 10px; height: 8px; margin-bottom: 10px;">
                <div class="progress-bar" style="background: #FF9800; height: 100%; border-radius: 10px; width: ${session.progress}%; transition: width 0.3s ease;"></div>
              </div>
              
              <div style="display: flex; justify-content: space-between; font-size: 0.85em; color: #888;">
                <span>Round ${session.currentRound}/${session.totalRounds}</span>
                <span>Accuracy: ${(session.graduationScore * 100).toFixed(1)}%</span>
              </div>
              
              ${session.intensityMultiplier && session.intensityMultiplier > 1 ? `
                <div style="display: flex; justify-content: space-between; font-size: 0.8em; color: #FF9800; margin-top: 5px;">
                  <span>${session.trainingIntensity.replace('_', ' ').toUpperCase()} (${session.intensityMultiplier}x)</span>
                  <span>Effective: ${session.effectiveRounds?.toLocaleString() || 'N/A'} rounds</span>
                </div>
              ` : ''}
              
              <div class="training-details" id="details-${session.personaName}" style="display: none; margin-top: 15px; padding-top: 15px; border-top: 1px solid #444;">
                <div style="font-size: 0.85em; color: #ccc;">
                  <div><strong>Specialization:</strong> ${session.specialization}</div>
                  <div><strong>Started:</strong> ${session.startTime.toLocaleTimeString()}</div>
                  <div style="margin-top: 10px;"><strong>Recent Activity:</strong></div>
                  <div style="max-height: 100px; overflow-y: auto; margin-top: 5px;">
                    ${session.logs.slice(-3).map(log => `<div style="margin: 2px 0; color: #aaa;">${log}</div>`).join('')}
                  </div>
                </div>
              </div>
            </div>
          `).join('')}
          
          ${status.activeTraining.length === 0 ? `
            <div style="text-align: center; color: #666; padding: 20px; font-style: italic;">
              No personas currently training. Send a sheriff to the Academy!
            </div>
          ` : ''}
        </div>
        
        <!-- Completed Training -->
        ${status.completed.length > 0 ? `
          <div class="completed-training" style="margin-top: 20px;">
            <h3 style="color: #888; font-size: 1em; margin-bottom: 10px;">Recent Graduates</h3>
            ${status.completed.map(session => `
              <div class="completed-persona" style="background: #2a2a2a; border-radius: 6px; padding: 10px; margin-bottom: 8px; border-left: 4px solid ${session.status === 'graduated' ? '#4CAF50' : '#f44336'};">
                <div style="display: flex; justify-content: space-between; align-items: center;">
                  <span style="font-weight: bold; color: #e0e0e0;">${session.personaName}</span>
                  <span style="color: ${session.status === 'graduated' ? '#4CAF50' : '#f44336'}; font-size: 0.85em;">
                    ${session.status === 'graduated' ? '🎓 Graduated' : '❌ Failed'}
                  </span>
                </div>
                <div style="font-size: 0.8em; color: #888; margin-top: 5px;">
                  ${session.specialization} • ${(session.graduationScore * 100).toFixed(1)}% accuracy
                  ${session.fineTuneId ? ` • LoRA: ${session.fineTuneId.substring(0, 20)}...` : ''}
                </div>
              </div>
            `).join('')}
          </div>
        ` : ''}
        
        <!-- Academy Actions -->
        <div class="academy-actions" style="margin-top: 20px; text-align: center;">
          <button onclick="sendSheriffToAcademy()" style="background: #4CAF50; margin-right: 10px;">
            🛡️ Send Sheriff to Academy
          </button>
          <button onclick="trainCustomPersona()" style="background: #2196F3;">
            🎓 Train Custom Persona
          </button>
        </div>
      </div>
    `;
  }

  /**
   * Generate Academy JavaScript for the web interface
   */
  generateAcademyJS() {
    return `
      // Academy WebSocket handling
      function handleAcademyUpdate(data) {
        if (data.type === 'academy_update') {
          updateTrainingDisplay(data.personaName, data.session);
        }
      }
      
      function updateTrainingDisplay(personaName, session) {
        // Update the active training section
        const activeTraining = document.getElementById('active-training');
        if (activeTraining) {
          // Refresh the entire academy section
          updateAcademySection();
        }
      }
      
      function toggleTrainingDetails(personaName) {
        const details = document.getElementById('details-' + personaName);
        if (details) {
          details.style.display = details.style.display === 'none' ? 'block' : 'none';
        }
      }
      
      function sendSheriffToAcademy() {
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({
            type: 'start_academy_training',
            personaName: 'Sheriff-' + Date.now(),
            specialization: 'protocol_enforcement',
            rounds: 10
          }));
        }
      }
      
      function trainCustomPersona() {
        const personaName = prompt('Enter persona name:');
        const specialization = prompt('Enter specialization (e.g., protocol_enforcement, command_validation):') || 'protocol_enforcement';
        
        if (personaName && ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({
            type: 'start_academy_training',
            personaName: personaName,
            specialization: specialization,
            rounds: 10
          }));
        }
      }
      
      function updateAcademySection() {
        // Request academy status update
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({
            type: 'get_academy_status'
          }));
        }
      }
      
      // Academy updates now happen via WebSocket broadcasts - no need for polling
    `;
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

module.exports = AcademyWebInterface;