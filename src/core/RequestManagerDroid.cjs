/**
 * Request Manager Droid
 * Handles all incoming API requests and routes them appropriately
 */

class RequestManagerDroid {
  constructor(continuum) {
    this.continuum = continuum;
    this.requestCount = 0;
    this.activeRequests = new Map();
  }

  async handleRequest(task, requestId = null) {
    const id = requestId || `req_${Date.now()}_${++this.requestCount}`;
    
    console.log(`🤖 RequestManager: Handling task "${task.substring(0, 50)}..." [${id}]`);
    
    this.activeRequests.set(id, {
      task,
      startTime: Date.now(),
      status: 'processing'
    });

    try {
      // Route request based on content
      let result;
      
      if (this.isScreenshotRequest(task)) {
        result = await this.handleScreenshotRequest(task, id);
      } else if (this.isUICommand(task)) {
        result = await this.handleUICommand(task, id);
      } else if (this.isSystemQuery(task)) {
        result = await this.handleSystemQuery(task, id);
      } else {
        // Default to AI routing
        result = await this.continuum.intelligentRoute(task);
      }
      
      this.activeRequests.set(id, {
        ...this.activeRequests.get(id),
        status: 'completed',
        result,
        endTime: Date.now()
      });
      
      return result;
      
    } catch (error) {
      console.error(`❌ RequestManager: Error handling ${id}:`, error);
      
      this.activeRequests.set(id, {
        ...this.activeRequests.get(id),
        status: 'failed',
        error: error.message,
        endTime: Date.now()
      });
      
      return {
        result: `❌ Request failed: ${error.message}`,
        role: 'RequestManager',
        coordination: false
      };
    }
  }

  isScreenshotRequest(task) {
    const keywords = ['screenshot', 'capture', 'snap', 'image of interface'];
    return keywords.some(keyword => task.toLowerCase().includes(keyword));
  }

  isUICommand(task) {
    const keywords = ['drawer', 'open agent', 'close agent', 'select agent', 'switch room'];
    return keywords.some(keyword => task.toLowerCase().includes(keyword));
  }

  isSystemQuery(task) {
    const keywords = ['status', 'list agents', 'show agents', 'academy status', 'version'];
    return keywords.some(keyword => task.toLowerCase().includes(keyword));
  }

  async handleScreenshotRequest(task, requestId) {
    console.log(`📸 RequestManager: Processing screenshot request [${requestId}]`);
    
    if (!this.continuum.screenshotIntegration) {
      throw new Error('Screenshot integration not available');
    }
    
    const screenshotResult = await this.continuum.screenshotIntegration.handleScreenshotCommand(task, 'RequestManager');
    
    if (screenshotResult.success) {
      return {
        result: `📸 Screenshot captured successfully!\n\nImage saved at: ${screenshotResult.path}\nFilename: ${screenshotResult.filename}\nTimestamp: ${screenshotResult.timestamp}\n\nYou can open it with: open "${screenshotResult.path}"`,
        role: 'RequestManager',
        coordination: false,
        screenshot_path: screenshotResult.path,
        screenshot_filename: screenshotResult.filename
      };
    } else {
      throw new Error(screenshotResult.message);
    }
  }

  async handleUICommand(task, requestId) {
    console.log(`🎮 RequestManager: Processing UI command [${requestId}]`);
    
    // For now, return instruction for UI commands since they need browser interaction
    return {
      result: `🎮 UI Command recognized: "${task}"\n\nTo execute this command, open the browser at http://localhost:${this.continuum.port} and use the interface directly, or use the browser console:\n\nFor agent drawer: toggleAgentDrawer()\nFor screenshots: takeWebScreenshot()`,
      role: 'RequestManager',
      coordination: false
    };
  }

  async handleSystemQuery(task, requestId) {
    console.log(`📊 RequestManager: Processing system query [${requestId}]`);
    
    if (task.toLowerCase().includes('status')) {
      const status = await this.getSystemStatus();
      return {
        result: `📊 System Status:\n${status}`,
        role: 'RequestManager', 
        coordination: false
      };
    }
    
    if (task.toLowerCase().includes('agents')) {
      const agents = await this.getAgentsList();
      return {
        result: `🤖 Available Agents:\n${agents}`,
        role: 'RequestManager',
        coordination: false
      };
    }
    
    // Default to AI routing for other system queries
    return await this.continuum.intelligentRoute(task);
  }

  async getSystemStatus() {
    const uptime = Math.floor(process.uptime());
    const sessions = this.continuum.sessions.size;
    const costs = this.continuum.costs.total;
    
    return `Uptime: ${uptime}s | Active Sessions: ${sessions} | Total Cost: $${costs.toFixed(4)} | Requests Processed: ${this.requestCount}`;
  }

  async getAgentsList() {
    const sessions = Array.from(this.continuum.sessions.keys());
    return sessions.length > 0 ? sessions.join(', ') : 'No active agents';
  }

  getRequestStats() {
    const active = Array.from(this.activeRequests.values()).filter(r => r.status === 'processing').length;
    const completed = Array.from(this.activeRequests.values()).filter(r => r.status === 'completed').length;
    const failed = Array.from(this.activeRequests.values()).filter(r => r.status === 'failed').length;
    
    return { active, completed, failed, total: this.requestCount };
  }
}

module.exports = RequestManagerDroid;