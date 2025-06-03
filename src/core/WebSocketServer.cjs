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
}

module.exports = WebSocketServer;