/**
 * Remote Agent Manager
 * Handles connections from AI agents on other machines
 * Enables distributed AI workforce across multiple systems
 */

const WebSocket = require('ws');

class RemoteAgentManager {
  constructor(continuum) {
    this.continuum = continuum;
    this.remoteAgents = new Map(); // agentId -> agent connection info
    this.agentConnections = new Map(); // agentId -> websocket
    this.hostConnections = new Map(); // sessionId -> available agents from that host
    this.agentCapabilities = new Map(); // agentId -> capabilities list
    this.agentHeartbeats = new Map(); // agentId -> last heartbeat timestamp
    
    // Heartbeat monitoring
    this.heartbeatInterval = 30000; // 30 seconds
    this.setupHeartbeatMonitoring();
  }

  /**
   * Register a remote agent connection (AI or Human)
   */
  registerRemoteAgent(ws, agentInfo) {
    const {
      agentId,
      agentName,
      agentType = 'ai', // 'ai', 'human', 'user', 'system'
      capabilities = [],
      hostInfo = {},
      authToken = null,
      userInfo = {} // For human users: username, avatar, etc.
    } = agentInfo;

    // Validate agent registration
    if (!agentId || !agentName) {
      throw new Error('Agent ID and name are required');
    }

    // Check for existing agent with same ID
    if (this.remoteAgents.has(agentId)) {
      console.log(`⚠️  Agent ${agentId} already registered, updating connection...`);
      this.disconnectAgent(agentId);
    }

    // Store agent information
    const agent = {
      id: agentId,
      name: agentName,
      type: agentType,
      capabilities: capabilities,
      hostInfo: {
        hostname: hostInfo.hostname || 'unknown',
        platform: hostInfo.platform || 'unknown',
        version: hostInfo.version || 'unknown',
        ip: hostInfo.ip || 'unknown'
      },
      userInfo: agentType === 'human' || agentType === 'user' ? userInfo : null,
      status: 'connected',
      connectedAt: Date.now(),
      lastSeen: Date.now(),
      authToken: authToken,
      messageCount: 0,
      errorCount: 0
    };

    this.remoteAgents.set(agentId, agent);
    this.agentConnections.set(agentId, ws);
    this.agentCapabilities.set(agentId, capabilities);
    this.agentHeartbeats.set(agentId, Date.now());

    const agentTypeLabel = agentType === 'human' || agentType === 'user' ? 'Human Operator' : 'AI Agent';
    console.log(`🌐 ${agentTypeLabel} connected: ${agentName} (${agentId}) from ${agent.hostInfo.hostname}`);
    console.log(`📊 ${agentType === 'human' ? 'Skills' : 'Capabilities'}: ${capabilities.join(', ') || 'general'}`);

    // Notify all connected clients about new agent
    this.broadcastAgentUpdate();

    // Send acknowledgment to agent
    this.sendToAgent(agentId, {
      type: 'registration_ack',
      status: 'success',
      assignedId: agentId,
      serverInfo: {
        version: require('../../package.json').version,
        timestamp: Date.now()
      }
    });

    return agent;
  }

  /**
   * Handle agent disconnection
   */
  disconnectAgent(agentId) {
    const agent = this.remoteAgents.get(agentId);
    if (!agent) return false;

    console.log(`👋 Remote agent disconnected: ${agent.name} (${agentId})`);

    // Clean up all data structures
    this.remoteAgents.delete(agentId);
    this.agentConnections.delete(agentId);
    this.agentCapabilities.delete(agentId);
    this.agentHeartbeats.delete(agentId);

    // Notify clients about agent departure
    this.broadcastAgentUpdate();

    return true;
  }

  /**
   * Send message to specific remote agent
   */
  async sendToAgent(agentId, message) {
    const ws = this.agentConnections.get(agentId);
    const agent = this.remoteAgents.get(agentId);

    if (!ws || !agent) {
      throw new Error(`Agent ${agentId} not found or not connected`);
    }

    try {
      // Add message metadata
      const fullMessage = {
        ...message,
        timestamp: Date.now(),
        serverId: this.continuum.sessionId || 'continuum-server'
      };

      ws.send(JSON.stringify(fullMessage));
      
      // Update agent stats
      agent.messageCount++;
      agent.lastSeen = Date.now();
      this.agentHeartbeats.set(agentId, Date.now());

      return true;
    } catch (error) {
      console.error(`❌ Failed to send message to agent ${agentId}:`, error.message);
      agent.errorCount++;
      
      // If too many errors, disconnect agent
      if (agent.errorCount > 5) {
        console.log(`🚨 Too many errors for agent ${agentId}, disconnecting...`);
        this.disconnectAgent(agentId);
      }
      
      throw error;
    }
  }

  /**
   * Send task to remote agent and wait for response
   */
  async sendTaskToAgent(agentId, task, timeout = 30000) {
    const agent = this.remoteAgents.get(agentId);
    if (!agent) {
      throw new Error(`Agent ${agentId} not found`);
    }

    return new Promise((resolve, reject) => {
      const taskId = `task_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      // Set up timeout
      const timeoutId = setTimeout(() => {
        reject(new Error(`Task timeout: Agent ${agentId} did not respond within ${timeout}ms`));
      }, timeout);

      // Set up response handler
      const responseHandler = (response) => {
        if (response.taskId === taskId) {
          clearTimeout(timeoutId);
          if (response.error) {
            reject(new Error(response.error));
          } else {
            resolve(response.result);
          }
        }
      };

      // Store response handler (simplified - in real implementation would use event emitters)
      this.pendingTasks = this.pendingTasks || new Map();
      this.pendingTasks.set(taskId, responseHandler);

      // Send task to agent
      this.sendToAgent(agentId, {
        type: 'task',
        taskId: taskId,
        task: task,
        priority: 'normal',
        timeout: timeout
      }).catch(reject);
    });
  }

  /**
   * Handle messages from remote agents
   */
  handleAgentMessage(ws, message, sessionId) {
    try {
      const data = JSON.parse(message);
      
      switch (data.type) {
        case 'agent_register':
          this.registerRemoteAgent(ws, data.agentInfo);
          break;
          
        case 'heartbeat':
          this.handleHeartbeat(data.agentId);
          break;
          
        case 'task_response':
          this.handleTaskResponse(data);
          break;
          
        case 'agent_status_update':
          this.handleStatusUpdate(data.agentId, data.status);
          break;
          
        case 'capability_update':
          this.handleCapabilityUpdate(data.agentId, data.capabilities);
          break;
          
        default:
          console.log(`⚠️  Unknown message type from agent: ${data.type}`);
      }
    } catch (error) {
      console.error('❌ Error handling agent message:', error.message);
    }
  }

  /**
   * Handle heartbeat from agent
   */
  handleHeartbeat(agentId) {
    const agent = this.remoteAgents.get(agentId);
    if (agent) {
      agent.lastSeen = Date.now();
      this.agentHeartbeats.set(agentId, Date.now());
      agent.status = 'connected';
    }
  }

  /**
   * Handle task response from agent
   */
  handleTaskResponse(data) {
    const { taskId, result, error, agentId } = data;
    
    // Find and call response handler
    if (this.pendingTasks && this.pendingTasks.has(taskId)) {
      const handler = this.pendingTasks.get(taskId);
      this.pendingTasks.delete(taskId);
      handler({ taskId, result, error });
    }

    // Update agent stats
    const agent = this.remoteAgents.get(agentId);
    if (agent) {
      agent.lastSeen = Date.now();
      if (error) {
        agent.errorCount++;
      }
    }
  }

  /**
   * Handle agent status updates
   */
  handleStatusUpdate(agentId, status) {
    const agent = this.remoteAgents.get(agentId);
    if (agent) {
      agent.status = status;
      agent.lastSeen = Date.now();
      this.broadcastAgentUpdate();
    }
  }

  /**
   * Handle capability updates
   */
  handleCapabilityUpdate(agentId, capabilities) {
    const agent = this.remoteAgents.get(agentId);
    if (agent) {
      agent.capabilities = capabilities;
      this.agentCapabilities.set(agentId, capabilities);
      agent.lastSeen = Date.now();
      this.broadcastAgentUpdate();
    }
  }

  /**
   * Get all available agents (local + remote)
   */
  getAllAvailableAgents() {
    const localAgents = this.getLocalAgents();
    const remoteAgents = Array.from(this.remoteAgents.values()).map(agent => ({
      id: agent.id,
      name: agent.name,
      type: agent.type,
      capabilities: agent.capabilities,
      status: agent.status,
      source: 'remote',
      hostInfo: agent.hostInfo,
      lastSeen: agent.lastSeen,
      messageCount: agent.messageCount,
      errorCount: agent.errorCount
    }));

    return [...localAgents, ...remoteAgents];
  }

  /**
   * Get local agents from the main continuum system
   */
  getLocalAgents() {
    // Get available local AI models/agents
    const localAgents = [];
    
    // Add Claude agents if available
    if (this.continuum.anthropic) {
      localAgents.push({
        id: 'claude-sonnet',
        name: 'Claude Sonnet',
        type: 'ai',
        capabilities: ['general', 'code', 'analysis', 'reasoning'],
        status: 'available',
        source: 'local'
      });
    }

    // Add GPT agents if available
    if (this.continuum.openai) {
      localAgents.push({
        id: 'gpt-4',
        name: 'GPT-4',
        type: 'ai', 
        capabilities: ['general', 'code', 'creative', 'analysis'],
        status: 'available',
        source: 'local'
      });
    }

    // Add system agents
    localAgents.push({
      id: 'system',
      name: 'System',
      type: 'system',
      capabilities: ['commands', 'file-operations', 'system-info'],
      status: 'available',
      source: 'local'
    });

    return localAgents;
  }

  /**
   * Broadcast agent updates to all connected clients
   */
  broadcastAgentUpdate() {
    const allAgents = this.getAllAvailableAgents();
    
    // Send to all connected WebSocket clients
    this.continuum.activeConnections.forEach((ws, sessionId) => {
      try {
        ws.send(JSON.stringify({
          type: 'agents_update',
          agents: allAgents,
          timestamp: Date.now()
        }));
      } catch (error) {
        console.error(`Failed to send agent update to session ${sessionId}:`, error.message);
      }
    });
  }

  /**
   * Setup heartbeat monitoring
   */
  setupHeartbeatMonitoring() {
    setInterval(() => {
      const now = Date.now();
      const timeoutThreshold = this.heartbeatInterval * 2; // 60 seconds

      this.agentHeartbeats.forEach((lastHeartbeat, agentId) => {
        if (now - lastHeartbeat > timeoutThreshold) {
          console.log(`💔 Agent ${agentId} heartbeat timeout, disconnecting...`);
          this.disconnectAgent(agentId);
        }
      });
    }, this.heartbeatInterval);
  }

  /**
   * Find agents by capability
   */
  findAgentsByCapability(capability) {
    const matchingAgents = [];
    
    this.remoteAgents.forEach((agent, agentId) => {
      if (agent.capabilities.includes(capability) && agent.status === 'connected') {
        matchingAgents.push(agent);
      }
    });

    return matchingAgents;
  }

  /**
   * Get agent statistics
   */
  getAgentStats() {
    const stats = {
      totalAgents: this.remoteAgents.size,
      connectedAgents: 0,
      totalMessages: 0,
      totalErrors: 0,
      capabilities: new Set(),
      hosts: new Set()
    };

    this.remoteAgents.forEach((agent) => {
      if (agent.status === 'connected') {
        stats.connectedAgents++;
      }
      stats.totalMessages += agent.messageCount;
      stats.totalErrors += agent.errorCount;
      agent.capabilities.forEach(cap => stats.capabilities.add(cap));
      stats.hosts.add(agent.hostInfo.hostname);
    });

    return {
      ...stats,
      capabilities: Array.from(stats.capabilities),
      hosts: Array.from(stats.hosts)
    };
  }

  /**
   * Send task to best available agent for capability
   */
  async routeTaskByCapability(capability, task, timeout = 30000) {
    const availableAgents = this.findAgentsByCapability(capability);
    
    if (availableAgents.length === 0) {
      throw new Error(`No agents available with capability: ${capability}`);
    }

    // Sort by message count (load balancing)
    availableAgents.sort((a, b) => a.messageCount - b.messageCount);
    const selectedAgent = availableAgents[0];

    console.log(`🎯 Routing ${capability} task to ${selectedAgent.name} (${selectedAgent.id})`);
    
    return await this.sendTaskToAgent(selectedAgent.id, task, timeout);
  }
}

module.exports = RemoteAgentManager;