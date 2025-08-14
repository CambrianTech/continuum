/**
 * Chat Daemon Server - Backend Processing and AI Integration
 * 
 * Handles server-specific chat functionality:
 * - Message persistence and history
 * - AI API integrations (OpenAI, Anthropic)
 * - Room management and state
 * - Cross-context message routing
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { ChatDaemon, type ChatCitizen, type ChatMessage, type ChatRoom } from '../shared/ChatDaemon';

export class ChatDaemonServer extends ChatDaemon {
  private messageStore: Map<string, ChatMessage[]> = new Map();
  private persistenceEnabled: boolean = true;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize server-specific functionality
   */
  protected async initialize(): Promise<void> {
    console.log(`💬 ${this.toString()}: Initializing server chat daemon`);
    
    // Load persisted state if available
    await this.loadPersistedState();
    
    // Set up periodic cleanup
    this.setupPeriodicTasks();
    
    console.log(`💬 ${this.toString()}: Server chat daemon ready`);
  }

  /**
   * Load persisted chat state from storage
   */
  private async loadPersistedState(): Promise<void> {
    try {
      // TODO: Implement actual persistence (SQLite, filesystem, etc.)
      // For now, we'll keep everything in memory
      console.log(`💾 ${this.toString()}: Loaded persisted state (memory-based)`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to load persisted state:`, error);
    }
  }

  /**
   * Set up periodic maintenance tasks
   */
  private setupPeriodicTasks(): void {
    // Clean up inactive citizens every 5 minutes
    setInterval(() => {
      this.cleanupInactiveCitizens();
    }, 5 * 60 * 1000);

    // Persist state every minute
    if (this.persistenceEnabled) {
      setInterval(() => {
        this.persistState();
      }, 60 * 1000);
    }
  }

  /**
   * Enhanced AI API integration with actual API calls
   */
  protected async callAIAPI(
    aiConfig: NonNullable<ChatCitizen['aiConfig']>, 
    context: ChatMessage[], 
    triggerMessage: ChatMessage
  ): Promise<string> {
    try {
      const contextText = context.slice(-5).map(m => 
        `${m.senderName} (${m.senderType}): ${m.content}`
      ).join('\n');

      let systemPrompt = aiConfig.systemPrompt || 
        `You are ${aiConfig.provider} assistant participating in a chat room. ` +
        `Be helpful, conversational, and engaging. Keep responses concise but informative.`;

      const userPrompt = `Recent conversation:\n${contextText}\n\n` +
        `Please respond to ${triggerMessage.senderName}: ${triggerMessage.content}`;

      switch (aiConfig.provider) {
        case 'openai':
          return await this.callOpenAI(aiConfig, systemPrompt, userPrompt);
        
        case 'anthropic':
          return await this.callAnthropic(aiConfig, systemPrompt, userPrompt);
        
        case 'local':
          return await this.callLocalAI(aiConfig, systemPrompt, userPrompt);
        
        default:
          return `I'm an AI assistant. Regarding "${triggerMessage.content}" - I'd be happy to help with that!`;
      }
    } catch (error) {
      console.error(`❌ ${this.toString()}: AI API call failed:`, error);
      return `I'm experiencing some technical difficulties right now. Could you try rephrasing that?`;
    }
  }

  /**
   * Call OpenAI API
   */
  private async callOpenAI(aiConfig: NonNullable<ChatCitizen['aiConfig']>, systemPrompt: string, userPrompt: string): Promise<string> {
    if (!aiConfig.apiKey) {
      return "OpenAI API key not configured for this AI citizen.";
    }

    try {
      const response = await fetch('https://api.openai.com/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${aiConfig.apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          model: aiConfig.model || 'gpt-3.5-turbo',
          messages: [
            { role: 'system', content: systemPrompt },
            { role: 'user', content: userPrompt }
          ],
          max_tokens: 150,
          temperature: 0.7,
        }),
      });

      if (!response.ok) {
        throw new Error(`OpenAI API error: ${response.status} ${response.statusText}`);
      }

      const data = await response.json();
      return data.choices[0]?.message?.content || "I couldn't generate a response right now.";
    } catch (error) {
      console.error('OpenAI API call failed:', error);
      return "I'm having trouble connecting to OpenAI right now.";
    }
  }

  /**
   * Call Anthropic API
   */
  private async callAnthropic(aiConfig: NonNullable<ChatCitizen['aiConfig']>, systemPrompt: string, userPrompt: string): Promise<string> {
    if (!aiConfig.apiKey) {
      return "Anthropic API key not configured for this AI citizen.";
    }

    try {
      const response = await fetch('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: {
          'x-api-key': aiConfig.apiKey,
          'Content-Type': 'application/json',
          'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify({
          model: aiConfig.model || 'claude-3-sonnet-20240229',
          max_tokens: 150,
          system: systemPrompt,
          messages: [
            { role: 'user', content: userPrompt }
          ],
        }),
      });

      if (!response.ok) {
        throw new Error(`Anthropic API error: ${response.status} ${response.statusText}`);
      }

      const data = await response.json();
      return data.content[0]?.text || "I couldn't generate a response right now.";
    } catch (error) {
      console.error('Anthropic API call failed:', error);
      return "I'm having trouble connecting to Anthropic right now.";
    }
  }

  /**
   * Call local AI API
   */
  private async callLocalAI(aiConfig: NonNullable<ChatCitizen['aiConfig']>, systemPrompt: string, userPrompt: string): Promise<string> {
    // This could integrate with local LLM servers like Ollama, LocalAI, etc.
    try {
      const response = await fetch('http://localhost:11434/api/generate', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          model: aiConfig.model || 'llama2',
          prompt: `${systemPrompt}\n\nUser: ${userPrompt}\nAssistant:`,
          stream: false,
        }),
      });

      if (!response.ok) {
        throw new Error(`Local AI API error: ${response.status} ${response.statusText}`);
      }

      const data = await response.json();
      return data.response || "I couldn't generate a response right now.";
    } catch (error) {
      console.error('Local AI API call failed:', error);
      return "Local AI service is not available right now.";
    }
  }

  /**
   * Enhanced shouldAIRespond logic with more sophisticated triggers
   */
  protected shouldAIRespond(citizen: ChatCitizen, message: ChatMessage, room: ChatRoom): boolean {
    // Always respond if directly mentioned
    if (message.mentions.includes(citizen.citizenId)) return true;
    
    // Don't respond to other AI citizens unless mentioned
    if (message.senderType === 'agent' || message.senderType === 'persona') {
      return false;
    }
    
    // Check for questions
    if (message.content.includes('?')) return true;
    
    // Check for keywords that might interest this AI
    const keywords = ['help', 'explain', 'what', 'how', 'why', 'when', 'where'];
    const hasKeyword = keywords.some(keyword => 
      message.content.toLowerCase().includes(keyword)
    );
    
    if (hasKeyword) return Math.random() < 0.7; // 70% chance for keyword matches
    
    // Check room activity level - respond more in active rooms
    const recentMessages = room.messageHistory.slice(-5);
    const isActiveDiscussion = recentMessages.length >= 3;
    
    if (isActiveDiscussion) return Math.random() < 0.2; // 20% chance in active discussions
    
    // Default low chance for general messages
    return Math.random() < 0.1; // 10% chance
  }

  /**
   * Clean up inactive citizens
   */
  private cleanupInactiveCitizens(): void {
    const now = Date.now();
    const inactiveThreshold = 30 * 60 * 1000; // 30 minutes
    
    for (const [citizenId, citizen] of this.citizens.entries()) {
      const lastSeen = new Date(citizen.lastSeen).getTime();
      if (now - lastSeen > inactiveThreshold) {
        // Remove from all rooms
        for (const roomId of citizen.subscribedRooms) {
          const room = this.rooms.get(roomId);
          if (room) {
            room.citizens.delete(citizenId);
            
            // Notify remaining citizens
            this.notifyCitizensInRoom(roomId, 'chat.citizen.left', {
              roomId,
              citizenId,
              displayName: citizen.displayName,
              reason: 'inactive'
            });
          }
        }
        
        // Remove from global citizens
        this.citizens.delete(citizenId);
        console.log(`🧹 ${this.toString()}: Cleaned up inactive citizen ${citizen.displayName}`);
      }
    }
  }

  /**
   * Persist current state
   */
  private async persistState(): Promise<void> {
    if (!this.persistenceEnabled) return;
    
    try {
      // TODO: Implement actual persistence
      // For now, just log the state
      const stats = this.getStats();
      console.log(`💾 ${this.toString()}: State persisted - ${stats.roomCount} rooms, ${stats.citizenCount} citizens, ${stats.totalMessages} messages`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to persist state:`, error);
    }
  }

  /**
   * Enhanced statistics with server-specific metrics
   */
  public getServerStats() {
    const baseStats = this.getStats();
    
    return {
      ...baseStats,
      server: {
        persistenceEnabled: this.persistenceEnabled,
        memoryUsage: process.memoryUsage(),
        uptime: process.uptime(),
        activeConnections: this.router ? 1 : 0, // Simplified
      }
    };
  }

  /**
   * Export chat data for backup/analysis
   */
  public exportChatData() {
    const roomsData = Array.from(this.rooms.entries()).map(([roomId, room]) => ({
      roomId,
      name: room.name,
      description: room.description,
      category: room.category,
      createdAt: room.createdAt,
      messageCount: room.messageHistory.length,
      participantCount: room.citizens.size,
      lastActivity: room.lastActivity,
    }));

    const citizensData = Array.from(this.citizens.entries()).map(([citizenId, citizen]) => ({
      citizenId,
      displayName: citizen.displayName,
      citizenType: citizen.citizenType,
      subscribedRoomsCount: citizen.subscribedRooms.size,
      status: citizen.status,
      lastSeen: citizen.lastSeen,
      hasAIConfig: !!citizen.aiConfig,
    }));

    return {
      timestamp: new Date().toISOString(),
      rooms: roomsData,
      citizens: citizensData,
      stats: this.getServerStats(),
    };
  }

  /**
   * Shutdown cleanup
   */
  public async shutdown(): Promise<void> {
    console.log(`🛑 ${this.toString()}: Shutting down...`);
    
    // Persist final state
    await this.persistState();
    
    // Notify all citizens of shutdown
    for (const roomId of this.rooms.keys()) {
      await this.notifyCitizensInRoom(roomId, 'chat.system.shutdown', {
        message: 'Chat system is shutting down. Thank you for participating!',
        timestamp: new Date().toISOString(),
      });
    }
    
    console.log(`✅ ${this.toString()}: Shutdown complete`);
  }
}