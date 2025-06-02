/**
 * Base Agent class - all AI agents inherit from this
 */

class BaseAgent {
  constructor(name, config = {}) {
    this.name = name;
    this.config = config;
    this.tools = {};
    this.loadTools();
  }

  loadTools() {
    // Each agent loads its own tools
    // Override in subclasses
  }

  async execute(task) {
    // 1. Get AI response
    const response = await this.getAIResponse(task);
    
    // 2. Execute any tools requested in response
    const toolResults = await this.processToolCommands(response);
    
    // 3. Return combined result
    return {
      response,
      toolResults,
      agent: this.name
    };
  }

  async getAIResponse(task) {
    throw new Error('Subclasses must implement getAIResponse');
  }

  async processToolCommands(response) {
    const results = [];
    
    // Each tool type scans for its own commands
    for (const [toolName, tool] of Object.entries(this.tools)) {
      const toolResults = await tool.processResponse(response);
      results.push(...toolResults);
    }
    
    return results;
  }
}

module.exports = BaseAgent;