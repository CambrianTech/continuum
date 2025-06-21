/**
 * Share Command - Centralized sharing functionality
 * Core continuum daemon handles all sharing destinations
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class ShareCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'share',
      category: 'Core',
      icon: '🔗',
      description: 'Universal sharing across continuum ecosystem',
      parameters: {
        target: {
          type: 'object',
          required: true,
          description: 'Target user, agent, or service to share with'
        },
        content: {
          type: 'object',
          required: false,
          description: 'Content to share (file path, data, etc.)'
        },
        method: {
          type: 'string',
          required: false,
          description: 'Sharing method (slack, email, console, etc.)'
        },
        input: {
          type: 'object',
          required: false,
          description: 'Input from previous command in pipeline'
        }
      },
      examples: [
        'share --target {"name": "joel"}',
        'share --target {"name": "joel"} --method slack',
        'share --content {"file": "screenshot.png"}'
      ]
    };
  }
  
  static async execute(params, continuum) {
    const options = this.parseParams(params);
    const target = options.target || 'user';
    const content = options.content;
    
    if (!content) {
      return { success: false, error: 'No content specified' };
    }
    
    return await this.share(content, target, continuum);
  }
  
  static async share(content, target, continuum, from = null) {
    // Universal sharing fabric - intelligent routing across ecosystem
    switch (target) {
      case 'user':
        return this.shareWithUser(content, from);
      case 'agent':
        return this.shareWithAgent(content, continuum, from);
      case 'ai':
        return this.shareWithAI(content, continuum, from);
      case 'ui':
      case 'browser':
      case 'console':
        return this.shareWithUI(content, continuum, from);
      case 'return':
        return { success: true, content, from };
      default:
        // Smart routing - ecosystem diversity
        if (continuum && continuum.webSocketServer) {
          return this.shareWithUI(content, continuum, from);
        }
        return this.shareWithUser(content, from);
    }
  }
  
  static shareWithUser(content, from) {
    try {
      // For Mac: Open PNG files directly - simple and practical
      const { exec } = require('child_process');
      exec(`open "${content}"`, (error) => {
        if (error) {
          console.warn(`⚠️ Could not share with user: ${error.message}`);
        } else {
          console.log(`👤 Shared PNG with user: ${content} ${from ? `(from: ${from})` : ''}`);
          // Future: This will integrate into chat UI seamlessly
        }
      });
      return { 
        success: true, 
        target: 'user', 
        content, 
        from,
        accessible_path: content // Agents can access directly
      };
    } catch (error) {
      return { success: false, error: error.message };
    }
  }
  
  static shareWithAgent(content, continuum, from) {
    // Share with other continuum agents
    if (continuum && continuum.agentRegistry) {
      continuum.agentRegistry.broadcast({
        type: 'agent_share',
        content: content,
        from: from,
        timestamp: new Date().toISOString()
      });
      console.log(`🤖 Shared with continuum agents: ${content}`);
    }
    return { success: true, target: 'agent', content, from };
  }
  
  static shareWithAI(content, continuum, from) {
    // Share with AI systems (Claude, GPT, etc.)
    if (continuum && continuum.aiConnections) {
      continuum.aiConnections.broadcast({
        type: 'ai_share',
        content: content,
        from: from,
        timestamp: new Date().toISOString()
      });
      console.log(`🧠 Shared with AI systems: ${content}`);
    }
    return { success: true, target: 'ai', content, from };
  }

  static shareWithUI(content, continuum, from) {
    if (continuum && continuum.webSocketServer) {
      continuum.webSocketServer.broadcast({
        type: 'shared_content',
        content: content,
        from: from,
        timestamp: new Date().toISOString()
      });
      console.log(`🔗 Shared with UI client: ${content} ${from ? `(from: ${from})` : ''}`);
    }
    return { success: true, target: 'ui', content, from };
  }
  
  // Inherited from BaseCommand - elegant and consistent
}

module.exports = ShareCommand;