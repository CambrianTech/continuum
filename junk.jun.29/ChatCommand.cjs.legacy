/**
 * Chat Command - Start and manage conversations with agents
 * Orchestrates proper multi-agent chat like Discord
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class ChatCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'chat',
      description: 'Start and manage conversations with agents in chat rooms',
      icon: '💬',
      category: 'communication',
      parameters: {
        room: {
          type: 'string',
          required: false,
          default: 'general',
          description: 'Chat room to join or create'
        },
        agents: {
          type: 'array',
          required: false,
          description: 'List of agents to invite to the chat'
        },
        message: {
          type: 'string',
          required: false,
          description: 'Initial message to send'
        },
        private: {
          type: 'boolean',
          required: false,
          default: false,
          description: 'Create a private chat room'
        },
        persistent: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Keep chat history persistent'
        }
      },
      examples: [
        {
          description: 'Start chat with PlannerAI and CodeAI',
          usage: 'chat --agents ["PlannerAI", "CodeAI"] --message "Help me build a web app"'
        },
        {
          description: 'Join existing room',
          usage: 'chat --room "project-alpha"'
        },
        {
          description: 'Create private chat',
          usage: 'chat --agents ["PlannerAI"] --private true --room "private-planning"'
        },
        {
          description: 'Start general chat',
          usage: 'chat --message "Hello everyone!"'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { room = 'general', agents = [], message, private: isPrivate = false, persistent = true } = this.parseParams(paramsString);

      console.log(`💬 Chat: Starting conversation in room "${room}"`);

      if (!context || !context.webSocketServer) {
        return this.createErrorResult('No WebSocket server available for chat');
      }

      // Create or join chat room
      const roomData = await this.createOrJoinRoom(room, {
        isPrivate,
        persistent,
        creator: 'user',
        agents,
        context
      });

      // Add agents to the room
      if (agents && agents.length > 0) {
        await this.inviteAgentsToRoom(room, agents, context);
      }

      // Send initial message if provided
      if (message) {
        await this.sendMessageToRoom(room, message, 'user', context);
      }

      console.log(`✅ Chat: Room "${room}" ready with ${agents.length} agents`);

      return this.createSuccessResult({
        room,
        agents,
        roomType: isPrivate ? 'private' : 'public',
        persistent,
        message: message || null,
        participants: ['user', ...agents],
        timestamp: new Date().toISOString()
      }, `Chat started in room "${room}"`);

    } catch (error) {
      console.error(`❌ Chat command failed: ${error.message}`);
      return this.createErrorResult(`Chat failed: ${error.message}`);
    }
  }

  static async createOrJoinRoom(roomName, options, context) {
    const { isPrivate, persistent, creator, agents } = options;

    // Initialize room if it doesn't exist
    if (!context.chatRooms) {
      context.chatRooms = new Map();
    }

    if (!context.chatRooms.has(roomName)) {
      const roomData = {
        name: roomName,
        type: isPrivate ? 'private' : 'public',
        persistent,
        creator,
        participants: [creator],
        agents: [...agents],
        messages: [],
        createdAt: new Date().toISOString(),
        lastActivity: new Date().toISOString()
      };

      context.chatRooms.set(roomName, roomData);
      console.log(`🏠 Chat: Created new room "${roomName}" (${roomData.type})`);

      // Broadcast room creation
      context.webSocketServer.broadcast({
        type: 'room_created',
        room: roomName,
        roomData,
        timestamp: new Date().toISOString()
      });
    }

    return context.chatRooms.get(roomName);
  }

  static async inviteAgentsToRoom(roomName, agents, context) {
    const room = context.chatRooms?.get(roomName);
    if (!room) {
      throw new Error(`Room "${roomName}" not found`);
    }

    // Add agents to room participants
    for (const agent of agents) {
      if (!room.participants.includes(agent)) {
        room.participants.push(agent);
      }
      if (!room.agents.includes(agent)) {
        room.agents.push(agent);
      }
    }

    console.log(`🤖 Chat: Invited agents [${agents.join(', ')}] to room "${roomName}"`);

    // Notify all participants about agent joins
    context.webSocketServer.broadcast({
      type: 'agents_joined',
      room: roomName,
      agents,
      participants: room.participants,
      timestamp: new Date().toISOString()
    });

    // Send system message about agents joining
    const systemMessage = {
      id: this.generateMessageId(),
      type: 'system',
      content: `${agents.join(', ')} joined the conversation`,
      room: roomName,
      timestamp: new Date().toISOString(),
      sender: 'system'
    };

    room.messages.push(systemMessage);
    room.lastActivity = new Date().toISOString();

    // Broadcast system message
    context.webSocketServer.broadcast({
      type: 'message',
      messageData: systemMessage,
      room: roomName
    });
  }

  static async sendMessageToRoom(roomName, content, sender, context) {
    const room = context.chatRooms?.get(roomName);
    if (!room) {
      throw new Error(`Room "${roomName}" not found`);
    }

    const message = {
      id: this.generateMessageId(),
      type: 'user_message',
      content,
      room: roomName,
      sender,
      timestamp: new Date().toISOString(),
      participants: [...room.participants] // Snapshot of who can see this message
    };

    // Add to room history
    room.messages.push(message);
    room.lastActivity = new Date().toISOString();

    console.log(`💬 Chat: Message sent to room "${roomName}" by ${sender}`);

    // Broadcast to all participants (including agents)
    context.webSocketServer.broadcast({
      type: 'message',
      messageData: message,
      room: roomName,
      chatHistory: this.getRecentHistory(room, 10) // Include recent context
    });

    // Trigger agent responses if this is a user message
    if (sender === 'user' && room.agents.length > 0) {
      await this.triggerAgentResponses(room, message, context);
    }

    return message;
  }

  static async triggerAgentResponses(room, userMessage, context) {
    // Get full conversation context (last 20 messages)
    const conversationHistory = this.getRecentHistory(room, 20);
    
    // Format conversation for agents
    const contextString = conversationHistory.map(msg => 
      `[${msg.timestamp}] ${msg.sender}: ${msg.content}`
    ).join('\n');

    console.log(`🤖 Chat: Triggering responses from ${room.agents.length} agents with full context`);

    // Send to each agent with FULL conversation context
    for (const agent of room.agents) {
      try {
        // Build agent prompt with full context
        const agentPrompt = `
MULTI-AGENT CHAT CONTEXT:
Room: ${room.name}
Participants: ${room.participants.join(', ')}
Current conversation history:

${contextString}

Latest message from ${userMessage.sender}: ${userMessage.content}

You are ${agent} in this group conversation. Respond naturally and collaborate with other agents. Keep responses concise but helpful. You can see all previous messages above.
        `.trim();

        // Send to continuum for agent processing
        const result = await context.continuum?.sendTask(agent, agentPrompt);
        
        if (result && result.response) {
          // Create agent message
          const agentMessage = {
            id: this.generateMessageId(),
            type: 'agent_message',
            content: result.response,
            room: room.name,
            sender: agent,
            timestamp: new Date().toISOString(),
            participants: [...room.participants],
            inResponseTo: userMessage.id
          };

          // Add to room history
          room.messages.push(agentMessage);
          room.lastActivity = new Date().toISOString();

          // Broadcast agent response
          context.webSocketServer.broadcast({
            type: 'message',
            messageData: agentMessage,
            room: room.name,
            chatHistory: this.getRecentHistory(room, 5)
          });

          console.log(`🤖 Chat: ${agent} responded in room "${room.name}"`);
        }

      } catch (error) {
        console.error(`❌ Chat: Agent ${agent} failed to respond:`, error.message);
      }
    }
  }

  static getRecentHistory(room, count = 10) {
    return room.messages
      .slice(-count)
      .map(msg => ({
        id: msg.id,
        content: msg.content,
        sender: msg.sender,
        timestamp: msg.timestamp,
        type: msg.type
      }));
  }

  static generateMessageId() {
    return `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  // Helper methods for external use
  static async listActiveRooms(context) {
    if (!context.chatRooms) return [];
    
    return Array.from(context.chatRooms.values()).map(room => ({
      name: room.name,
      type: room.type,
      participants: room.participants,
      messageCount: room.messages.length,
      lastActivity: room.lastActivity
    }));
  }

  static async getRoomHistory(roomName, context, limit = 50) {
    const room = context.chatRooms?.get(roomName);
    if (!room) return [];
    
    return room.messages.slice(-limit);
  }
}

module.exports = ChatCommand;