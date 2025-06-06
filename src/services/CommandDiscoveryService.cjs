/**
 * Command Discovery Service
 * Provides command schema and connection information to agents and users
 */

class CommandDiscoveryService {
  constructor(continuum) {
    this.continuum = continuum;
  }

  getConnectionInfo() {
    return {
      api_base: `http://localhost:${this.continuum.port}`,
      websocket_url: `ws://localhost:${this.continuum.port}`,
      commands_schema: `http://localhost:${this.continuum.port}/api/commands`,
      timestamp: new Date().toISOString()
    };
  }

  getWelcomeMessage() {
    const connectionInfo = this.getConnectionInfo();
    return `🔧 **For Agents & Developers:** Get available commands and API schema at ${connectionInfo.commands_schema}`;
  }

  async getCommandSchema() {
    try {
      const fs = require('fs');
      const path = require('path');
      
      const schemaPath = path.join(__dirname, '..', '..', 'schema', 'commands.schema.json');
      
      if (fs.existsSync(schemaPath)) {
        const commandSchema = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
        
        return {
          ...commandSchema,
          connection_info: this.getConnectionInfo(),
          available_commands: this.getAvailableCommands()
        };
      }
      
      return null;
    } catch (error) {
      console.error('Failed to load command schema:', error);
      return null;
    }
  }

  getAvailableCommands() {
    return {
      ui_interaction: [
        {
          name: "toggle_agent_drawer",
          description: "Opens/closes the cyberpunk agent command center",
          usage: "Ask to 'open agent drawer' or 'show agent command center'",
          examples: ["open the agent drawer", "show cyberpunk agent center"]
        },
        {
          name: "take_screenshot",
          description: "Capture a screenshot of the current interface",
          usage: "Ask to 'take a screenshot' or 'capture current interface'",
          examples: ["take a screenshot", "capture the current interface", "show me what's on screen"]
        },
        {
          name: "select_agent", 
          description: "Select a specific AI agent",
          usage: "Ask to 'select agent [name]'",
          parameters: [{ name: "agent_id", type: "string", required: true }],
          examples: ["select agent PlannerAI", "choose CodeAI"]
        },
        {
          name: "switch_room",
          description: "Switch between Chat and Academy rooms", 
          usage: "Ask to 'switch to [room]'",
          parameters: [{ name: "room", type: "string", required: true }],
          examples: ["switch to academy room", "go to chat room"]
        },
        {
          name: "send_to_academy",
          description: "Send an agent to Academy training",
          usage: "Ask to 'send [agent] to academy'",
          parameters: [{ name: "agent_id", type: "string", required: true }],
          examples: ["send PlannerAI to academy", "train CodeAI in the academy"]
        }
      ],
      system_operations: [
        {
          name: "get_status",
          endpoint: "/api/status",
          method: "GET",
          description: "Get system status and metrics",
          examples: [`curl http://localhost:${this.continuum.port}/api/status`]
        },
        {
          name: "list_agents",
          endpoint: "/api/agents", 
          method: "GET",
          description: "Get list of available agents",
          examples: [`curl http://localhost:${this.continuum.port}/api/agents`]
        },
        {
          name: "get_academy_status",
          endpoint: "/api/academy/status",
          method: "GET", 
          description: "Get Academy training status",
          examples: [`curl http://localhost:${this.continuum.port}/api/academy/status`]
        }
      ],
      shell_execution: [
        {
          name: "exec",
          format: "[CMD:EXEC] command",
          description: "Execute shell commands",
          examples: ["[CMD:EXEC] date", "[CMD:EXEC] curl ipinfo.io"]
        },
        {
          name: "webfetch",
          format: "[CMD:WEBFETCH] url", 
          description: "Fetch content from websites",
          examples: ["[CMD:WEBFETCH] https://wttr.in/london"]
        },
        {
          name: "file_read",
          format: "[CMD:FILE_READ] path",
          description: "Read file contents",
          examples: ["[CMD:FILE_READ] package.json"]
        }
      ]
    };
  }
}

module.exports = CommandDiscoveryService;