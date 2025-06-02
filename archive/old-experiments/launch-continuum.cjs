#!/usr/bin/env node
/**
 * CONTINUUM SPAWN
 * 
 * Launches Claude instances via Continuum into the pool
 * Uses the Real Continuum architecture for proper Claude coordination
 */
const fs = require('fs');
const path = require('path');
const http = require('http');
const WebSocket = require('ws');
const { spawn } = require('child_process');

class ContinuumSpawn {
  constructor() {
    this.claudeInstances = new Map();
    this.websocketServer = null;
    this.httpServer = null;
    this.messageQueue = [];
    this.isRunning = false;
  }

  async launchContinuumPool() {
    console.log('🌌 CONTINUUM SPAWN - Launching Claude Pool');
    console.log('==========================================');
    
    // Create web interface
    await this.createWebInterface();
    
    // Launch Claude instances via Continuum
    await this.spawnClaudeViaContinuum('QuestionerClaude', 'Ask clarifying questions');
    await this.spawnClaudeViaContinuum('PlannerClaude', 'Create detailed plans');
    await this.spawnClaudeViaContinuum('ImplementerClaude', 'Execute and implement solutions');
    
    this.isRunning = true;
    console.log('🎉 Continuum Pool launched with Claude instances!');
    console.log('🌐 Access at http://localhost:5556');  // Different port to avoid conflicts
  }

  async spawnClaudeViaContinuum(instanceName, role) {
    console.log(`🚀 Spawning ${instanceName} via Continuum...`);
    
    // Create instance config
    const config = {
      name: instanceName,
      role: role,
      type: 'claude-cli',
      status: 'initializing',
      messageHistory: [],
      lastActivity: new Date().toISOString(),
      continuumProcess: null
    };
    
    // Launch via Continuum by creating a dedicated Claude process
    config.continuumProcess = await this.createContinuumClaude(instanceName, role);
    
    // Store instance
    this.claudeInstances.set(instanceName, config);
    
    // Mark as ready
    config.status = 'ready';
    console.log(`✅ ${instanceName} spawned via Continuum and ready`);
    
    return config;
  }

  async createContinuumClaude(instanceName, role) {
    // Create a communication directory for this instance
    const commDir = path.join(process.cwd(), `.continuum-${instanceName}`);
    if (!fs.existsSync(commDir)) {
      fs.mkdirSync(commDir);
    }

    // Create input/output files for communication
    const inputFile = path.join(commDir, 'input.txt');
    const outputFile = path.join(commDir, 'output.txt');
    const statusFile = path.join(commDir, 'status.txt');

    // Initialize files
    fs.writeFileSync(statusFile, 'ready');
    fs.writeFileSync(inputFile, '');
    fs.writeFileSync(outputFile, '');

    console.log(`📁 Created Continuum communication directory: ${commDir}`);

    return {
      commDir,
      inputFile,
      outputFile,
      statusFile,
      instanceName,
      role
    };
  }

  async sendMessageToClaude(instanceName, message) {
    const instance = this.claudeInstances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }

    console.log(`📨 Sending to ${instanceName}: "${message}"`);
    
    // Create prompt with role context
    const rolePrompt = this.createRolePrompt(instance.role, message);
    
    try {
      // Use Continuum's file-based communication
      const response = await this.callClaudeViaContinuum(instance.continuumProcess, rolePrompt);
      
      // Store in history
      instance.messageHistory.push({
        timestamp: new Date().toISOString(),
        user: message,
        assistant: response
      });
      instance.lastActivity = new Date().toISOString();
      
      console.log(`📨 ${instanceName} responded: "${response}"`);
      return response;
      
    } catch (error) {
      console.error(`❌ ${instanceName} failed: ${error.message}`);
      throw error;
    }
  }

  async callClaudeViaContinuum(continuumProcess, prompt) {
    const { inputFile, outputFile, statusFile } = continuumProcess;

    // Write the prompt to input file
    fs.writeFileSync(inputFile, prompt);
    fs.writeFileSync(statusFile, 'processing');

    // Call Claude directly and redirect output to the output file
    const escapedPrompt = prompt.replace(/"/g, '\\"').replace(/\n/g, '\\n');
    const command = `claude --print "${escapedPrompt}" > "${outputFile}" 2>/dev/null || echo "Error" > "${outputFile}"`;

    return new Promise((resolve, reject) => {
      const { exec } = require('child_process');
      
      exec(command, (error, stdout, stderr) => {
        try {
          // Read the output file
          const response = fs.readFileSync(outputFile, 'utf8').trim();
          
          if (response && response !== 'Error' && response.length > 0) {
            fs.writeFileSync(statusFile, 'ready');
            resolve(response);
          } else {
            fs.writeFileSync(statusFile, 'error');
            reject(new Error('No valid response from Claude'));
          }
        } catch (readError) {
          fs.writeFileSync(statusFile, 'error');
          reject(new Error(`Failed to read response: ${readError.message}`));
        }
      });
    });
  }

  createRolePrompt(role, userMessage) {
    const roleContexts = {
      'Ask clarifying questions': `You are QuestionerClaude. Your role is to ask clarifying questions when users make requests. Always ask follow-up questions to better understand what the user needs. Be specific and helpful.

User: ${userMessage}