/**
 * Tool Interface - Protocol for all tools
 * 
 * Each tool knows how to:
 * 1. Parse AI responses for its commands
 * 2. Execute those commands
 * 3. Return results
 * 
 * Tools are completely self-contained and handle their own I/O
 */

class ToolInterface {
  constructor(name) {
    if (new.target === ToolInterface) {
      throw new Error('ToolInterface is abstract and cannot be instantiated');
    }
    this.name = name;
    this.metrics = { executions: 0, errors: 0, totalTime: 0 };
  }

  /**
   * Process AI response and execute any commands for this tool
   * @param {string} response - AI response text
   * @returns {Promise<Array>} - Array of execution results
   */
  async processResponse(response) {
    throw new Error('Subclasses must implement processResponse()');
  }

  /**
   * Execute a specific command for this tool
   * @param {string} command - Command to execute
   * @returns {Promise<Object>} - Execution result
   */
  async execute(command) {
    throw new Error('Subclasses must implement execute()');
  }

  /**
   * Get tool metrics
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
    this.metrics.executions++;
  }
}

module.exports = ToolInterface;