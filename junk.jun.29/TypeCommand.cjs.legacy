const BaseCommand = require('../../core/BaseCommand.cjs');
const { exec } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

/**
 * Type Command - Simulates keyboard typing with natural timing
 * Handles text input with realistic human-like typing patterns
 */
class TypeCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'TYPE',
      category: 'keyboard control',
      icon: '⌨️',
      description: 'Type text with natural human-like timing and rhythm',
      parameters: {
        text: {
          type: 'string',
          required: true,
          description: 'Text to type'
        },
        speed: {
          type: 'string',
          required: false,
          default: 'natural',
          description: 'Typing speed: fast, natural, slow, instant'
        },
        pause: {
          type: 'number',
          required: false,
          description: 'Pause in milliseconds before typing'
        }
      },
      examples: [
        '{"text": "Hello World"}',
        '{"text": "Slow typing", "speed": "slow"}',
        '{"text": "With delay", "pause": 500}',
        '{"text": "Fast typing", "speed": "fast"}'
      ],
      usage: 'Simulates natural keyboard typing with configurable speed and timing'
    };
  }


  static async execute(params, context) {
    try {
      const { text, speed = 'natural', pause } = this.parseParams(params);

      if (!text || typeof text !== 'string') {
        return this.createErrorResult('Text parameter is required');
      }

      // Add pause if specified
      if (pause && pause > 0) {
        console.log(`⏱️ Pausing ${pause}ms before typing...`);
        await this.wait(pause);
      }

      console.log(`⌨️ Typing text with ${speed} speed: "${text.substring(0, 50)}${text.length > 50 ? '...' : ''}"`);

      // Get typing command based on speed
      const typeCommand = await this.createTypingCommand(text, speed);

      // Execute the typing
      const { stdout, stderr } = await execAsync(typeCommand);
      
      if (stderr) {
        console.log(`⚠️ Type command warning: ${stderr}`);
      }

      // Send visual feedback to browser if available
      if (context && context.webSocketServer) {
        context.webSocketServer.broadcast({
          type: 'typing_feedback',
          text: text.substring(0, 100), // Limit feedback text
          speed,
          timestamp: new Date().toISOString()
        });
      }

      console.log(`✅ Text typed successfully (${text.length} characters)`);

      return this.createSuccessResult({
        typed: true,
        text,
        length: text.length,
        speed,
        timestamp: new Date().toISOString()
      });

    } catch (error) {
      console.error('❌ Type command failed:', error);
      return this.createErrorResult(`Failed to type text: ${error.message}`);
    }
  }

  static async createTypingCommand(text, speed) {
    // Escape special characters for shell
    const escapedText = text.replace(/'/g, "'\"'\"'");

    switch (speed) {
      case 'instant':
        return `cliclick t:'${escapedText}'`;
      
      case 'fast':
        // Fast typing with minimal delays
        return this.createCharacterByCharacterCommand(escapedText, 20);
      
      case 'natural':
      default:
        // Natural human-like typing with variable delays
        return this.createNaturalTypingCommand(escapedText);
      
      case 'slow':
        // Deliberate slow typing
        return this.createCharacterByCharacterCommand(escapedText, 150);
    }
  }

  static createCharacterByCharacterCommand(text, baseDelay) {
    const commands = [];
    
    for (let i = 0; i < text.length; i++) {
      const char = text[i];
      const delay = baseDelay + (Math.random() * 20 - 10); // Add some variation
      
      if (char === ' ') {
        commands.push(`cliclick key:space`);
      } else if (char === '\n') {
        commands.push(`cliclick key:return`);
      } else if (char === '\t') {
        commands.push(`cliclick key:tab`);
      } else {
        commands.push(`cliclick t:'${char.replace(/'/g, "'\"'\"'")}'`);
      }
      
      if (i < text.length - 1) {
        commands.push(`sleep ${delay / 1000}`);
      }
    }
    
    return commands.join(' && ');
  }

  static createNaturalTypingCommand(text) {
    const commands = [];
    let i = 0;
    
    while (i < text.length) {
      const char = text[i];
      let charsToType = 1;
      
      // Occasionally type multiple characters at once (common typing pattern)
      if (Math.random() < 0.3 && i + 2 < text.length) {
        charsToType = Math.min(3, text.length - i);
      }
      
      const chunk = text.substring(i, i + charsToType);
      
      if (chunk === ' ') {
        commands.push(`cliclick key:space`);
      } else if (chunk === '\n') {
        commands.push(`cliclick key:return`);
      } else if (chunk === '\t') {
        commands.push(`cliclick key:tab`);
      } else {
        commands.push(`cliclick t:'${chunk.replace(/'/g, "'\"'\"'")}'`);
      }
      
      i += charsToType;
      
      if (i < text.length) {
        // Variable delay based on character type and context
        let delay = 80 + (Math.random() * 60); // Base 80-140ms
        
        // Longer pauses after punctuation
        if (['.', '!', '?', ',', ';', ':'].includes(char)) {
          delay += 100 + (Math.random() * 100);
        }
        
        // Shorter delays for common letter combinations
        if (i > 0 && /[a-zA-Z]/.test(text[i-1]) && /[a-zA-Z]/.test(text[i])) {
          delay *= 0.8;
        }
        
        commands.push(`sleep ${delay / 1000}`);
      }
    }
    
    return commands.join(' && ');
  }

  static wait(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

}

module.exports = TypeCommand;