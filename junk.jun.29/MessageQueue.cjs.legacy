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
          type: 'response',
          message: "👋 Welcome to Continuum! I'm ready to help coordinate your tasks. Please note that AI features require API keys to be configured. You can add them in your .continuum/config.env file.",
          agent: 'System',
          room: 'general',
          sheriff_status: 'VALID',
          costs: continuum.costs
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
          type: 'response',
          message: finalGreeting,
          agent: 'GeneralAI',
          room: 'general',
          sheriff_status: 'VALID',
          costs: continuum.costs
        };
      } catch (error) {
        console.error('AI greeting failed:', error);
        return {
          type: 'response',
          message: "👋 Welcome to Continuum! I'm a coordination system for managing AI agents and tasks. How can I help you today?",
          agent: 'System',
          room: 'general',
          sheriff_status: 'VALID',
          costs: continuum.costs
        };
      }
    });
  }

  queueTaskResult(ws, task, role, continuum, commandId = null) {
    console.log(`📋 MESSAGE_QUEUE: queueTaskResult called with task: "${task}"`);
    console.log(`📋 MESSAGE_QUEUE: Role: "${role}"`);
    console.log(`📋 MESSAGE_QUEUE: CommandId: "${commandId}"`);
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
        
        const response = {
          type: 'response',
          message: finalResult,
          agent: result.role || role,
          room: 'general',
          sheriff_status: 'VALID',
          costs: continuum.costs,
          data: {
            role: result.role || role,
            task: task,
            result: finalResult,
            costs: continuum.costs
          }
        };
        
        // Include commandId if available for command matching
        if (commandId) {
          response.commandId = commandId;
          response.type = 'command_response';
          // For command_response type, Python client expects 'result' field
          response.result = response.data;
        }
        
        return response;
      } catch (taskError) {
        console.error('Task error:', taskError);
        const errorResponse = {
          type: 'response',
          message: `Task failed: ${taskError.message}`,
          agent: 'System',
          room: 'general',
          sheriff_status: 'ERROR',
          success: false,
          error: taskError.message
        };
        
        // Include commandId if available for command matching
        if (commandId) {
          errorResponse.commandId = commandId;
          errorResponse.type = 'command_response';
          // For command_response type, Python client expects 'result' field
          errorResponse.result = errorResponse;
        }
        
        return errorResponse;
      }
    });
  }

  cleanup(sessionId) {
    this.queues.delete(sessionId);
  }
}

module.exports = MessageQueue;