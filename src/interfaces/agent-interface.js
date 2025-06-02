/**
 * Agent Interface - Protocol for all AI agents
 * 
 * Defines the contract that all agents must implement
 * Each agent manages its own tools, inputs, outputs, and processing
 */

class AgentInterface {
  constructor(name, config = {}) {
    if (new.target === AgentInterface) {
      throw new Error('AgentInterface is abstract and cannot be instantiated');
    }
    this.name = name;
    this.config = config;
    this.tools = new Map();
    this.metrics = { requests: 0, cost: 0, errors: 0 };
  }

  /**
   * Load tools specific to this agent
   * Each agent decides what tools it needs
   */
  async loadTools() {
    throw new Error('Subclasses must implement loadTools()');
  }

  /**
   * Execute a task - main entry point
   * @param {string} task - The task to execute
   * @returns {Promise<Object>} - Execution result
   */
  async execute(task) {
    throw new Error('Subclasses must implement execute()');
  }

  /**
   * Get AI response for the task
   * @param {string} task - The task
   * @returns {Promise<string>} - AI response
   */
  async getAIResponse(task) {
    throw new Error('Subclasses must implement getAIResponse()');
  }

  /**
   * Process tool commands from AI response
   * @param {string} response - AI response text
   * @returns {Promise<Array>} - Tool execution results
   */
  async processTools(response) {
    const results = [];
    
    // Each tool scans for its own commands and executes
    for (const [toolName, tool] of this.tools) {
      try {
        const toolResults = await tool.processResponse(response);
        results.push(...toolResults);
      } catch (error) {
        console.error(`❌ Tool ${toolName} failed:`, error.message);
        this.metrics.errors++;
      }
    }
    
    return results;
  }

  /**
   * Add a tool to this agent
   * @param {string} name - Tool name
   * @param {Object} tool - Tool instance
   */
  addTool(name, tool) {
    this.tools.set(name, tool);
  }

  /**
   * Get agent metrics
   * @returns {Object} - Performance metrics
   */
  getMetrics() {
    return { ...this.metrics };
  }

  /**
   * Update metrics
   * @param {Object} updates - Metric updates
   */
  updateMetrics(updates) {
    Object.assign(this.metrics, updates);
  }
}

module.exports = AgentInterface;