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
      // Check if we have any AI models available
      if (!continuum.modelRegistry || continuum.modelRegistry.getAvailableModels().length === 0) {
        console.log('⚠️ No AI models available, sending static greeting');
        return {
          type: 'result',
          data: {
            role: 'System',
            task: 'user_connection_greeting',
            result: "👋 Welcome to Continuum! I'm ready to help coordinate your tasks. Please note that AI features require API keys to be configured. You can add them in your .continuum/config.env file.",
            costs: continuum.costs
          }
        };
      }

      const connectionEvent = 'A new user just connected. Give them a brief, friendly greeting and ask how you can help. Keep it conversational and short - no long explanations about the system.';
      
      try {
        // Force greeting to use GeneralAI to avoid identity confusion
        const greeting = await continuum.sendTask('GeneralAI', connectionEvent);
        
        // 🤖 Protocol Sheriff: Validate greeting (only if Sheriff is available)
        let finalGreeting = greeting;
        if (continuum.protocolSheriff && continuum.modelCaliber) {
          try {
            const validation = await continuum.protocolSheriff.validateResponse(
              greeting, 
              'user connection', 
              'GeneralAI'
            );
            
            finalGreeting = (!validation.isValid && validation.correctedResponse) 
              ? validation.correctedResponse 
              : greeting;
          } catch (validationError) {
            console.log('⚠️ Protocol Sheriff validation failed, using original greeting');
          }
        }
        
        return {
          type: 'result',
          data: {
            role: 'GeneralAI',
            task: 'user_connection_greeting',
            result: finalGreeting,
            costs: continuum.costs
          }
        };
      } catch (error) {
        console.error('AI greeting failed:', error);
        return {
          type: 'result',
          data: {
            role: 'System',
            task: 'user_connection_greeting',
            result: "👋 Welcome to Continuum! I'm a coordination system for managing AI agents and tasks. How can I help you today?",
            costs: continuum.costs
          }
        };
      }
    });
  }

  queueTaskResult(ws, task, role, continuum) {
    console.log(`📋 MESSAGE_QUEUE: queueTaskResult called with task: "${task}"`);
    console.log(`📋 MESSAGE_QUEUE: Role: "${role}"`);
    console.log(`📋 MESSAGE_QUEUE: About to call continuum.intelligentRoute`);
    
    this.queueMessage(ws, async () => {
      try {
        console.log(`📋 MESSAGE_QUEUE: Calling intelligentRoute with task: "${task}"`);
        const result = await continuum.intelligentRoute(task);
        console.log(`📋 MESSAGE_QUEUE: intelligentRoute returned:`, result);
        
        // 🤖 Protocol Sheriff: Validate response before sending via WebSocket
        const validation = await continuum.protocolSheriff.validateResponse(
          result.result, 
          task, 
          result.role
        );
        
        const finalResult = (!validation.isValid && validation.correctedResponse) 
          ? validation.correctedResponse 
          : result.result;
        
        return {
          type: 'result',
          data: {
            role: result.role || role,
            task: task,
            result: finalResult,
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