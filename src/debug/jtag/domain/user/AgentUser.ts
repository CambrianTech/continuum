/**
 * AgentUser - Specialized AI agent implementation
 *
 * Task-focused AI citizens with specific tool access and capabilities.
 * Examples: CodeAI (debugging), PlannerAI (strategy), Auto Route (routing)
 */

import { AIUser, type AIUserData, type AIModelConfig } from './AIUser';

/**
 * Agent-specific tool and API access configuration
 */
export interface AgentToolAccess {
  readonly toolId: string;
  readonly permissions: readonly string[];
  readonly rateLimit?: number;
  readonly scope?: 'read' | 'write' | 'execute';
}

/**
 * Agent specialization areas
 */
export type AgentSpecialization =
  | 'code-analysis'
  | 'debugging'
  | 'refactoring'
  | 'strategy-planning'
  | 'routing-selection'
  | 'data-processing'
  | 'system-monitoring'
  | 'security-analysis'
  | 'performance-optimization'
  | 'test-generation';

/**
 * Agent-specific data extending AI capabilities
 */
export interface AgentUserData extends AIUserData {
  readonly aiType: 'agent';
  readonly specialization: AgentSpecialization;
  readonly toolAccess: readonly AgentToolAccess[];
  readonly automationLevel: 'manual' | 'assisted' | 'autonomous';
  readonly maxConcurrentTasks: number;
}

/**
 * Agent User - Specialized AI with specific tool access
 *
 * Task-oriented AI citizens that perform specialized functions.
 * Each agent has specific tool permissions and operational constraints.
 */
export class AgentUser extends AIUser {
  declare protected readonly data: AgentUserData;

  constructor(data: AgentUserData) {
    super(data);
  }

  /**
   * Agent-specific implementations
   */
  getAdapterType(): string {
    return 'agent-specialized';
  }

  getInteractionModel(): 'agent-specialized' {
    return 'agent-specialized';
  }

  getCapabilitySet(): readonly string[] {
    const baseCapabilities = super.getCapabilitySet();
    const agentCapabilities = [
      'tool-execution',
      'task-automation',
      'specialized-analysis',
      'concurrent-processing',
      'result-validation',
      ...this.getSpecializationCapabilities()
    ];
    return [...baseCapabilities, ...agentCapabilities];
  }

  /**
   * Agent-specific accessors
   */
  get specialization(): AgentSpecialization {
    return this.data.specialization;
  }

  get toolAccess(): readonly AgentToolAccess[] {
    return this.data.toolAccess;
  }

  get automationLevel(): AgentUserData['automationLevel'] {
    return this.data.automationLevel;
  }

  get maxConcurrentTasks(): number {
    return this.data.maxConcurrentTasks;
  }

  /**
   * Agent-specific behaviors
   */
  hasToolAccess(toolId: string, permission: string): boolean {
    return this.toolAccess.some(access =>
      access.toolId === toolId &&
      access.permissions.includes(permission)
    );
  }

  canExecuteAutonomously(): boolean {
    return this.automationLevel === 'autonomous';
  }

  canProcessConcurrently(): boolean {
    return this.maxConcurrentTasks > 1;
  }

  updateAutomationLevel(level: AgentUserData['automationLevel']): AgentUser {
    const newData = {
      ...this.data,
      automationLevel: level
    };
    return new AgentUser(newData);
  }

  grantToolAccess(toolAccess: AgentToolAccess): AgentUser {
    const existingIndex = this.data.toolAccess.findIndex(access => access.toolId === toolAccess.toolId);

    let newToolAccess: readonly AgentToolAccess[];
    if (existingIndex >= 0) {
      // Update existing tool access
      newToolAccess = this.data.toolAccess.map((access, index) =>
        index === existingIndex ? toolAccess : access
      );
    } else {
      // Add new tool access
      newToolAccess = [...this.data.toolAccess, toolAccess];
    }

    const newData = {
      ...this.data,
      toolAccess: newToolAccess
    };
    return new AgentUser(newData);
  }

  revokeToolAccess(toolId: string): AgentUser {
    const newData = {
      ...this.data,
      toolAccess: this.data.toolAccess.filter(access => access.toolId !== toolId)
    };
    return new AgentUser(newData);
  }

  /**
   * Specialization-specific capabilities
   */
  private getSpecializationCapabilities(): readonly string[] {
    switch (this.specialization) {
      case 'code-analysis':
      case 'debugging':
      case 'refactoring':
        return ['code-parsing', 'syntax-analysis', 'bug-detection', 'refactor-suggestions'];

      case 'strategy-planning':
        return ['strategic-analysis', 'planning-optimization', 'resource-allocation'];

      case 'routing-selection':
        return ['agent-routing', 'task-delegation', 'load-balancing'];

      case 'data-processing':
        return ['data-transformation', 'batch-processing', 'data-validation'];

      case 'system-monitoring':
        return ['health-monitoring', 'performance-tracking', 'alert-generation'];

      case 'security-analysis':
        return ['vulnerability-scanning', 'threat-detection', 'security-auditing'];

      case 'performance-optimization':
        return ['performance-profiling', 'optimization-analysis', 'resource-optimization'];

      case 'test-generation':
        return ['test-creation', 'coverage-analysis', 'test-validation'];

      default:
        return [];
    }
  }

  /**
   * Factory methods
   */
  static fromData(data: AgentUserData): AgentUser {
    return new AgentUser(data);
  }

  static createNew(
    displayName: string,
    sessionId: string,
    specialization: AgentSpecialization,
    modelConfig: AIModelConfig
  ): AgentUser {
    const data: AgentUserData = {
      userId: crypto.randomUUID(),
      sessionId,
      displayName,
      citizenType: 'ai',
      aiType: 'agent',
      specialization,
      modelConfig,
      toolAccess: [],
      automationLevel: 'assisted',
      maxConcurrentTasks: 3,
      capabilities: [],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    };
    return new AgentUser(data);
  }

  /**
   * Immutable update implementation
   */
  protected createInstance(data: any): AgentUser {
    return new AgentUser(data as AgentUserData);
  }
}