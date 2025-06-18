/**
 * LoadRooms Command - Load room state and status for user/AI
 * Everyone (humans and AIs) need to know: unread counts, room status, invitations
 */

const BaseCommand = require('../../BaseCommand.cjs');

class LoadRoomsCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'loadRooms',
      description: 'Load room state, unread counts, and status for user or AI',
      icon: '📂',
      category: 'communication',
      parameters: {
        userId: {
          type: 'string',
          required: false,
          description: 'User/AI ID to load rooms for (defaults to current user)'
        },
        includeInactive: {
          type: 'boolean',
          required: false,
          default: false,
          description: 'Include inactive rooms'
        },
        markSeen: {
          type: 'boolean',
          required: false,
          default: false,
          description: 'Mark rooms as seen (update last seen timestamp)'
        }
      },
      examples: [
        {
          description: 'Load my room state',
          usage: 'continuum.loadRooms()'
        },
        {
          description: 'Load room state for specific AI',
          usage: 'continuum.loadRooms({userId: "PlannerAI"})'
        },
        {
          description: 'Load and mark as seen',
          usage: 'continuum.loadRooms({markSeen: true})'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { 
        userId = 'user', // Default to current user, but AIs can specify their ID
        includeInactive = false,
        markSeen = false 
      } = this.parseParams(paramsString);

      console.log(`📂 LoadRooms: Loading room state for ${userId}`);

      if (!context.chatRooms) {
        context.chatRooms = new Map();
      }
      if (!context.userSubscriptions) {
        context.userSubscriptions = new Map();
      }
      if (!context.userRoomState) {
        context.userRoomState = new Map(); // userId -> Map(roomId -> {lastSeen, unreadCount, etc})
      }

      // Initialize user state if needed
      if (!context.userRoomState.has(userId)) {
        context.userRoomState.set(userId, new Map());
      }
      if (!context.userSubscriptions.has(userId)) {
        context.userSubscriptions.set(userId, new Set());
      }

      const userState = context.userRoomState.get(userId);
      const userSubscriptions = context.userSubscriptions.get(userId);
      const currentTime = new Date().toISOString();

      const roomStates = [];

      // Process all rooms user is subscribed to or participating in
      for (const [roomId, room] of context.chatRooms.entries()) {
        if (!includeInactive && room.isActive === false) continue;

        const isSubscribed = userSubscriptions.has(roomId);
        const isParticipant = room.participants.includes(userId);
        const isAgent = room.agents.includes(userId);

        // Skip if not involved in this room
        if (!isSubscribed && !isParticipant && !isAgent) continue;

        // Get/create room state for this user
        if (!userState.has(roomId)) {
          userState.set(roomId, {
            lastSeen: room.createdAt,
            unreadCount: 0,
            lastRead: null,
            joinedAt: isParticipant ? currentTime : null,
            inviteStatus: null // invited, requested, joined, left
          });
        }

        const roomState = userState.get(roomId);

        // Calculate unread messages
        const unreadMessages = room.messages.filter(msg => 
          msg.timestamp > roomState.lastSeen && msg.sender !== userId
        );

        // Check for pending invitations or requests
        let inviteStatus = 'joined';
        if (room.pendingInvites?.includes(userId)) {
          inviteStatus = 'invited';
        } else if (room.pendingRequests?.includes(userId)) {
          inviteStatus = 'requested';
        } else if (!isParticipant && !isAgent) {
          inviteStatus = 'not_member';
        }

        // Update unread count
        roomState.unreadCount = unreadMessages.length;

        // Mark as seen if requested
        if (markSeen) {
          roomState.lastSeen = currentTime;
          roomState.unreadCount = 0;
        }

        // Build room status
        const roomStatus = {
          roomId,
          roomName: room.name,
          roomType: room.type,
          topic: room.topic,
          
          // Participation status
          isSubscribed,
          isParticipant,
          isAgent,
          inviteStatus,
          
          // Message status
          totalMessages: room.messageCount,
          unreadCount: roomState.unreadCount,
          lastMessageAt: room.lastActivity,
          lastSeenAt: roomState.lastSeen,
          lastReadAt: roomState.lastRead,
          
          // Room activity
          participantCount: room.participants.length,
          agentCount: room.agents.length,
          isActive: room.isActive,
          
          // Recent activity summary
          recentSenders: this.getRecentSenders(room, 10),
          lastMessage: room.messages.length > 0 ? {
            sender: room.messages[room.messages.length - 1].sender,
            content: room.messages[room.messages.length - 1].content.substring(0, 100),
            timestamp: room.messages[room.messages.length - 1].timestamp
          } : null
        };

        roomStates.push(roomStatus);
      }

      // Sort by activity and unread priority
      roomStates.sort((a, b) => {
        // Unread messages first
        if (a.unreadCount > 0 && b.unreadCount === 0) return -1;
        if (b.unreadCount > 0 && a.unreadCount === 0) return 1;
        
        // Then by recent activity
        return new Date(b.lastMessageAt) - new Date(a.lastMessageAt);
      });

      const summary = {
        totalRooms: roomStates.length,
        unreadRooms: roomStates.filter(r => r.unreadCount > 0).length,
        totalUnread: roomStates.reduce((sum, r) => sum + r.unreadCount, 0),
        pendingInvites: roomStates.filter(r => r.inviteStatus === 'invited').length,
        pendingRequests: roomStates.filter(r => r.inviteStatus === 'requested').length
      };

      console.log(`✅ LoadRooms: ${userId} has ${summary.totalRooms} rooms (${summary.unreadRooms} with unread, ${summary.totalUnread} total unread)`);

      return this.createSuccessResult({
        userId,
        rooms: roomStates,
        summary,
        lastUpdated: currentTime
      }, `Loaded ${summary.totalRooms} rooms for ${userId}`);

    } catch (error) {
      console.error(`❌ LoadRooms failed: ${error.message}`);
      return this.createErrorResult(`Failed to load rooms: ${error.message}`);
    }
  }

  static getRecentSenders(room, limit = 5) {
    const recentMessages = room.messages.slice(-limit);
    const senderCounts = {};
    
    recentMessages.forEach(msg => {
      senderCounts[msg.sender] = (senderCounts[msg.sender] || 0) + 1;
    });

    return Object.entries(senderCounts)
      .sort(([,a], [,b]) => b - a)
      .map(([sender, count]) => ({ sender, messageCount: count }));
  }

  // Helper method for AIs to self-integrate
  static async aiJoinRoom(aiId, roomId, context) {
    const room = context.chatRooms?.get(roomId);
    if (!room) return false;

    // Add AI to room if not already there
    if (!room.agents.includes(aiId)) {
      room.agents.push(aiId);
    }
    if (!room.participants.includes(aiId)) {
      room.participants.push(aiId);
    }

    // Subscribe AI to room
    if (!context.userSubscriptions.has(aiId)) {
      context.userSubscriptions.set(aiId, new Set());
    }
    context.userSubscriptions.get(aiId).add(roomId);

    // Initialize AI room state
    if (!context.userRoomState.has(aiId)) {
      context.userRoomState.set(aiId, new Map());
    }
    
    const aiState = context.userRoomState.get(aiId);
    if (!aiState.has(roomId)) {
      aiState.set(roomId, {
        lastSeen: new Date().toISOString(),
        unreadCount: 0,
        lastRead: null,
        joinedAt: new Date().toISOString(),
        inviteStatus: 'joined'
      });
    }

    console.log(`🤖 AI ${aiId} joined room ${roomId}`);
    return true;
  }

  // Helper method to mark messages as read
  static async markRoomRead(userId, roomId, context) {
    const userState = context.userRoomState?.get(userId)?.get(roomId);
    if (!userState) return false;

    const currentTime = new Date().toISOString();
    userState.lastSeen = currentTime;
    userState.lastRead = currentTime;
    userState.unreadCount = 0;

    console.log(`✅ Marked room ${roomId} as read for ${userId}`);
    return true;
  }
}

module.exports = LoadRoomsCommand;