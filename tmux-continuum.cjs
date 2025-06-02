#!/usr/bin/env node
/**
 * TMUX CONTINUUM
 * 
 * Launches Claude instances in tmux sessions via Continuum
 * Uses tmux for proper terminal environment that Claude expects
 */
const { exec, spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const http = require('http');
const WebSocket = require('ws');

class TmuxContinuum {
  constructor() {
    this.claudeInstances = new Map();
    this.sessionPrefix = 'continuum-claude';
    this.isRunning = false;
  }

  async launchContinuumPool() {
    console.log('🔥 TMUX CONTINUUM - Launching Claude Pool in tmux');
    console.log('===============================================');
    
    // Create web interface
    await this.createWebInterface();
    
    // Launch Claude instances in tmux sessions
    await this.spawnClaudeInTmux('QuestionerClaude', 'Ask clarifying questions');
    await this.spawnClaudeInTmux('PlannerClaude', 'Create detailed plans');
    await this.spawnClaudeInTmux('ImplementerClaude', 'Execute and implement solutions');
    
    this.isRunning = true;
    console.log('🎉 Tmux Continuum Pool launched!');
    console.log('🌐 Access at http://localhost:5556');
    console.log('🖥️  View tmux sessions: tmux list-sessions | grep continuum');
  }

  async spawnClaudeInTmux(instanceName, role) {
    console.log(`🚀 Spawning ${instanceName} in tmux session...`);
    
    const sessionName = `${this.sessionPrefix}-${instanceName.toLowerCase()}`;
    
    // Create tmux session with Claude
    return new Promise((resolve, reject) => {
      const createCommand = `tmux new-session -d -s "${sessionName}" claude`;
      
      exec(createCommand, (error, stdout, stderr) => {
        if (error) {
          console.log(`❌ Failed to create tmux session: ${error.message}`);
          reject(error);
          return;
        }
        
        console.log(`✅ ${instanceName} tmux session created: ${sessionName}`);
        
        // Store instance config
        const config = {
          name: instanceName,
          role: role,
          sessionName: sessionName,
          status: 'ready',
          messageHistory: [],
          lastActivity: new Date().toISOString()
        };
        
        this.claudeInstances.set(instanceName, config);
        resolve(config);
      });
    });
  }

  async sendMessageToClaude(instanceName, message) {
    const instance = this.claudeInstances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }

    console.log(`📨 Sending to ${instanceName}: "${message}"`);
    
    // Create role prompt
    const rolePrompt = this.createRolePrompt(instance.role, message);
    
    try {
      // Send to tmux session
      const response = await this.sendToTmuxSession(instance.sessionName, rolePrompt);
      
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

  async sendToTmuxSession(sessionName, prompt) {
    // Create a temporary file for the prompt
    const tempFile = path.join(process.cwd(), `.tmux-prompt-${Date.now()}.txt`);
    fs.writeFileSync(tempFile, prompt);
    
    return new Promise((resolve, reject) => {
      // Send the prompt to Claude in the tmux session
      const sendCommand = `tmux send-keys -t "${sessionName}" "$(cat ${tempFile})" Enter`;
      
      exec(sendCommand, (error, stdout, stderr) => {
        if (error) {
          fs.unlinkSync(tempFile);
          reject(new Error(`Failed to send to tmux: ${error.message}`));
          return;
        }
        
        // Wait a moment then capture the response
        setTimeout(() => {
          const captureCommand = `tmux capture-pane -t "${sessionName}" -p`;
          
          exec(captureCommand, (captureError, captureStdout, captureStderr) => {
            fs.unlinkSync(tempFile);
            
            if (captureError) {
              reject(new Error(`Failed to capture response: ${captureError.message}`));
              return;
            }
            
            // Parse the response from tmux output
            const lines = captureStdout.split('\n');
            const response = this.parseClaudeResponse(lines);
            
            if (response) {
              resolve(response);
            } else {
              reject(new Error('No valid response captured'));
            }
          });
        }, 3000); // Wait 3 seconds for Claude to respond
      });
    });
  }

  parseClaudeResponse(tmuxLines) {
    // Find the Claude response in tmux output
    // Look for lines that aren't prompts or system messages
    const responseLines = [];
    let foundResponse = false;
    
    for (const line of tmuxLines) {
      const trimmed = line.trim();
      
      // Skip empty lines and Claude prompts
      if (!trimmed || trimmed.startsWith('claude>') || trimmed.startsWith('User:')) {
        continue;
      }
      
      // Look for actual response content
      if (trimmed.length > 0 && !trimmed.includes('Welcome to Claude')) {
        responseLines.push(trimmed);
        foundResponse = true;
      }
    }
    
    return foundResponse ? responseLines.join(' ').trim() : null;
  }

  createRolePrompt(role, userMessage) {
    const roleContexts = {
      'Ask clarifying questions': `You are QuestionerClaude. Your role is to ask clarifying questions when users make requests. Always ask follow-up questions to better understand what the user needs. Be specific and helpful.

User: ${userMessage}