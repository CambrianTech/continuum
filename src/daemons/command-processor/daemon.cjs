/**
 * CommandProcessor Daemon - Simple JavaScript implementation
 * This bridges the gap until TypeScript compilation issues are resolved
 */

class CommandProcessorDaemon {
  constructor() {
    this.name = 'command-processor';
    this.commands = new Map();
    this.running = false;
  }

  async start() {
    console.log('🚀 CommandProcessor Daemon starting...');
    this.running = true;
    
    // Register command routing intercept
    this.setupCommandRouting();
    
    console.log('✅ CommandProcessor Daemon ready');
    return true;
  }

  setupCommandRouting() {
    console.log('🌉 Setting up command routing bridge...');
    
    // This is where we intercept commands going to PlannerAI
    // and route them to proper command modules instead
    
    process.on('message', (message) => {
      if (message.type === 'route_command') {
        this.routeCommand(message.task, message.source);
      }
    });
  }

  async routeCommand(task, source = 'unknown') {
    console.log(`🌉 DAEMON: Routing command: "${task.substring(0, 50)}..."`);
    
    // Parse command from task string
    const command = this.parseCommand(task);
    
    if (command) {
      console.log(`🎯 DAEMON: Found command ${command.name}, routing to proper handler`);
      return this.executeCommand(command);
    } else {
      console.log(`🌉 DAEMON: No command detected, falling back to AI routing`);
      return { role: 'PlannerAI', task, fallback: true };
    }
  }

  parseCommand(task) {
    const taskTrimmed = task.trim();
    
    // Handle direct command format: "migration {...}"
    const directMatch = taskTrimmed.match(/^(\w+)\s*(.*)/);
    if (directMatch) {
      const commandName = directMatch[1].toUpperCase();
      
      // Known commands that should route to modules, not PlannerAI
      const knownCommands = [
        'MIGRATION', 'SCREENSHOT', 'HELP', 'AGENTS', 'WORKSPACE',
        'BROWSER_JS', 'BROWSERJS', 'EXEC', 'RESTART', 'STATUS'
      ];
      
      if (knownCommands.includes(commandName)) {
        return {
          name: commandName,
          params: directMatch[2] || '{}',
          source: 'daemon-routed'
        };
      }
    }
    
    return null;
  }

  async executeCommand(command) {
    console.log(`⚡ DAEMON: Executing ${command.name} via proper command system`);
    
    try {
      // For now, return successful routing indication
      // TODO: Actually call the command modules
      return {
        role: 'BusCommand',
        command: command.name,
        params: command.params,
        source: 'command-processor-daemon',
        routed_by: 'daemon'
      };
    } catch (error) {
      console.error(`❌ DAEMON: Command execution failed:`, error);
      return {
        role: 'CommandError',
        error: error.message,
        command: command.name
      };
    }
  }

  async stop() {
    console.log('🛑 CommandProcessor Daemon stopping...');
    this.running = false;
  }

  isRunning() {
    return this.running;
  }
}

// Start daemon if run directly
if (require.main === module) {
  const daemon = new CommandProcessorDaemon();
  daemon.start().then(() => {
    console.log('📡 CommandProcessor Daemon is running...');
    
    // Keep alive
    setInterval(() => {
      if (daemon.isRunning()) {
        console.log('💓 CommandProcessor Daemon heartbeat');
      }
    }, 30000);
  });
}

module.exports = CommandProcessorDaemon;