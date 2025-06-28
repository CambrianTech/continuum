/**
 * Agent Configuration Data
 * Centralized agent definitions for the system
 */

export interface Agent {
  readonly id: string;
  readonly name: string;
  readonly role: string;
  readonly avatar: string;
  readonly status: 'online' | 'offline' | 'busy';
  readonly type: 'ai' | 'human';
  readonly capabilities?: string[];
  readonly description?: string;
}

export const defaultAgents: Agent[] = [
  {
    id: 'system',
    name: 'System',
    role: 'System Coordinator',
    avatar: '⚙️',
    status: 'online',
    type: 'ai',
    capabilities: ['system-management', 'coordination', 'monitoring'],
    description: 'System coordination and management'
  }
];

/**
 * Get all available agents
 */
export function getAgents(): Agent[] {
  return [...defaultAgents];
}

/**
 * Get agent by ID
 */
export function getAgentById(id: string): Agent | undefined {
  return defaultAgents.find(agent => agent.id === id);
}

/**
 * Add custom agent (for future extensibility)
 */
export function addAgent(agent: Agent): void {
  // TODO: Implement dynamic agent registration
  // This would integrate with the persona system for custom agents
}