/**
 * JoinRoom Command - Subscribe to chat rooms like Discord
 * Join multiple rooms, get notifications from all subscribed rooms
 */

const BaseCommand = require('../../BaseCommand.cjs');

class JoinRoomCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'joinRoom',
      description: 'Join and subscribe to chat rooms for notifications',
      icon: '🚪',
      category: 'communication',
      parameters: {
        room: {
          type: 'string',
          required: true,
          description: 'Room name or ID to join'
        },
        subscribe: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Subscribe to room notifications'
        },
        catchUp: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Get recent message history'
        },
        historyLimit: {
          type: 'number',
          required: false,
          default: 20,
          description: 'Number of recent messages to fetch'
        }
      },
      examples: [
        {
          description: 'Join project room and subscribe',
          usage: 'joinRoom --room "project-alpha"'
        },
        {
          description: 'Join without subscribing to notifications',
          usage: 'joinRoom --room "general" --subscribe false'
        },
        {
          description: 'Join and get last 50 messages',
          usage: 'joinRoom --room "dev-chat" --historyLimit 50'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { 
        room, 
        subscribe = true, 
        catchUp = true, 
        historyLimit = 20 
      } = this.parseParams(paramsString);

      const roomId = this.normalizeRoomName(room);
      const userId = 'user'; // TODO: Get actual user ID from context

      console.log(`🚪 JoinRoom: User joining room "${roomId}"`);

      if (!context || !context.webSocketServer) {
        return this.createErrorResult('No WebSocket server available');
      }

      // Initialize systems if needed
      if (!context.chatRooms) {
        context.chatRooms = new Map();
      }
      if (!context.userSubscriptions) {
        context.userSubscriptions = new Map();
      }

      // Check if room exists
      const roomData = context.chatRooms.get(roomId);
      if (!roomData) {
        return this.createErrorResult(`Room "${roomId}" not found. Use \`listRooms\` to see available rooms.`);
      }

      // Check permissions
      if (roomData.permissions.canJoin === 'invite-only' && !roomData.participants.includes(userId)) {
        return this.createErrorResult(`Room "${roomId}" is invite-only`);
      }

      // Add user to room participants
      if (!roomData.participants.includes(userId)) {
        roomData.participants.push(userId);
        console.log(`👤 JoinRoom: Added user to participants of "${roomId}"`);
      }

      // Subscribe user to room notifications (like Discord channel subscriptions)
      if (subscribe) {
        if (!context.userSubscriptions.has(userId)) {
          context.userSubscriptions.set(userId, new Set());
        }
        context.userSubscriptions.get(userId).add(roomId);
        console.log(`🔔 JoinRoom: User subscribed to notifications from "${roomId}"`);
      }

      // Update room activity
      roomData.lastActivity = new Date().toISOString();

      // Get room history for catch-up
      let recentMessages = [];
      if (catchUp) {
        recentMessages = roomData.messages.slice(-historyLimit).map(msg => ({
          id: msg.id,
          content: msg.content,
          sender: msg.sender,
          timestamp: msg.timestamp,
          type: msg.type
        }));
      }

      // Broadcast join event to room subscribers
      this.broadcastToRoomSubscribers(roomId, {
        type: 'user_joined_room',
        roomId,
        userId,
        roomName: roomData.name,
        timestamp: new Date().toISOString()
      }, context);

      // Send system message about user joining
      await this.sendSystemMessage(roomId, `${userId} joined the room`, context);

      console.log(`✅ JoinRoom: User successfully joined "${roomId}"`);

      return this.createSuccessResult({
        roomId,
        roomName: roomData.name,
        roomType: roomData.type,
        subscribed: subscribe,
        participants: roomData.participants,
        agents: roomData.agents,
        messageCount: roomData.messageCount,
        recentMessages,
        joinedAt: new Date().toISOString()
      }, `Joined room "${roomData.name}"`);

    } catch (error) {
      console.error(`❌ JoinRoom failed: ${error.message}`);
      return this.createErrorResult(`Failed to join room: ${error.message}`);
    }
  }

  static normalizeRoomName(name) {
    if (!name || typeof name !== 'string') {
      throw new Error('Room name is required and must be a string');
    }
    return name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .replace(/-+/g, '-');
  }

  static broadcastToRoomSubscribers(roomId, message, context) {
    const subscribedUsers = [];
    
    for (const [userId, roomSet] of context.userSubscriptions.entries()) {
      if (roomSet.has(roomId)) {
        subscribedUsers.push(userId);
      }
    }

    console.log(`📡 Broadcasting to ${subscribedUsers.length} subscribers of room "${roomId}"`);

    context.webSocketServer.broadcast({
      ...message,
      subscribedUsers,
      roomSubscription: true
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

  // Helper method to leave/unsubscribe from room
  static async leaveRoom(roomId, userId, context) {
    const room = context.chatRooms?.get(roomId);
    if (!room) return false;

    // Remove from participants
    const participantIndex = room.participants.indexOf(userId);
    if (participantIndex > -1) {
      room.participants.splice(participantIndex, 1);
    }

    // Unsubscribe from notifications
    const userSubs = context.userSubscriptions?.get(userId);
    if (userSubs) {
      userSubs.delete(roomId);
    }

    // Send system message
    await this.sendSystemMessage(roomId, `${userId} left the room`, context);

    // Broadcast leave event
    this.broadcastToRoomSubscribers(roomId, {
      type: 'user_left_room',
      roomId,
      userId,
      timestamp: new Date().toISOString()
    }, context);

    console.log(`👋 User ${userId} left room "${roomId}"`);
    return true;
  }

  // Helper to get user's subscribed rooms
  static getUserSubscriptions(userId, context) {
    const subscriptions = context.userSubscriptions?.get(userId);
    if (!subscriptions) return [];

    const roomList = [];
    for (const roomId of subscriptions) {
      const room = context.chatRooms.get(roomId);
      if (room && room.isActive) {
        roomList.push({
          id: roomId,
          name: room.name,
          type: room.type,
          messageCount: room.messageCount,
          lastActivity: room.lastActivity,
          participants: room.participants.length,
          hasUnread: false // TODO: Track unread messages
        });
      }
    }

    return roomList;
  }
}

module.exports = JoinRoomCommand;