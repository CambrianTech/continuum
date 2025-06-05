/**
 * AgentSelector Utility Functions
 * Modular utilities for agent management and filtering
 */

export class AgentLogger {
  constructor(componentName = 'AgentSelector') {
    this.componentName = componentName;
    this.logLevel = 'INFO'; // DEBUG, INFO, WARN, ERROR
  }

  debug(message, data = {}) {
    if (this.logLevel === 'DEBUG') {
      console.debug(`[${this.componentName}] ${message}`, data);
    }
  }

  info(message, data = {}) {
    if (['DEBUG', 'INFO'].includes(this.logLevel)) {
      console.log(`[${this.componentName}] ${message}`, data);
    }
  }

  warn(message, data = {}) {
    if (['DEBUG', 'INFO', 'WARN'].includes(this.logLevel)) {
      console.warn(`[${this.componentName}] ${message}`, data);
    }
  }

  error(message, data = {}) {
    console.error(`[${this.componentName}] ${message}`, data);
  }

  setLogLevel(level) {
    this.logLevel = level;
    this.info('Log level changed', { level });
  }
}

export class AgentFilter {
  static filterBySearch(agents, searchQuery) {
    if (!searchQuery) return agents;
    
    const query = searchQuery.toLowerCase();
    return agents.filter(agent => 
      agent.name.toLowerCase().includes(query) ||
      agent.role.toLowerCase().includes(query) ||
      (agent.hostInfo?.hostname || '').toLowerCase().includes(query)
    );
  }

  static filterByType(agents, type) {
    return agents.filter(agent => agent.type === type);
  }

  static filterByStatus(agents, status) {
    return agents.filter(agent => agent.status === status);
  }

  static filterByFavorites(agents, favoriteSet) {
    return agents.filter(agent => favoriteSet.has(agent.id));
  }

  static sortByActivity(agents) {
    return [...agents].sort((a, b) => {
      const aTime = new Date(a.lastActive || 0).getTime();
      const bTime = new Date(b.lastActive || 0).getTime();
      return bTime - aTime;
    });
  }

  static sortByName(agents) {
    return [...agents].sort((a, b) => a.name.localeCompare(b.name));
  }

  static groupByType(agents) {
    return agents.reduce((groups, agent) => {
      const type = agent.type || 'unknown';
      if (!groups[type]) groups[type] = [];
      groups[type].push(agent);
      return groups;
    }, {});
  }
}

export class AgentMetrics {
  constructor() {
    this.metrics = new Map();
  }

  trackSelection(agentId) {
    const current = this.metrics.get(agentId) || { selections: 0, lastSelected: null };
    current.selections++;
    current.lastSelected = new Date().toISOString();
    this.metrics.set(agentId, current);
  }

  trackInteraction(agentId, type = 'general') {
    const current = this.metrics.get(agentId) || { interactions: {}, lastInteraction: null };
    if (!current.interactions) current.interactions = {};
    if (!current.interactions[type]) current.interactions[type] = 0;
    current.interactions[type]++;
    current.lastInteraction = new Date().toISOString();
    this.metrics.set(agentId, current);
  }

  getPopularAgents(limit = 5) {
    return Array.from(this.metrics.entries())
      .map(([id, data]) => ({ id, ...data }))
      .sort((a, b) => (b.selections || 0) - (a.selections || 0))
      .slice(0, limit);
  }

  getAgentStats(agentId) {
    return this.metrics.get(agentId) || null;
  }

  exportMetrics() {
    return Object.fromEntries(this.metrics);
  }

  importMetrics(data) {
    this.metrics = new Map(Object.entries(data));
  }
}

export class AgentValidator {
  static validateAgent(agent) {
    const required = ['id', 'name', 'role'];
    const missing = required.filter(field => !agent[field]);
    
    if (missing.length > 0) {
      throw new Error(`Agent missing required fields: ${missing.join(', ')}`);
    }

    return {
      isValid: true,
      warnings: AgentValidator.getWarnings(agent)
    };
  }

  static getWarnings(agent) {
    const warnings = [];
    
    if (!agent.avatar) warnings.push('No avatar specified');
    if (!agent.gradient) warnings.push('No gradient specified');
    if (!agent.status) warnings.push('No status specified');
    if (agent.type && !['ai', 'human', 'user', 'assistant', 'system'].includes(agent.type)) {
      warnings.push(`Unknown agent type: ${agent.type}`);
    }

    return warnings;
  }

  static sanitizeAgent(agent) {
    const base = {
      id: String(agent.id || '').trim(),
      name: String(agent.name || '').trim(),
      role: String(agent.role || '').trim(),
      avatar: agent.avatar || '🤖',
      gradient: agent.gradient || 'linear-gradient(135deg, #666, #888)',
      status: agent.status || 'offline',
      type: agent.type || 'ai'
    };
    
    // Add any additional properties from the original agent
    Object.keys(agent).forEach(key => {
      if (!base.hasOwnProperty(key)) {
        base[key] = agent[key];
      }
    });
    
    return base;
  }
}

export class AgentStorage {
  constructor(storageKey = 'continuum-agent-preferences') {
    this.storageKey = storageKey;
  }

  saveFavorites(favoriteSet) {
    try {
      const favorites = Array.from(favoriteSet);
      localStorage.setItem(`${this.storageKey}-favorites`, JSON.stringify(favorites));
    } catch (error) {
      console.warn('Failed to save favorites to localStorage:', error);
    }
  }

  loadFavorites() {
    try {
      const stored = localStorage.getItem(`${this.storageKey}-favorites`);
      return stored ? new Set(JSON.parse(stored)) : new Set();
    } catch (error) {
      console.warn('Failed to load favorites from localStorage:', error);
      return new Set();
    }
  }

  saveMetrics(metrics) {
    try {
      localStorage.setItem(`${this.storageKey}-metrics`, JSON.stringify(metrics.exportMetrics()));
    } catch (error) {
      console.warn('Failed to save metrics to localStorage:', error);
    }
  }

  loadMetrics() {
    try {
      const stored = localStorage.getItem(`${this.storageKey}-metrics`);
      if (stored) {
        const metrics = new AgentMetrics();
        metrics.importMetrics(JSON.parse(stored));
        return metrics;
      }
    } catch (error) {
      console.warn('Failed to load metrics from localStorage:', error);
    }
    return new AgentMetrics();
  }

  clear() {
    try {
      localStorage.removeItem(`${this.storageKey}-favorites`);
      localStorage.removeItem(`${this.storageKey}-metrics`);
    } catch (error) {
      console.warn('Failed to clear storage:', error);
    }
  }
}