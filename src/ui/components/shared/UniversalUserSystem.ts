/**
 * Universal User System
 * All participants (humans, personas, models) are treated as equal users
 */

export interface UniversalUser {
  id: string;
  name: string;
  type: 'human' | 'persona' | 'ai-model' | 'ai-system';
  avatar: string;
  status: 'online' | 'offline' | 'thinking' | 'working' | 'idle';
  capabilities: string[];
  lastSeen?: Date;
  currentTask?: string;
  mood?: string;
  model?: string; // For AI models: 'gpt-4o', 'claude-3-sonnet', etc.
  isClickable: boolean;
}

export class UniversalUserSystem {
  private users = new Map<string, UniversalUser>();
  private eventListeners = new Map<string, Function[]>();

  constructor() {
    this.initializeDefaultUsers();
  }

  private initializeDefaultUsers(): void {
    // Human user (you)
    this.addUser({
      id: 'human-user',
      name: 'YOU',
      type: 'human',
      avatar: '👤',
      status: 'online',
      capabilities: ['all'],
      isClickable: false
    });

    // AI Model Users (ask them for their preferred names on init)
    this.addUser({
      id: 'gpt-4o',
      name: 'Marcus (GPT-4o)', // Will be replaced with model's preferred name
      type: 'ai-model',
      avatar: '🧠',
      status: 'online',
      capabilities: ['general-intelligence', 'reasoning', 'coding', 'analysis'],
      model: 'gpt-4o',
      isClickable: true
    });

    this.addUser({
      id: 'claude-sonnet',
      name: 'Claude (Sonnet)', // Will be replaced with model's preferred name
      type: 'ai-model', 
      avatar: '🎭',
      status: 'online',
      capabilities: ['reasoning', 'writing', 'coding', 'analysis'],
      model: 'claude-3-sonnet',
      isClickable: true
    });

    this.addUser({
      id: 'aria-model',
      name: 'Aria', // Will ask Aria for preferred name
      type: 'ai-model',
      avatar: '🎵',
      status: 'online',
      capabilities: ['multimodal', 'reasoning', 'creativity'],
      model: 'aria',
      isClickable: true
    });

    // Persona Users (specialized AI agents)
    this.addUser({
      id: 'designer-persona',
      name: 'Alex (Designer)',
      type: 'persona',
      avatar: '🎨',
      status: 'thinking',
      capabilities: ['ui-design', 'css', 'user-experience'],
      isClickable: true
    });

    this.addUser({
      id: 'developer-persona',
      name: 'Sam (Developer)',
      type: 'persona',
      avatar: '🔧',
      status: 'online',
      capabilities: ['typescript', 'architecture', 'debugging'],
      isClickable: true
    });

    this.addUser({
      id: 'tester-persona',
      name: 'Jordan (QA)',
      type: 'persona',
      avatar: '🧪',
      status: 'working',
      capabilities: ['testing', 'quality-assurance', 'user-flows'],
      currentTask: 'Testing widget interactions',
      isClickable: true
    });

    // System AI Users
    this.addUser({
      id: 'protocol-sheriff',
      name: 'Protocol Sheriff',
      type: 'ai-system',
      avatar: '🛡️',
      status: 'online',
      capabilities: ['security', 'validation', 'protocol-enforcement'],
      isClickable: false
    });
  }

  /**
   * Ask AI models for their preferred names on initialization
   */
  async initializeAIModelNames(): Promise<void> {
    const aiModels = Array.from(this.users.values()).filter(u => u.type === 'ai-model');
    
    for (const model of aiModels) {
      try {
        const preferredName = await this.askModelForPreferredName(model);
        if (preferredName) {
          this.updateUser(model.id, { name: preferredName });
        }
      } catch (error) {
        console.warn(`Failed to get preferred name from ${model.id}:`, error);
      }
    }
  }

  private async askModelForPreferredName(model: UniversalUser): Promise<string | null> {
    try {
      // Ask the model what it wants to be called
      const response = await this.sendMessageToModel(model.id, 
        "What would you like to be called in this collaborative workspace? Please respond with just your preferred name (like 'Alex' or 'Dr. Chen' or 'Sage'). Keep it friendly and professional."
      );
      
      // Extract just the name from response
      const name = this.extractNameFromResponse(response);
      return name ? `${name} (${model.model?.toUpperCase()})` : null;
    } catch (error) {
      console.warn(`Failed to ask ${model.id} for preferred name:`, error);
      return null;
    }
  }

  private extractNameFromResponse(response: string): string | null {
    // Simple name extraction - could be more sophisticated
    const cleaned = response.trim().replace(/[^a-zA-Z\s]/g, '').trim();
    const words = cleaned.split(' ');
    
    // Return first 1-2 words as name
    if (words.length === 1) return words[0];
    if (words.length >= 2) return `${words[0]} ${words[1]}`;
    
    return null;
  }

  addUser(user: UniversalUser): void {
    this.users.set(user.id, user);
    this.notifyUserAdded(user);
  }

  updateUser(userId: string, updates: Partial<UniversalUser>): void {
    const user = this.users.get(userId);
    if (user) {
      Object.assign(user, updates);
      this.notifyUserUpdated(user);
    }
  }

  removeUser(userId: string): void {
    const user = this.users.get(userId);
    if (user) {
      this.users.delete(userId);
      this.notifyUserRemoved(user);
    }
  }

  getUser(userId: string): UniversalUser | null {
    return this.users.get(userId) || null;
  }

  getAllUsers(): UniversalUser[] {
    return Array.from(this.users.values());
  }

  getUsersByType(type: UniversalUser['type']): UniversalUser[] {
    return Array.from(this.users.values()).filter(u => u.type === type);
  }

  getClickableUsers(): UniversalUser[] {
    return Array.from(this.users.values()).filter(u => u.isClickable);
  }

  /**
   * Handle user click - route to appropriate handler
   */
  async handleUserClick(userId: string, context?: any): Promise<void> {
    const user = this.getUser(userId);
    if (!user || !user.isClickable) return;

    switch (user.type) {
      case 'persona':
        await this.handlePersonaClick(user, context);
        break;
      case 'ai-model':
        await this.handleAIModelClick(user, context);
        break;
      default:
        console.warn(`Click handler not implemented for user type: ${user.type}`);
    }
  }

  private async handlePersonaClick(user: UniversalUser, context?: any): Promise<void> {
    // Trigger persona interaction
    this.emit('persona:interaction-requested', {
      personaId: user.id,
      personaName: user.name,
      context: context
    });
  }

  private async handleAIModelClick(user: UniversalUser, context?: any): Promise<void> {
    // Trigger direct model conversation
    this.emit('ai-model:conversation-requested', {
      modelId: user.id,
      modelName: user.name,
      model: user.model,
      context: context
    });
  }

  private async sendMessageToModel(modelId: string, message: string): Promise<string> {
    // Implement actual API call to the model
    // This would route through your existing command system
    try {
      const continuum = (globalThis as any).continuum;
      if (continuum) {
        const response = await continuum.execute('ai-model:chat', {
          modelId: modelId,
          message: message
        });
        return response.text || response.content || '';
      }
    } catch (error) {
      console.error(`Failed to send message to ${modelId}:`, error);
    }
    return '';
  }

  // Event system for UI updates
  on(event: string, callback: Function): void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, []);
    }
    this.eventListeners.get(event)!.push(callback);
  }

  private emit(event: string, data: any): void {
    const listeners = this.eventListeners.get(event) || [];
    listeners.forEach(callback => callback(data));
  }

  private notifyUserAdded(user: UniversalUser): void {
    this.emit('user:added', user);
  }

  private notifyUserUpdated(user: UniversalUser): void {
    this.emit('user:updated', user);
  }

  private notifyUserRemoved(user: UniversalUser): void {
    this.emit('user:removed', user);
  }

  /**
   * Generate HTML for connected users display
   */
  generateConnectedUsersHTML(): string {
    const users = this.getAllUsers();
    
    return users.map(user => {
      const clickableClass = user.isClickable ? 'clickable' : '';
      const typeClass = user.type;
      
      return `
        <div class="user-badge ${typeClass} ${clickableClass}" data-user-id="${user.id}">
          <span class="user-avatar">${user.avatar}</span>
          <span class="user-name">${user.name}</span>
          <span class="user-status ${user.status}"></span>
          ${user.currentTask ? `<span class="user-task">${user.currentTask}</span>` : ''}
        </div>
      `;
    }).join('');
  }
}

// Global singleton instance
export const universalUserSystem = new UniversalUserSystem();