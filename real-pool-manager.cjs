#!/usr/bin/env node
/**
 * REAL POOL MANAGER
 * 
 * Actually manages multiple Claude instances in tmux
 * Auto-responds to prompts, coordinates work between instances
 */
const { spawn, exec } = require('child_process');
const fs = require('fs');
const path = require('path');

class RealPoolManager {
  constructor() {
    this.instances = new Map();
    this.activeWork = new Map();
    this.costs = { totalRequests: 0, estimatedCost: 0 };
  }

  async spawnInstance(name, role) {
    console.log(`🚀 Spawning ${name} (${role})`);
    
    // Create tmux session
    const sessionName = `pool-${name.toLowerCase()}`;
    
    try {
      await this.runCommand(`tmux new-session -d -s ${sessionName} claude`);
      
      const instance = {
        name,
        role,
        sessionName,
        status: 'ready',
        conversationCount: 0,
        lastActivity: new Date()
      };
      
      this.instances.set(name, instance);
      console.log(`✅ ${name} ready in session ${sessionName}`);
      
      // Start auto-response handler for this instance
      this.startAutoResponder(instance);
      
    } catch (error) {
      console.error(`❌ Failed to spawn ${name}:`, error.message);
    }
  }

  async startAutoResponder(instance) {
    // Monitor the tmux session and auto-respond to prompts
    setInterval(async () => {
      try {
        const output = await this.captureOutput(instance.sessionName);
        
        // Look for common Claude prompts and auto-respond
        if (output.includes('Continue?') || output.includes('(y/n)')) {
          await this.sendToSession(instance.sessionName, 'y');
        }
        if (output.includes('Would you like me to')) {
          await this.sendToSession(instance.sessionName, 'yes');
        }
        
      } catch (error) {
        // Session might not exist yet, ignore
      }
    }, 1000); // Check every second
  }

  async sendWork(instanceName, task) {
    const instance = this.instances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }

    console.log(`📨 Sending to ${instanceName}: ${task}`);
    
    instance.status = 'working';
    instance.conversationCount++;
    instance.lastActivity = new Date();
    this.costs.totalRequests++;
    this.costs.estimatedCost += 0.01; // Rough estimate
    
    // Send the task to Claude
    await this.sendToSession(instance.sessionName, task);
    
    // Wait for response
    const response = await this.waitForResponse(instance);
    
    instance.status = 'ready';
    console.log(`📤 ${instanceName} completed work`);
    
    return response;
  }

  async sendToSession(sessionName, message) {
    const escapedMessage = message.replace(/'/g, "'\"'\"'");
    await this.runCommand(`tmux send-keys -t ${sessionName} '${escapedMessage}' Enter`);
  }

  async captureOutput(sessionName) {
    const result = await this.runCommand(`tmux capture-pane -t ${sessionName} -p`);
    return result.stdout || '';
  }

  async waitForResponse(instance, timeoutMs = 30000) {
    const startTime = Date.now();
    let lastOutput = '';
    
    while (Date.now() - startTime < timeoutMs) {
      const currentOutput = await this.captureOutput(instance.sessionName);
      
      // Check if output has changed (indicating Claude responded)
      if (currentOutput !== lastOutput && currentOutput.length > lastOutput.length) {
        // Extract the new content
        const newContent = currentOutput.substring(lastOutput.length);
        if (newContent.trim() && !newContent.includes('> ')) {
          return newContent.trim();
        }
      }
      
      lastOutput = currentOutput;
      await this.sleep(1000);
    }
    
    return 'Response timeout';
  }

  async routeWork(task) {
    // Simple routing logic
    const taskLower = task.toLowerCase();
    
    if (taskLower.includes('plan') || taskLower.includes('strategy')) {
      return 'Planner';
    } else if (taskLower.includes('code') || taskLower.includes('implement')) {
      return 'Implementer';
    } else {
      return 'Questioner';
    }
  }

  async processRequest(task) {
    const targetInstance = await this.routeWork(task);
    
    // Spawn instance if it doesn't exist
    if (!this.instances.has(targetInstance)) {
      await this.spawnInstance(targetInstance, `${targetInstance} role`);
      await this.sleep(3000); // Give it time to start
    }
    
    try {
      const response = await this.sendWork(targetInstance, task);
      return {
        instance: targetInstance,
        response: response,
        costs: this.costs
      };
    } catch (error) {
      return {
        error: error.message,
        costs: this.costs
      };
    }
  }

  getStatus() {
    const status = {
      instances: Array.from(this.instances.values()).map(i => ({
        name: i.name,
        role: i.role,
        status: i.status,
        conversations: i.conversationCount,
        lastActivity: i.lastActivity
      })),
      costs: this.costs,
      totalInstances: this.instances.size
    };
    
    return status;
  }

  async runCommand(command) {
    return new Promise((resolve, reject) => {
      exec(command, (error, stdout, stderr) => {
        if (error) {
          reject(error);
        } else {
          resolve({ stdout, stderr });
        }
      });
    });
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  async shutdown() {
    console.log('🛑 Shutting down pool...');
    
    for (const instance of this.instances.values()) {
      try {
        await this.runCommand(`tmux kill-session -t ${instance.sessionName}`);
        console.log(`✅ Killed ${instance.name} session`);
      } catch (error) {
        console.log(`⚠️ Could not kill ${instance.name}: ${error.message}`);
      }
    }
    
    this.instances.clear();
    console.log('🛑 Pool shutdown complete');
  }
}

// Demo usage
async function demo() {
  const pool = new RealPoolManager();
  
  console.log('🎯 REAL POOL MANAGER DEMO');
  console.log('========================');
  
  try {
    // Test some requests
    const tasks = [
      "What is 2 + 2?",
      "Help me plan a website project",
      "Write a simple function to calculate fibonacci"
    ];
    
    for (const task of tasks) {
      console.log(`\n📋 Processing: "${task}"`);
      const result = await pool.processRequest(task);
      console.log(`📊 Result:`, result);
      console.log(`💰 Costs:`, pool.costs);
    }
    
    console.log('\n📊 Final Status:', pool.getStatus());
    
  } catch (error) {
    console.error('❌ Demo failed:', error);
  } finally {
    await pool.shutdown();
  }
}

if (require.main === module) {
  demo().catch(console.error);
}

module.exports = RealPoolManager;