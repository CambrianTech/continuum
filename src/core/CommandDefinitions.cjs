/**
 * Centralized Command Definitions
 * Single source of truth for all API commands, descriptions, and examples
 */

class CommandDefinitions {
  static getCommandDefinitions() {
    return {
      // Core Commands
      SCREENSHOT: {
        category: 'Core',
        icon: '📸',
        description: 'Take screenshot of browser',
        params: '[selector] [coordinates] [resolution] [format] [quality]',
        examples: [
          '{}',
          '{"params": "selector .sidebar"}',
          '{"params": "100,200,800,600"}',
          '{"params": "1920x1080 format jpeg quality 0.8"}'
        ],
        usage: 'Captures current browser state. Use selector for specific elements, coordinates for regions, or leave empty for full page.'
      },
      
      BROWSER_JS: {
        category: 'Core',
        icon: '💻',
        description: 'Execute JavaScript in browser',
        params: '<javascript_code> [encoding]',
        examples: [
          '{"params": "console.log(\'test\')"}',
          '{"params": "Y29uc29sZS5sb2coJ3Rlc3QnKQ==", "encoding": "base64"}',
          '{"params": "document.title = \'New Title\'"}',
          '{"params": "document.querySelector(\'.btn\').click()"}'
        ],
        usage: 'Execute JavaScript in connected browsers. Use base64 encoding to avoid quote escaping issues.'
      },
      
      EXEC: {
        category: 'Core',
        icon: '⚡',
        description: 'Execute shell command',
        params: '<command_and_args>',
        examples: [
          '{"params": "ls -la"}',
          '{"params": "ps aux | grep node"}',
          '{"params": "git status"}',
          '{"params": "npm test"}'
        ],
        usage: 'Execute shell commands on the server. Use with caution - has full system access.'
      },
      
      FILE_READ: {
        category: 'Core',
        icon: '📖',
        description: 'Read file contents',
        params: '<file_path>',
        examples: [
          '{"params": "/path/to/file.txt"}',
          '{"params": "./package.json"}',
          '{"params": "~/config/settings.json"}'
        ],
        usage: 'Read contents of files on the server filesystem.'
      },
      
      FILE_WRITE: {
        category: 'Core',
        icon: '📝',
        description: 'Write file contents',
        params: '<file_path> <content>',
        examples: [
          '{"params": "/tmp/test.txt Hello World"}',
          '{"params": "./config.json {\\"key\\": \\"value\\"}"}'
        ],
        usage: 'Write content to files on the server filesystem.'
      },
      
      SAVE_FILE: {
        category: 'Core',
        icon: '💾',
        description: 'Save file from base64 data to filesystem',
        params: '{"filename": "name.ext", "directory": "path", "content": "base64_data", "mimeType": "type"}',
        examples: [
          '{"params": "{\\"filename\\": \\"test.txt\\", \\"directory\\": \\".continuum/screenshots\\", \\"content\\": \\"SGVsbG8gV29ybGQ=\\", \\"mimeType\\": \\"text/plain\\"}"}',
          '{"params": "{\\"filename\\": \\"image.png\\", \\"directory\\": \\".continuum/screenshots\\", \\"content\\": \\"base64_image_data\\", \\"mimeType\\": \\"image/png\\"}"}'
        ],
        usage: 'Save files to server filesystem from base64 blob data. Creates directories as needed. Perfect for screenshots, documents, and any binary data.'
      },
      
      WEBFETCH: {
        category: 'Core',
        icon: '🌐',
        description: 'Fetch web content',
        params: '<url> [options]',
        examples: [
          '{"params": "https://api.github.com/user"}',
          '{"params": "https://example.com headers={\\"Authorization\\": \\"Bearer token\\"}"}'
        ],
        usage: 'Fetch content from web URLs with optional headers and parameters.'
      },
      
      PYTHON: {
        category: 'Core',
        icon: '🐍',
        description: 'Execute Python code',
        params: '<python_code>',
        examples: [
          '{"params": "print(\\"Hello World\\")"}',
          '{"params": "import json; print(json.dumps({\\"test\\": True}))"}'
        ],
        usage: 'Execute Python code on the server. Use for data processing and calculations.'
      },
      
      // Automation Commands
      ACTIVATE_CURSOR: {
        category: 'Automation',
        icon: '🖱️',
        description: 'Show visual cursor',
        params: '[position]',
        examples: [
          '{}',
          '{"params": "100,200"}',
          '{"params": "center"}'
        ],
        usage: 'Display a visual cursor indicator on the screen for user guidance.'
      },
      
      DEACTIVATE_CURSOR: {
        category: 'Automation',
        icon: '🖱️',
        description: 'Hide visual cursor',
        params: '',
        examples: ['{}'],
        usage: 'Hide the visual cursor indicator.'
      },
      
      CLICK: {
        category: 'Automation',
        icon: '👆',
        description: 'Click coordinates or element',
        params: '<x,y> OR <selector>',
        examples: [
          '{"params": "100,200"}',
          '{"params": "button.submit"}',
          '{"params": "#login-btn"}',
          '{"params": ".sidebar .menu-item:first"}'
        ],
        usage: 'Click at specific coordinates or CSS selector. Use coordinates for precise positioning or selectors for elements.'
      },
      
      MOVE: {
        category: 'Automation',
        icon: '↗️',
        description: 'Move cursor to position',
        params: '<x,y> OR <selector>',
        examples: [
          '{"params": "300,400"}',
          '{"params": ".hover-target"}',
          '{"params": "center"}'
        ],
        usage: 'Move cursor to coordinates or element without clicking.'
      },
      
      DRAG: {
        category: 'Automation',
        icon: '🔄',
        description: 'Drag from point A to B',
        params: '<from_x,from_y,to_x,to_y> OR <from_selector,to_selector>',
        examples: [
          '{"params": "100,100,300,300"}',
          '{"params": ".draggable,.drop-zone"}'
        ],
        usage: 'Drag from one position to another. Supports coordinates or CSS selectors.'
      },
      
      SCROLL: {
        category: 'Automation',
        icon: '📜',
        description: 'Scroll page or element',
        params: '<direction> [amount] [selector]',
        examples: [
          '{"params": "down"}',
          '{"params": "up 500"}',
          '{"params": "right 200 .scrollable"}',
          '{"params": "to_bottom"}'
        ],
        usage: 'Scroll page or specific elements. Supports directions: up, down, left, right, to_top, to_bottom.'
      },
      
      TYPE: {
        category: 'Automation',
        icon: '⌨️',
        description: 'Type text',
        params: '<text> [speed]',
        examples: [
          '{"params": "Hello World"}',
          '{"params": "username@example.com fast"}',
          '{"params": "password123 slow"}'
        ],
        usage: 'Type text at current cursor position. Optional speed: fast, normal, slow.'
      },
      
      KEY: {
        category: 'Automation',
        icon: '🔑',
        description: 'Send keyboard events',
        params: '<key_combination>',
        examples: [
          '{"params": "Enter"}',
          '{"params": "Ctrl+C"}',
          '{"params": "Alt+Tab"}',
          '{"params": "Cmd+V"}'
        ],
        usage: 'Send keyboard shortcuts and special keys. Supports combinations with Ctrl, Alt, Cmd, Shift.'
      },
      
      // Browser Commands
      ACTIVATE_WEB_BROWSER: {
        category: 'Browser',
        icon: '🌐',
        description: 'Start browser session',
        params: '[url]',
        examples: [
          '{}',
          '{"params": "https://google.com"}',
          '{"params": "http://localhost:3000"}'
        ],
        usage: 'Initialize browser automation session, optionally navigating to URL.'
      },
      
      DEACTIVATE_WEB_BROWSER: {
        category: 'Browser',
        icon: '🌐',
        description: 'End browser session',
        params: '',
        examples: ['{}'],
        usage: 'Terminate browser automation session and cleanup resources.'
      },
      
      WEB_NAVIGATE: {
        category: 'Browser',
        icon: '🧭',
        description: 'Navigate to URL',
        params: '<url>',
        examples: [
          '{"params": "https://github.com"}',
          '{"params": "http://localhost:9000"}',
          '{"params": "https://api.example.com/docs"}'
        ],
        usage: 'Navigate browser to specific URL.'
      },
      
      WEB_RELOAD: {
        category: 'Browser',
        icon: '🔄',
        description: 'Reload current page',
        params: '[force]',
        examples: [
          '{}',
          '{"params": "force"}'
        ],
        usage: 'Reload current page, optionally forcing cache refresh.'
      },
      
      WEB_SESSION_STATUS: {
        category: 'Browser',
        icon: '📊',
        description: 'Get browser status',
        params: '',
        examples: ['{}'],
        usage: 'Get current browser session status and information.'
      }
    };
  }
  
  static getCommand(commandName) {
    const commands = this.getCommandDefinitions();
    return commands[commandName.toUpperCase()] || null;
  }
  
  static getAllCommands() {
    return Object.keys(this.getCommandDefinitions());
  }
  
  static getCommandsByCategory(category) {
    const commands = this.getCommandDefinitions();
    return Object.entries(commands)
      .filter(([name, def]) => def.category === category)
      .reduce((acc, [name, def]) => ({ ...acc, [name]: def }), {});
  }
  
  static getCategories() {
    const commands = this.getCommandDefinitions();
    return [...new Set(Object.values(commands).map(cmd => cmd.category))];
  }
  
  static generateManPage(commandName = null) {
    if (commandName) {
      // Single command manual
      const cmd = this.getCommand(commandName);
      if (!cmd) {
        return `❌ Command '${commandName}' not found. Use 'continuum --man' to list all commands.`;
      }
      
      return `
📚 ${cmd.icon} ${commandName.toUpperCase()} - ${cmd.description}

USAGE:
  {"command": "${commandName.toUpperCase()}", "params": "${cmd.params}"}

DESCRIPTION:
  ${cmd.usage}

EXAMPLES:
${cmd.examples.map(ex => `  curl -X POST http://localhost:9000/connect -d '{"command": "${commandName.toUpperCase()}", "params": ${ex.replace('{}', '""')}}'`).join('\n')}

PARAMETERS:
  ${cmd.params || 'No parameters required'}

For all commands: continuum --man
For specific command: continuum --man <COMMAND_NAME>
`;
    }
    
    // Full manual
    const categories = this.getCategories();
    let manual = `📚 CONTINUUM API REFERENCE - Complete Command Manual\n\n`;
    
    categories.forEach(category => {
      const commands = this.getCommandsByCategory(category);
      manual += `${this.getCategoryIcon(category)} ${category.toUpperCase()} COMMANDS:\n`;
      
      Object.entries(commands).forEach(([name, def]) => {
        manual += `  ${name.padEnd(25)} ${def.description}\n`;
      });
      manual += '\n';
    });
    
    manual += `📋 USAGE EXAMPLES:\n`;
    manual += `  # Get single command help:\n`;
    manual += `  continuum --man SCREENSHOT\n\n`;
    manual += `  # Execute command:\n`;
    manual += `  curl -X POST http://localhost:9000/connect -d '{"command": "SCREENSHOT"}'\n\n`;
    manual += `  # Use base64 for complex JavaScript:\n`;
    manual += `  echo 'console.log("test")' | base64\n`;
    manual += `  curl -X POST http://localhost:9000/connect -d '{"command": "BROWSER_JS", "params": "BASE64_HERE", "encoding": "base64"}'\n\n`;
    
    return manual;
  }
  
  static getCategoryIcon(category) {
    const icons = {
      'Core': '🔧',
      'Automation': '🎮',
      'Browser': '🌐',
      'Gaming': '🎯',
      'Visual': '👁️'
    };
    return icons[category] || '📁';
  }
}

module.exports = CommandDefinitions;