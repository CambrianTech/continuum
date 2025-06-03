/**
 * WebSocket Message Queue
 * Handles queued message processing to prevent race conditions
 */

const WebSocket = require('ws');

class MessageQueue {
  constructor() {
    this.queues = new Map(); // sessionId -> queue
  }

  initializeSession(ws, sessionId) {
    ws.messageQueue = [];
    ws.isProcessing = false;
    ws.sessionId = sessionId;
    this.queues.set(sessionId, ws);
  }

  queueMessage(ws, messageGenerator) {
    ws.messageQueue.push(messageGenerator);
    this.processQueue(ws);
  }

  async processQueue(ws) {
    if (ws.isProcessing || ws.messageQueue.length === 0) {
      return;
    }

    ws.isProcessing = true;
    
    while (ws.messageQueue.length > 0) {
      const messageGenerator = ws.messageQueue.shift();
      try {
        const message = await messageGenerator();
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify(message));
        }
      } catch (error) {
        console.error('Queue message error:', error);
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({
            type: 'error',
            data: 'Message processing failed'
          }));
        }
      }
    }
    
    ws.isProcessing = false;
  }

  queueGreeting(ws, continuum) {
    this.queueMessage(ws, async () => {
      const connectionEvent = 'A new user just connected. Give them a brief, friendly greeting and ask how you can help. Keep it conversational and short - no long explanations about the system.';
      
      try {
        // Force greeting to use GeneralAI to avoid identity confusion
        const greeting = await continuum.sendTask('GeneralAI', connectionEvent);
        return {
          type: 'result',
          data: {
            role: 'GeneralAI',
            task: 'user_connection_greeting',
            result: greeting,
            costs: continuum.costs
          }
        };
      } catch (error) {
        console.error('AI greeting failed:', error);
        return {
          type: 'error',
          data: 'Greeting failed'
        };
      }
    });
  }

  queueTaskResult(ws, task, role, continuum) {
    this.queueMessage(ws, async () => {
      try {
        const result = await continuum.intelligentRoute(task);
        
        return {
          type: 'result',
          data: {
            role: result.role || role,
            task: task,
            result: result.result || result,
            costs: continuum.costs
          }
        };
      } catch (taskError) {
        console.error('Task error:', taskError);
        return {
          type: 'error',
          data: `Task failed: ${taskError.message}`
        };
      }
    });
  }

  cleanup(sessionId) {
    this.queues.delete(sessionId);
  }
}

module.exports = MessageQueue;