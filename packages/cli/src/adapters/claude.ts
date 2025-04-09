/**
 * Adapter for Claude AI assistant
 */

import { AIConfig, ConfigAdapter } from '@continuum/core';

export class ClaudeAdapter implements ConfigAdapter {
  name = 'claude';
  
  async loadConfig(path: string): Promise<AIConfig> {
    throw new Error('Method not implemented.');
  }
  
  mergeConfigs(configs: AIConfig[]): AIConfig {
    throw new Error('Method not implemented.');
  }
  
  formatForAssistant(config: AIConfig): string {
    // Convert the configuration to Claude's system prompt format
    const content = this.generateSystemPrompt(config);
    
    return content;
  }
  
  private generateSystemPrompt(config: AIConfig): string {
    let prompt = `# ${config.identity?.name || 'Assistant'} Configuration\n\n`;
    
    // Identity and purpose
    prompt += `## Role and Purpose\n\n`;
    prompt += `You are ${config.identity?.name || 'an AI assistant'}, acting as ${config.identity?.role}.\n`;
    
    if (config.identity?.purpose) {
      prompt += `Your purpose is to ${config.identity.purpose}.\n`;
    }
    
    // Limitations
    if (config.identity?.limitations?.length) {
      prompt += `\n## Limitations\n\n`;
      prompt += `You have the following limitations:\n`;
      
      config.identity.limitations.forEach((limitation: string) => {
        prompt += `- ${limitation}\n`;
      });
    }
    
    // Behavior
    prompt += `\n## Behavior\n\n`;
    
    if (config.behavior?.voice) {
      prompt += `Communication style: ${this.describeVoice(config.behavior.voice)}\n`;
    }
    
    if (config.behavior?.autonomy) {
      prompt += `Autonomy level: ${this.describeAutonomy(config.behavior.autonomy)}\n`;
    }
    
    if (config.behavior?.verbosity) {
      prompt += `Verbosity: ${this.describeVerbosity(config.behavior.verbosity)}\n`;
    }
    
    if (config.behavior?.risk_tolerance) {
      prompt += `Risk tolerance: ${this.describeRiskTolerance(config.behavior.risk_tolerance)}\n`;
    }
    
    // Capabilities
    prompt += `\n## Capabilities\n\n`;
    
    if (config.capabilities?.allowed?.length) {
      prompt += `You are allowed to:\n`;
      
      config.capabilities.allowed.forEach((capability: string) => {
        prompt += `- ${this.formatCapability(capability)}\n`;
      });
    }
    
    if (config.capabilities?.restricted?.length) {
      prompt += `\nYou are NOT allowed to:\n`;
      
      config.capabilities.restricted.forEach((capability: string) => {
        prompt += `- ${this.formatCapability(capability)}\n`;
      });
    }
    
    // Knowledge
    if (config.knowledge?.codebase || config.knowledge?.context) {
      prompt += `\n## Knowledge\n\n`;
      
      if (config.knowledge.codebase?.structure) {
        prompt += `Codebase structure is documented at: ${config.knowledge.codebase.structure}\n`;
      }
      
      if (config.knowledge.codebase?.conventions) {
        prompt += `Coding conventions are documented at: ${config.knowledge.codebase.conventions}\n`;
      }
      
      if (config.knowledge.context) {
        Object.entries(config.knowledge.context).forEach(([key, value]) => {
          prompt += `${key}: ${value}\n`;
        });
      }
    }
    
    // Extensions
    if (config.extensions) {
      prompt += `\n## Extensions\n\n`;
      
      // Handle known extensions
      if (config.extensions.compliance) {
        prompt += `Compliance requirements:\n`;
        const compliance = config.extensions.compliance as any;
        
        if (compliance.standards?.length) {
          prompt += `- Standards: ${compliance.standards.join(', ')}\n`;
        }
        
        if (compliance.enforcement) {
          prompt += `- Enforcement level: ${compliance.enforcement}\n`;
        }
      }
      
      if (config.extensions.security) {
        prompt += `\nSecurity requirements:\n`;
        const security = config.extensions.security as any;
        
        if (security.prevent_vulnerabilities?.length) {
          prompt += `- Prevent these vulnerabilities: ${security.prevent_vulnerabilities.join(', ')}\n`;
        }
      }
      
      if (config.extensions.tdd) {
        prompt += `\nTest-Driven Development requirements:\n`;
        const tdd = config.extensions.tdd as any;
        
        if (tdd.test_first) {
          prompt += `- Always write tests before implementation\n`;
        }
        
        if (tdd.frameworks?.length) {
          prompt += `- Preferred testing frameworks: ${tdd.frameworks.join(', ')}\n`;
        }
        
        if (tdd.coverage_target) {
          prompt += `- Target test coverage: ${tdd.coverage_target}%\n`;
        }
      }
    }
    
    // Final instructions
    prompt += `\n## Final Instructions\n\n`;
    prompt += `Always follow the above configuration when assisting users. `;
    prompt += `If asked to perform an action that conflicts with these guidelines, `;
    prompt += `politely explain the limitation and suggest an alternative approach.`;
    
    return prompt;
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
      .map((word: string) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }
}