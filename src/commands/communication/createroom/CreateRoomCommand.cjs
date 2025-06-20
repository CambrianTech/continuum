/**
 * CreateRoom Command - Discord-style room management
 * Subscribe to multiple rooms, see all activity
 */

const BaseCommand = require('../../BaseCommand.cjs');

class CreateRoomCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'createRoom',
      description: 'Create and manage chat rooms like Discord channels',
      icon: '🏠',
      category: 'communication',
      parameters: {
        name: {
          type: 'string',
          required: true,
          description: 'Room name (will be normalized: "My Room" → "my-room")'
        },
        type: {
          type: 'string',
          required: false,
          default: 'public',
          description: 'Room type: public, private, dm, group'
        },
        topic: {
          type: 'string',
          required: false,
          description: 'Room topic/description'
        },
        autoJoin: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Automatically join after creating'
        },
        agents: {
          type: 'array',
          required: false,
          default: [],
          description: 'Default agents to invite to this room'
        },
        permissions: {
          type: 'object',
          required: false,
          description: 'Room permissions (who can join, post, etc.)'
        }
      },
      examples: [
        {
          description: 'Create public project room',
          usage: 'createRoom --name "Project Alpha" --topic "Web app development"'
        },
        {
          description: 'Create private planning room with agents',
          usage: 'createRoom --name "strategy-session" --type private --agents ["PlannerAI", "CodeAI"]'
        },
        {
          description: 'Create DM with specific agent',
          usage: 'createRoom --name "dm-planner" --type dm --agents ["PlannerAI"]'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { 
        name, 
        type = 'public', 
        topic, 
        autoJoin = true, 
        agents = [], 
        permissions = {} 
      } = this.parseParams(paramsString);

      // Normalize room name (like Discord)
      const roomId = this.normalizeRoomName(name);

      console.log(`🏠 CreateRoom: Creating "${name}" (${roomId}) as ${type} room`);

      if (!context || !context.webSocketServer) {
        return this.createErrorResult('No WebSocket server available');
      }

      // Initialize room system
      if (!context.chatRooms) {
        context.chatRooms = new Map();
      }
      if (!context.userSubscriptions) {
        context.userSubscriptions = new Map(); // user -> Set of room IDs
      }

      // Check if room already exists
      if (context.chatRooms.has(roomId)) {
        return this.createErrorResult(`Room "${roomId}" already exists`);
      }

      // Create room data
      const roomData = {
        id: roomId,
        name: name, // Display name
        normalizedName: roomId, // URL-safe name
        type,
        topic: topic || '',
        creator: 'user', // TODO: Get actual user ID
        participants: ['user'], // Start with creator
        subscribedUsers: new Set(['user']), // Who gets notifications
        agents: [...agents],
        messages: [],
        permissions: {
          canJoin: type === 'public' ? 'anyone' : 'invite-only',
          canPost: 'participants',
          canInvite: 'participants',
          ...permissions
        },
        createdAt: new Date().toISOString(),
        lastActivity: new Date().toISOString(),
        messageCount: 0,
        isActive: true
      };

      // Add room to system
      context.chatRooms.set(roomId, roomData);

      // Subscribe creator to room (like joining a Discord channel)
      if (!context.userSubscriptions.has('user')) {
        context.userSubscriptions.set('user', new Set());
      }
      context.userSubscriptions.get('user').add(roomId);

      console.log(`✅ CreateRoom: Room "${roomId}" created and user subscribed`);

      // Add agents if specified
      if (agents.length > 0) {
        await this.addAgentsToRoom(roomId, agents, context);
      }

      // Auto-join if requested
      if (autoJoin) {
        await this.joinUserToRoom(roomId, 'user', context);
      }

      // Broadcast room creation to all connected clients
      context.webSocketServer.broadcast({
        type: 'room_created',
        roomData: this.sanitizeRoomData(roomData),
        timestamp: new Date().toISOString()
      });

      // Send welcome message
      if (autoJoin) {
        await this.sendSystemMessage(roomId, `Welcome to ${name}! ${topic ? `Topic: ${topic}` : ''}`, context);
      }

      return this.createSuccessResult({
        roomId,
        name,
        type,
        topic,
        agents,
        autoJoined: autoJoin,
        subscribed: true,
        participants: roomData.participants,
        permissions: roomData.permissions
      }, `Room "${name}" created successfully`);

    } catch (error) {
      console.error(`❌ CreateRoom failed: ${error.message}`);
      return this.createErrorResult(`Room creation failed: ${error.message}`);
    }
  }

  static normalizeRoomName(name) {
    if (!name || typeof name !== 'string') {
      throw new Error('Room name is required and must be a string');
    }
    return name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-') // Replace non-alphanumeric with dashes
      .replace(/^-+|-+$/g, '')     // Remove leading/trailing dashes
      .replace(/-+/g, '-');        // Collapse multiple dashes
  }

  static async addAgentsToRoom(roomId, agents, context) {
    const room = context.chatRooms.get(roomId);
    if (!room) return;

    for (const agent of agents) {
      if (!room.participants.includes(agent)) {
        room.participants.push(agent);
      }
      if (!room.agents.includes(agent)) {
        room.agents.push(agent);
      }
    }

    console.log(`🤖 CreateRoom: Added agents [${agents.join(', ')}] to room "${roomId}"`);

    // Broadcast agent addition
    context.webSocketServer.broadcast({
      type: 'room_agents_added',
      roomId,
      agents,
      timestamp: new Date().toISOString()
    });
  }

  static async joinUserToRoom(roomId, userId, context) {
    const room = context.chatRooms.get(roomId);
    if (!room) return;

    // Add to participants if not already there
    if (!room.participants.includes(userId)) {
      room.participants.push(userId);
    }

    // Subscribe user to room updates
    if (!context.userSubscriptions.has(userId)) {
      context.userSubscriptions.set(userId, new Set());
    }
    context.userSubscriptions.get(userId).add(roomId);

    console.log(`👤 CreateRoom: User ${userId} joined room "${roomId}"`);

    // Broadcast join event
    context.webSocketServer.broadcast({
      type: 'user_joined_room',
      roomId,
      userId,
      timestamp: new Date().toISOString()
    });
  }

  static async sendSystemMessage(roomId, content, context) {
    const room = context.chatRooms.get(roomId);
    if (!room) return;

    const message = {
      id: `sys_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      type: 'system',
      content,
      room: roomId,
      sender: 'system',
      timestamp: new Date().toISOString()
    };

    room.messages.push(message);
    room.messageCount++;
    room.lastActivity = new Date().toISOString();

    // Broadcast to subscribed users only
    this.broadcastToRoomSubscribers(roomId, {
      type: 'message',
      messageData: message,
      roomId
    }, context);
  }

  static broadcastToRoomSubscribers(roomId, message, context) {
    // Only send to users subscribed to this room (like Discord)
    const subscribedUsers = [];
    
    for (const [userId, roomSet] of context.userSubscriptions.entries()) {
      if (roomSet.has(roomId)) {
        subscribedUsers.push(userId);
      }
    }

    console.log(`📡 Broadcasting to ${subscribedUsers.length} subscribers of room "${roomId}"`);

    // In a real implementation, this would filter WebSocket clients by user
    context.webSocketServer.broadcast({
      ...message,
      subscribedUsers, // Clients can filter on their end
      roomSubscription: true
    });
  }

  static sanitizeRoomData(roomData) {
    // Return public room data (hide sensitive info)
    return {
      id: roomData.id,
      name: roomData.name,
      type: roomData.type,
      topic: roomData.topic,
      participants: roomData.participants,
      agents: roomData.agents,
      messageCount: roomData.messageCount,
      lastActivity: roomData.lastActivity,
      isActive: roomData.isActive,
      permissions: roomData.permissions
    };
  }

  // Helper methods for room management
  static async listUserRooms(userId, context) {
    if (!context.userSubscriptions?.has(userId)) return [];

    const userRooms = [];
    const subscribedRoomIds = context.userSubscriptions.get(userId);

    for (const roomId of subscribedRoomIds) {
      const room = context.chatRooms.get(roomId);
      if (room && room.isActive) {
        userRooms.push(this.sanitizeRoomData(room));
      }
    }

    return userRooms;
  }

  static async getRoomMembers(roomId, context) {
    const room = context.chatRooms?.get(roomId);
    return room ? room.participants : [];
  }
}

module.exports = CreateRoomCommand;