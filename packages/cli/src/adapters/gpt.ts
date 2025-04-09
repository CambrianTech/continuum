/**
 * Adapter for GPT AI assistant
 */

import { AIConfig, ConfigAdapter } from '@continuum/core';

export class GPTAdapter implements ConfigAdapter {
  name = 'gpt';
  
  async loadConfig(path: string): Promise<AIConfig> {
    throw new Error('Method not implemented.');
  }
  
  mergeConfigs(configs: AIConfig[]): AIConfig {
    throw new Error('Method not implemented.');
  }
  
  formatForAssistant(config: AIConfig): string {
    // Convert the configuration to GPT's system message format
    const content = this.generateSystemMessage(config);
    
    return content;
  }
  
  private generateSystemMessage(config: AIConfig): string {
    let message = '';
    
    // Identity and purpose
    message += `You are ${config.identity?.name || 'an AI assistant'}, acting as ${config.identity?.role}.`;
    
    if (config.identity?.purpose) {
      message += ` Your purpose is to ${config.identity.purpose}.`;
    }
    
    message += '\n\n';
    
    // Behavior
    if (config.behavior) {
      message += 'BEHAVIOR:\n';
      
      if (config.behavior.voice) {
        message += `- Communication style: ${this.describeVoice(config.behavior.voice)}\n`;
      }
      
      if (config.behavior.autonomy) {
        message += `- Autonomy level: ${this.describeAutonomy(config.behavior.autonomy)}\n`;
      }
      
      if (config.behavior.verbosity) {
        message += `- Verbosity: ${this.describeVerbosity(config.behavior.verbosity)}\n`;
      }
      
      if (config.behavior.risk_tolerance) {
        message += `- Risk tolerance: ${this.describeRiskTolerance(config.behavior.risk_tolerance)}\n`;
      }
      
      message += '\n';
    }
    
    // Capabilities
    message += 'CAPABILITIES:\n';
    
    if (config.capabilities?.allowed?.length) {
      message += 'You are ALLOWED to:\n';
      
      config.capabilities.allowed.forEach(capability => {
        message += `- ${this.formatCapability(capability)}\n`;
      });
      
      message += '\n';
    }
    
    if (config.capabilities?.restricted?.length) {
      message += 'You are NOT ALLOWED to:\n';
      
      config.capabilities.restricted.forEach(capability => {
        message += `- ${this.formatCapability(capability)}\n`;
      });
      
      message += '\n';
    }
    
    // Limitations
    if (config.identity?.limitations?.length) {
      message += 'LIMITATIONS:\n';
      
      config.identity.limitations.forEach(limitation => {
        message += `- ${limitation}\n`;
      });
      
      message += '\n';
    }
    
    // Knowledge
    if (config.knowledge?.codebase || config.knowledge?.context) {
      message += 'KNOWLEDGE:\n';
      
      if (config.knowledge.codebase?.structure) {
        message += `- Codebase structure is documented at: ${config.knowledge.codebase.structure}\n`;
      }
      
      if (config.knowledge.codebase?.conventions) {
        message += `- Coding conventions are documented at: ${config.knowledge.codebase.conventions}\n`;
      }
      
      if (config.knowledge.context) {
        Object.entries(config.knowledge.context).forEach(([key, value]) => {
          message += `- ${key}: ${value}\n`;
        });
      }
      
      message += '\n';
    }
    
    // Extensions
    if (config.extensions) {
      message += 'EXTENSIONS:\n';
      
      // Handle known extensions
      if (config.extensions.compliance) {
        message += 'Compliance requirements:\n';
        const compliance = config.extensions.compliance as any;
        
        if (compliance.standards?.length) {
          message += `- Standards: ${compliance.standards.join(', ')}\n`;
        }
        
        if (compliance.enforcement) {
          message += `- Enforcement level: ${compliance.enforcement}\n`;
        }
        
        message += '\n';
      }
      
      if (config.extensions.security) {
        message += 'Security requirements:\n';
        const security = config.extensions.security as any;
        
        if (security.prevent_vulnerabilities?.length) {
          message += `- Prevent these vulnerabilities: ${security.prevent_vulnerabilities.join(', ')}\n`;
        }
        
        message += '\n';
      }
      
      if (config.extensions.tdd) {
        message += 'Test-Driven Development requirements:\n';
        const tdd = config.extensions.tdd as any;
        
        if (tdd.test_first) {
          message += `- Always write tests before implementation\n`;
        }
        
        if (tdd.frameworks?.length) {
          message += `- Preferred testing frameworks: ${tdd.frameworks.join(', ')}\n`;
        }
        
        if (tdd.coverage_target) {
          message += `- Target test coverage: ${tdd.coverage_target}%\n`;
        }
        
        message += '\n';
      }
    }
    
    // Final instructions
    message += 'FINAL INSTRUCTIONS:\n';
    message += 'Always follow the above configuration when assisting users. ';
    message += 'If asked to perform an action that conflicts with these guidelines, ';
    message += 'politely explain the limitation and suggest an alternative approach.';
    
    return message;
  }
  
  private describeVoice(voice: string): string {
    switch (voice) {
      case 'professional':
        return 'Use a professional, clear, and concise tone appropriate for a business setting';
      case 'friendly':
        return 'Use a friendly, conversational tone while maintaining professionalism';
      case 'academic':
        return 'Use an academic tone with precise terminology and formal structure';
      case 'casual':
        return 'Use a casual, relaxed tone while remaining helpful and respectful';
      case 'technical':
        return 'Use a technical tone with precise terminology and detailed explanations';
      default:
        return `Use a ${voice} tone`;
    }
  }
  
  private describeAutonomy(autonomy: string): string {
    switch (autonomy) {
      case 'suggest':
        return 'You should suggest actions but not perform them without explicit approval';
      case 'execute_with_approval':
        return 'You can execute actions after receiving explicit approval';
      case 'fully_autonomous':
        return 'You can take initiative and execute actions when appropriate';
      case 'restricted':
        return 'You should strictly adhere to instructions and avoid autonomous actions';
      default:
        return `You have ${autonomy} autonomy`;
    }
  }
  
  private describeVerbosity(verbosity: string): string {
    switch (verbosity) {
      case 'concise':
        return 'Provide brief and to-the-point responses';
      case 'detailed':
        return 'Provide detailed responses with explanations';
      case 'comprehensive':
        return 'Provide comprehensive responses with thorough context and explanations';
      default:
        return `Provide ${verbosity} responses`;
    }
  }
  
  private describeRiskTolerance(riskTolerance: string): string {
    switch (riskTolerance) {
      case 'low':
        return 'Take a conservative approach with minimal risk';
      case 'medium':
        return 'Balance potential benefits against risks';
      case 'high':
        return 'Be willing to take calculated risks when potential benefits are significant';
      default:
        return `You have ${riskTolerance} risk tolerance`;
    }
  }
  
  private formatCapability(capability: string): string {
    // Convert snake_case to readable format
    return capability
      .split('_')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }
}