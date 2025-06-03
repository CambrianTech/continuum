/**
 * WebSocket Server
 * Handles WebSocket connections and message routing
 */

const WebSocket = require('ws');
const MessageQueue = require('./MessageQueue.cjs');

class WebSocketServer {
  constructor(continuum, httpServer) {
    this.continuum = continuum;
    this.messageQueue = new MessageQueue();
    this.wss = new WebSocket.Server({ server: httpServer });
    this.setupWebSocket();
  }

  setupWebSocket() {
    this.wss.on('connection', (ws) => {
      // Create unique session for this connection
      const sessionId = `session_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      // Initialize session data with message queue
      this.messageQueue.initializeSession(ws, sessionId);
      this.continuum.activeConnections.set(sessionId, ws);
      this.continuum.conversationThreads.set(sessionId, []);
      
      console.log(`👤 User connected (Session: ${sessionId})`);
      
      // Send status immediately (not queued)
      this.sendStatus(ws, sessionId);
      
      // Queue the greeting to be sent after any tasks
      this.messageQueue.queueGreeting(ws, this.continuum);
      
      ws.on('message', (message) => this.handleMessage(ws, message));
      ws.on('close', () => this.handleDisconnect(sessionId));
      ws.on('error', (error) => this.handleError(sessionId, error));
    });
  }

  sendStatus(ws, sessionId) {
    const packageInfo = require('../../package.json');
    ws.send(JSON.stringify({
      type: 'status',
      data: {
        message: 'Ready to help',
        sessionId: sessionId,
        version: packageInfo.version,
        workingDir: process.cwd(),
        nodeVersion: process.version,
        pid: process.pid,
        uptime: process.uptime(),
        sessions: Array.from(this.continuum.sessions.entries()),
        costs: this.continuum.costs
      }
    }));
  }

  async handleMessage(ws, message) {
    try {
      console.log('📨 Received message:', message.toString());
      const data = JSON.parse(message);
      
      if (data.type === 'task') {
        const { role, task } = data;
        
        console.log(`🎯 Task: ${role} -> ${task}`);
        
        // Send working status immediately (not queued)
        ws.send(JSON.stringify({
          type: 'working',
          data: `🤖 ${role} processing: ${task.substring(0, 50)}...`
        }));
        
        // Queue the task result
        this.messageQueue.queueTaskResult(ws, task, role, this.continuum);
        
      } else if (data.type === 'message') {
        // Handle general chat messages with auto-routing and Sheriff validation
        const { content, room = 'general' } = data;
        
        console.log(`💬 ${room} message (auto-route): ${content}`);
        
        try {
          const result = await this.continuum.intelligentRoute(content);
          
          // Always run through Protocol Sheriff for validation
          const validation = await this.continuum.protocolSheriff.validateResponse(
            result.result, 
            content, 
            result.role
          );
          
          let finalResponse = result.result;
          if (!validation.isValid && validation.correctedResponse) {
            console.log(`🚨 Protocol Sheriff: Using corrected response`);
            finalResponse = validation.correctedResponse;
          }
          
          ws.send(JSON.stringify({
            type: 'response',
            message: finalResponse,
            agent: result.role,
            room: room,
            sheriff_status: validation.isValid ? 'VALID' : 'CORRECTED'
          }));
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'response',
            message: 'Sorry, I encountered an error processing your message.',
            agent: 'System',
            room: room
          }));
        }
        
      } else if (data.type === 'direct_message') {
        // Handle direct messages to specific agents
        const { content, agent, room = 'general' } = data;
        
        console.log(`📋 Direct message to ${agent}: ${content}`);
        
        try {
          const result = await this.continuum.sendTask(agent, content);
          
          // Always run through Protocol Sheriff for validation
          const validation = await this.continuum.protocolSheriff.validateResponse(
            result, 
            content, 
            agent
          );
          
          let finalResponse = result;
          if (!validation.isValid && validation.correctedResponse) {
            console.log(`🚨 Protocol Sheriff: Using corrected response`);
            finalResponse = validation.correctedResponse;
          }
          
          ws.send(JSON.stringify({
            type: 'response',
            message: finalResponse,
            agent: agent,
            room: room,
            sheriff_status: validation.isValid ? 'VALID' : 'CORRECTED'
          }));
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'response',
            message: `Sorry, ${agent} encountered an error: ${error.message}`,
            agent: 'System',
            room: room
          }));
        }
        
      } else if (data.type === 'group_message') {
        // Handle group chat messages to multiple agents
        const { content, agents, room = 'general' } = data;
        
        console.log(`👥 Group message to [${agents.join(', ')}]: ${content}`);
        
        // Send to each agent and collect responses
        const responses = [];
        for (const agent of agents) {
          try {
            const result = await this.continuum.sendTask(agent, 
              `${content}\n\n[GROUP CHAT CONTEXT: You are in a group chat with ${agents.filter(a => a !== agent).join(', ')}. Keep your response concise and collaborative.]`
            );
            
            // Always run through Protocol Sheriff for validation
            const validation = await this.continuum.protocolSheriff.validateResponse(
              result, 
              content, 
              agent
            );
            
            let finalResponse = result;
            if (!validation.isValid && validation.correctedResponse) {
              console.log(`🚨 Protocol Sheriff: Using corrected response for ${agent}`);
              finalResponse = validation.correctedResponse;
            }
            
            responses.push({
              agent: agent,
              message: finalResponse,
              sheriff_status: validation.isValid ? 'VALID' : 'CORRECTED'
            });
          } catch (error) {
            responses.push({
              agent: agent,
              message: `Error: ${error.message}`,
              sheriff_status: 'ERROR'
            });
          }
        }
        
        // Send all responses
        for (const response of responses) {
          ws.send(JSON.stringify({
            type: 'response',
            message: response.message,
            agent: response.agent,
            room: room,
            sheriff_status: response.sheriff_status,
            group_chat: true
          }));
        }
        
      } else if (data.type === 'academy_message') {
        // Handle Academy chat messages
        const { content } = data;
        
        console.log(`🎓 Academy message: ${content}`);
        
        ws.send(JSON.stringify({
          type: 'response',
          message: 'Academy training system is ready. Use the buttons to start training agents.',
          agent: 'Academy System',
          room: 'academy'
        }));
        
      } else if (data.type === 'start_academy_training') {
        // Handle Academy training requests
        const { personaName, specialization, rounds, customPrompt, trainingIntensity } = data;
        
        if (this.continuum.uiGenerator && this.continuum.uiGenerator.academyInterface) {
          const session = await this.continuum.uiGenerator.academyInterface.startAcademyTraining(
            personaName, 
            specialization, 
            { 
              rounds,
              customPrompt,
              trainingIntensity
            }
          );
          
          const message = customPrompt ? 
            `🎓 ${personaName} enrolled in custom Academy training for ${specialization}!` :
            `🎓 ${personaName} has been enrolled in Academy training for ${specialization}!`;
          
          ws.send(JSON.stringify({
            type: 'response',
            message: message,
            agent: 'Academy System',
            room: 'academy'
          }));
        }
        
      } else if (data.type === 'get_initial_academy_status') {
        // INTERRUPT-DRIVEN: Send initial Academy status once, then rely on push updates
        console.log('🎓 Initial Academy status requested');
        
        if (this.continuum.uiGenerator && this.continuum.uiGenerator.academyInterface) {
          const status = this.continuum.uiGenerator.academyInterface.getAcademyStatus();
          
          ws.send(JSON.stringify({
            type: 'academy_status_push',
            status: status
          }));
          console.log('🎓 PUSH: Initial Academy status sent');
        }
        
      } else if (data.type === 'get_academy_status') {
        // OLD POLLING METHOD - DISABLED
        console.log('🎓 Old polling request ignored - use push-based updates instead');
        return;
        
        if (this.continuum.uiGenerator && this.continuum.uiGenerator.academyInterface) {
          console.log('🎓 Getting Academy status...');
          const status = this.continuum.uiGenerator.academyInterface.getAcademyStatus();
          console.log('🎓 Academy status retrieved:', JSON.stringify(status, null, 2));
          
          const response = {
            type: 'academy_status',
            status: status
          };
          console.log('🎓 Sending response:', JSON.stringify(response, null, 2));
          
          try {
            ws.send(JSON.stringify(response));
            console.log('🎓 ✅ Academy status sent successfully to client');
          } catch (sendError) {
            console.error('🎓 ❌ Failed to send Academy status:', sendError);
          }
        } else {
          console.log('🎓 Academy interface not available, sending empty status');
          console.log('🎓 UIGenerator available:', !!this.continuum.uiGenerator);
          if (this.continuum.uiGenerator) {
            console.log('🎓 Academy interface available:', !!this.continuum.uiGenerator.academyInterface);
          }
          
          // Send empty status if academy interface is not available
          const emptyResponse = {
            type: 'academy_status',
            status: {
              activeTraining: [],
              completed: [],
              stats: {
                totalPersonas: 0,
                activeTraining: 0,
                graduated: 0,
                failed: 0
              }
            }
          };
          
          try {
            ws.send(JSON.stringify(emptyResponse));
            console.log('🎓 ✅ Empty Academy status sent successfully to client');
          } catch (sendError) {
            console.error('🎓 ❌ Failed to send empty Academy status:', sendError);
          }
        }
        
      } else if (data.type === 'version_check') {
        // Handle manual version check requests
        console.log('🔍 Manual version check requested via WebSocket');
        
        try {
          const restarted = await this.continuum.versionManager.checkAndRestart();
          
          if (!restarted) {
            ws.send(JSON.stringify({
              type: 'response',
              message: '✅ Version is up to date - no restart needed',
              agent: 'Version Manager',
              room: 'system'
            }));
          }
          // If restarted, the connection will be closed anyway
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'response',
            message: `❌ Version check failed: ${error.message}`,
            agent: 'Version Manager',
            room: 'system'
          }));
        }
        
      }
    } catch (error) {
      console.error('Message error:', error);
      ws.send(JSON.stringify({
        type: 'error',
        data: `Message error: ${error.message}`
      }));
    }
  }

  handleDisconnect(sessionId) {
    console.log(`👤 User disconnected (Session: ${sessionId})`);
    this.continuum.activeConnections.delete(sessionId);
    this.continuum.conversationThreads.delete(sessionId);
    this.messageQueue.cleanup(sessionId);
  }

  handleError(sessionId, error) {
    console.error(`WebSocket error for session ${sessionId}:`, error);
  }

  /**
   * Broadcast message to all connected clients
   */
  broadcast(message) {
    const messageString = JSON.stringify(message);
    
    this.continuum.activeConnections.forEach((ws, sessionId) => {
      if (ws.readyState === ws.OPEN) {
        try {
          ws.send(messageString);
        } catch (error) {
          console.error(`Failed to broadcast to session ${sessionId}:`, error);
          this.handleDisconnect(sessionId);
        }
      }
    });
  }
}

module.exports = WebSocketServer;