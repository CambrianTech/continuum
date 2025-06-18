/**
 * ListAgents Command - Query available agents and their status
 * Like /who or /users in chat systems
 */

const BaseCommand = require('../../BaseCommand.cjs');

class ListAgentsCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'listAgents',
      description: 'List available agents and their current status',
      icon: '🤖',
      category: 'communication',
      parameters: {
        status: {
          type: 'string',
          required: false,
          description: 'Filter by status: online, busy, idle, offline'
        },
        type: {
          type: 'string',
          required: false,
          description: 'Filter by type: ai, human, bot, system'
        },
        room: {
          type: 'string',
          required: false,
          description: 'Show agents in specific room'
        },
        capabilities: {
          type: 'boolean',
          required: false,
          default: false,
          description: 'Include agent capabilities and specializations'
        },
        detailed: {
          type: 'boolean',
          required: false,
          default: false,
          description: 'Show detailed agent information'
        }
      },
      examples: [
        {
          description: 'List all available agents',
          usage: 'listAgents'
        },
        {
          description: 'Show only online agents with capabilities',
          usage: 'listAgents --status online --capabilities true'
        },
        {
          description: 'List agents in specific room',
          usage: 'listAgents --room "project-alpha"'
        },
        {
          description: 'Show detailed AI agent info',
          usage: 'listAgents --type ai --detailed true'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { 
        status, 
        type, 
        room, 
        capabilities = false, 
        detailed = false 
      } = this.parseParams(paramsString);

      console.log(`🤖 ListAgents: Querying agents (status: ${status || 'all'}, type: ${type || 'all'})`);

      // Get all available agents
      const agents = await this.gatherAvailableAgents(context);

      // Apply filters
      let filteredAgents = agents;

      if (status) {
        filteredAgents = filteredAgents.filter(agent => agent.status === status);
      }

      if (type) {
        filteredAgents = filteredAgents.filter(agent => agent.type === type);
      }

      if (room) {
        const roomId = this.normalizeRoomName(room);
        const roomData = context.chatRooms?.get(roomId);
        if (roomData) {
          filteredAgents = filteredAgents.filter(agent => 
            roomData.participants.includes(agent.id) || roomData.agents.includes(agent.id)
          );
        } else {
          return this.createErrorResult(`Room "${room}" not found`);
        }
      }

      // Format agent data based on detail level
      const formattedAgents = filteredAgents.map(agent => {
        const basicInfo = {
          id: agent.id,
          name: agent.name,
          type: agent.type,
          status: agent.status,
          isOnline: agent.isOnline
        };

        if (capabilities || detailed) {
          basicInfo.capabilities = agent.capabilities;
          basicInfo.specializations = agent.specializations;
        }

        if (detailed) {
          basicInfo.description = agent.description;
          basicInfo.lastSeen = agent.lastSeen;
          basicInfo.responseTime = agent.responseTime;
          basicInfo.currentRooms = agent.currentRooms;
          basicInfo.totalMessages = agent.totalMessages;
        }

        return basicInfo;
      });

      console.log(`✅ ListAgents: Found ${filteredAgents.length} agents matching criteria`);

      return this.createSuccessResult({
        agents: formattedAgents,
        totalCount: filteredAgents.length,
        filters: { status, type, room },
        timestamp: new Date().toISOString()
      }, `Found ${filteredAgents.length} agents`);

    } catch (error) {
      console.error(`❌ ListAgents failed: ${error.message}`);
      return this.createErrorResult(`Failed to list agents: ${error.message}`);
    }
  }

  static async gatherAvailableAgents(context) {
    const agents = [];

    // Built-in AI agents
    const builtInAgents = [
      {
        id: 'PlannerAI',
        name: 'PlannerAI',
        type: 'ai',
        status: 'online',
        isOnline: true,
        description: 'Strategic planning and web commands',
        capabilities: ['planning', 'strategy', 'web-commands', 'project-management'],
        specializations: ['long-term-planning', 'resource-allocation', 'workflow-optimization'],
        responseTime: '~2s',
        lastSeen: new Date().toISOString(),
        totalMessages: 0,
        currentRooms: this.getAgentRooms('PlannerAI', context)
      },
      {
        id: 'CodeAI',
        name: 'CodeAI', 
        type: 'ai',
        status: 'online',
        isOnline: true,
        description: 'Code analysis, debugging, and development',
        capabilities: ['coding', 'debugging', 'code-review', 'architecture'],
        specializations: ['javascript', 'python', 'web-development', 'api-design'],
        responseTime: '~3s',
        lastSeen: new Date().toISOString(),
        totalMessages: 0,
        currentRooms: this.getAgentRooms('CodeAI', context)
      },
      {
        id: 'GeneralAI',
        name: 'GeneralAI',
        type: 'ai', 
        status: 'online',
        isOnline: true,
        description: 'General assistance and conversation',
        capabilities: ['general-help', 'conversation', 'research', 'writing'],
        specializations: ['natural-language', 'problem-solving', 'creative-writing'],
        responseTime: '~2s',
        lastSeen: new Date().toISOString(),
        totalMessages: 0,
        currentRooms: this.getAgentRooms('GeneralAI', context)
      },
      {
        id: 'ProtocolSheriff',
        name: 'Protocol Sheriff',
        type: 'system',
        status: 'online', 
        isOnline: true,
        description: 'Response validation and protocol enforcement',
        capabilities: ['validation', 'protocol-enforcement', 'security', 'compliance'],
        specializations: ['response-validation', 'security-checks', 'policy-enforcement'],
        responseTime: '~1s',
        lastSeen: new Date().toISOString(),
        totalMessages: 0,
        currentRooms: []
      }
    ];

    agents.push(...builtInAgents);

    // Academy-trained personas
    if (context.continuum?.academyInterface) {
      const trainedPersonas = await this.getTrainedPersonas(context);
      agents.push(...trainedPersonas);
    }

    // Remote agents
    if (context.continuum?.remoteAgentManager) {
      const remoteAgents = await this.getRemoteAgents(context);
      agents.push(...remoteAgents);
    }

    // Connected human users (if any)
    const humanUsers = await this.getConnectedHumans(context);
    agents.push(...humanUsers);

    return agents;
  }

  static getAgentRooms(agentId, context) {
    if (!context.chatRooms) return [];

    const rooms = [];
    for (const [roomId, roomData] of context.chatRooms.entries()) {
      if (roomData.agents.includes(agentId) || roomData.participants.includes(agentId)) {
        rooms.push({
          id: roomId,
          name: roomData.name,
          type: roomData.type
        });
      }
    }
    return rooms;
  }

  static async getTrainedPersonas(context) {
    const personas = [];
    
    if (context.continuum?.academyInterface?.completedPersonas) {
      for (const [personaId, session] of context.continuum.academyInterface.completedPersonas.entries()) {
        personas.push({
          id: personaId,
          name: session.personaName || personaId,
          type: 'persona',
          status: 'online',
          isOnline: true,
          description: `Academy-trained specialist: ${session.specialization}`,
          capabilities: [session.specialization, 'trained-responses'],
          specializations: [session.specialization],
          responseTime: '~2s',
          lastSeen: session.completedAt,
          totalMessages: 0,
          trainingRounds: session.rounds,
          currentRooms: this.getAgentRooms(personaId, context)
        });
      }
    }

    return personas;
  }

  static async getRemoteAgents(context) {
    const remoteAgents = [];
    
    // TODO: Query remote agent manager for connected agents
    // This would integrate with RemoteAgentManager to get live agent status
    
    return remoteAgents;
  }

  static async getConnectedHumans(context) {
    const humans = [];
    
    // Get connected human users from WebSocket connections
    if (context.webSocketServer) {
      const connectedClients = context.webSocketServer.getConnectedClients();
      
      for (const client of connectedClients) {
        if (client.userType === 'human' || !client.userType) {
          humans.push({
            id: client.userId || 'anonymous',
            name: client.userName || 'Anonymous User',
            type: 'human',
            status: 'online',
            isOnline: true,
            description: 'Human user',
            capabilities: ['conversation', 'commands', 'creativity'],
            specializations: ['human-insight', 'decision-making'],
            responseTime: 'variable',
            lastSeen: new Date().toISOString(),
            totalMessages: 0,
            currentRooms: this.getUserRooms(client.userId, context)
          });
        }
      }
    }

    return humans;
  }

  static getUserRooms(userId, context) {
    if (!context.userSubscriptions?.has(userId)) return [];

    const userRooms = [];
    const subscribedRoomIds = context.userSubscriptions.get(userId);

    for (const roomId of subscribedRoomIds) {
      const room = context.chatRooms.get(roomId);
      if (room && room.isActive) {
        userRooms.push({
          id: roomId,
          name: room.name,
          type: room.type
        });
      }
    }

    return userRooms;
  }

  static normalizeRoomName(name) {
    return name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .replace(/-+/g, '-');
  }
}

module.exports = ListAgentsCommand;