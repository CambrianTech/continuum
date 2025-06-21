/**
 * ListRooms Command - View available chat rooms
 * Like Discord's channel list or IRC's /list
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class ListRoomsCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'listRooms',
      description: 'List available chat rooms and your subscriptions',
      icon: '📋',
      category: 'communication',
      parameters: {
        type: {
          type: 'string',
          required: false,
          description: 'Filter by room type: public, private, dm, group'
        },
        subscribed: {
          type: 'boolean',
          required: false,
          description: 'Show only rooms you are subscribed to'
        },
        active: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Show only active rooms'
        },
        detailed: {
          type: 'boolean',
          required: false,
          default: false,
          description: 'Include detailed room information'
        }
      },
      examples: [
        {
          description: 'List all available rooms',
          usage: 'listRooms'
        },
        {
          description: 'Show only your subscribed rooms',
          usage: 'listRooms --subscribed true'
        },
        {
          description: 'List public rooms with details',
          usage: 'listRooms --type public --detailed true'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { 
        type, 
        subscribed, 
        active = true, 
        detailed = false 
      } = this.parseParams(paramsString);

      const userId = 'user'; // TODO: Get actual user ID

      console.log(`📋 ListRooms: Querying rooms (type: ${type || 'all'}, subscribed: ${subscribed || 'all'})`);

      if (!context.chatRooms) {
        return this.createSuccessResult({
          rooms: [],
          totalCount: 0,
          subscribedCount: 0
        }, 'No chat rooms available');
      }

      let rooms = Array.from(context.chatRooms.values());

      // Apply filters
      if (active) {
        rooms = rooms.filter(room => room.isActive !== false);
      }

      if (type) {
        rooms = rooms.filter(room => room.type === type);
      }

      if (subscribed) {
        const userSubs = context.userSubscriptions?.get(userId) || new Set();
        rooms = rooms.filter(room => userSubs.has(room.id));
      }

      // Format room data
      const formattedRooms = rooms.map(room => {
        const basicInfo = {
          id: room.id,
          name: room.name,
          type: room.type,
          participantCount: room.participants.length,
          messageCount: room.messageCount,
          lastActivity: room.lastActivity,
          isSubscribed: context.userSubscriptions?.get(userId)?.has(room.id) || false
        };

        if (detailed) {
          basicInfo.topic = room.topic;
          basicInfo.creator = room.creator;
          basicInfo.participants = room.participants;
          basicInfo.agents = room.agents;
          basicInfo.createdAt = room.createdAt;
          basicInfo.permissions = room.permissions;
          basicInfo.recentMessages = room.messages.slice(-3).map(msg => ({
            sender: msg.sender,
            content: msg.content.substring(0, 50) + (msg.content.length > 50 ? '...' : ''),
            timestamp: msg.timestamp
          }));
        }

        return basicInfo;
      });

      // Sort by activity
      formattedRooms.sort((a, b) => new Date(b.lastActivity) - new Date(a.lastActivity));

      const subscribedCount = formattedRooms.filter(room => room.isSubscribed).length;

      console.log(`✅ ListRooms: Found ${formattedRooms.length} rooms (${subscribedCount} subscribed)`);

      return this.createSuccessResult({
        rooms: formattedRooms,
        totalCount: formattedRooms.length,
        subscribedCount,
        filters: { type, subscribed, active, detailed },
        timestamp: new Date().toISOString()
      }, `Found ${formattedRooms.length} rooms`);

    } catch (error) {
      console.error(`❌ ListRooms failed: ${error.message}`);
      return this.createErrorResult(`Failed to list rooms: ${error.message}`);
    }
  }
}

module.exports = ListRoomsCommand;