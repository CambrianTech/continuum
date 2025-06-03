/**
 * Academy Web Interface - Shows training progress and Academy status
 */

const Academy = require('./Academy.cjs');
const { ModelRegistry } = require('./AIModel.cjs');
const ModelCaliber = require('./ModelCaliber.cjs');
const LoRAAdapter = require('./LoRAAdapter.cjs');
const { ModelAdapterFactory } = require('./ModelAdapter.cjs');

class AcademyWebInterface {
  constructor(continuum) {
    this.continuum = continuum;
    this.academy = new Academy(new ModelRegistry(), new ModelCaliber());
    this.trainingPersonas = new Map(); // Track active training sessions
    this.completedPersonas = new Map(); // Track completed personas
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
      totalRounds: options.rounds || 10,
      graduationScore: 0,
      logs: [`🎓 Enrolling ${personaName} in Academy...`]
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
      
      // Enroll in Academy
      const recruit = await this.academy.enrollRecruit(personaName, 'gpt-3.5-turbo', specialization);
      session.logs.push(`✅ ${personaName} enrolled for ${specialization} training`);
      this.broadcastAcademyUpdate(personaName, session);
      
      // Run training rounds with progress updates
      const totalRounds = session.totalRounds;
      
      for (let round = 1; round <= totalRounds; round++) {
        session.currentRound = round;
        session.progress = Math.round((round / totalRounds) * 100);
        session.status = `training_round_${round}`;
        session.logs.push(`🔥 Round ${round}/${totalRounds}: Adversarial training in progress...`);
        
        this.broadcastAcademyUpdate(personaName, session);
        
        // Simulate training round (in real implementation, call academy.runBootCamp)
        await this.simulateTrainingRound(recruit, round);
        
        // Calculate current accuracy
        const accuracy = this.calculateAccuracy(recruit, round);
        session.graduationScore = accuracy;
        session.logs.push(`📊 Round ${round} completed: ${(accuracy * 100).toFixed(1)}% accuracy`);
        
        this.broadcastAcademyUpdate(personaName, session);
        
        // Wait between rounds
        await this.sleep(1000);
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
    
    this.broadcastAcademyUpdate(personaName, session);
  }

  /**
   * Simulate a training round
   */
  async simulateTrainingRound(recruit, round) {
    // Simulate Academy adversarial training
    const accuracy = Math.min(0.5 + (round * 0.08) + (Math.random() * 0.1), 0.95);
    
    if (!recruit.trainingData) recruit.trainingData = [];
    
    recruit.trainingData.push({
      round,
      testsGenerated: 5,
      correctDetections: Math.floor(accuracy * 5),
      totalTests: 5,
      accuracy,
      timestamp: new Date().toISOString()
    });
    
    // Simulate training time
    await this.sleep(500 + Math.random() * 1000);
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
   * Broadcast Academy updates to all connected clients
   */
  broadcastAcademyUpdate(personaName, session) {
    if (this.continuum.webSocketServer) {
      const message = {
        type: 'academy_update',
        personaName,
        session: {
          ...session,
          logs: session.logs.slice(-10) // Only send last 10 log entries
        }
      };
      
      this.continuum.webSocketServer.broadcast(message);
    }
  }

  /**
   * Get Academy status for UI
   */
  getAcademyStatus() {
    const activeTraining = Array.from(this.trainingPersonas.values());
    const completed = Array.from(this.completedPersonas.values());
    
    return {
      activeTraining,
      completed: completed.slice(-5), // Last 5 completed
      stats: {
        totalPersonas: activeTraining.length + completed.length,
        activeTraining: activeTraining.length,
        graduated: completed.filter(p => p.status === 'graduated').length,
        failed: completed.filter(p => p.status === 'failed').length
      }
    };
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
      
      // Update academy section every 5 seconds
      setInterval(updateAcademySection, 5000);
    `;
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

module.exports = AcademyWebInterface;