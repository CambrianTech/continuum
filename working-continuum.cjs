#!/usr/bin/env node
/**
 * WORKING CONTINUUM
 * 
 * Uses the parts that actually work:
 * - claude -p with JSON output  
 * - Session management
 * - Interrupt-driven coordination
 * - Simple REST interface
 */
const { exec } = require('child_process');
const { promisify } = require('util');
const http = require('http');
const fs = require('fs');
const path = require('path');

const execAsync = promisify(exec);

class WorkingContinuum {
  constructor() {
    this.sessions = new Map();
    this.scratchpad = { tasks: [], results: [], events: [] };
    this.costs = { total: 0, requests: 0 };
    this.port = 5564;
  }

  async callClaude(prompt, sessionId = null) {
    try {
      const cmd = sessionId ? 
        `claude -r "${sessionId}" -p "${prompt}" --output-format json` :
        `claude -p "${prompt}" --output-format json`;
      
      const result = await execAsync(cmd);
      const response = JSON.parse(result.stdout);
      
      this.costs.total += response.cost_usd || 0;
      this.costs.requests++;
      
      return response;
    } catch (error) {
      throw new Error(`Claude call failed: ${error.message}`);
    }
  }

  async createInstance(role) {
    const prompt = `You are ${role} in a coordination system. Respond with: "Ready as ${role}"`;
    const response = await this.callClaude(prompt);
    
    this.sessions.set(role, {
      sessionId: response.session_id,
      role: role,
      created: new Date(),
      requests: 1,
      cost: response.cost_usd
    });
    
    return response.session_id;
  }

  async sendTask(role, task) {
    if (!this.sessions.has(role)) {
      await this.createInstance(role);
    }
    
    const session = this.sessions.get(role);
    const response = await this.callClaude(task, session.sessionId);
    
    session.requests++;
    session.cost += response.cost_usd;
    
    return response.result;
  }

  start() {
    const server = http.createServer(async (req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      
      if (req.method === 'OPTIONS') {
        res.writeHead(200);
        res.end();
        return;
      }

      const url = new URL(req.url, `http://localhost:${this.port}`);
      
      try {
        if (req.method === 'POST' && url.pathname === '/task') {
          let body = '';
          req.on('data', chunk => body += chunk);
          req.on('end', async () => {
            const { role, task } = JSON.parse(body);
            const result = await this.sendTask(role, task);
            
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ 
              success: true, 
              result: result,
              costs: this.costs,
              session: this.sessions.get(role)?.sessionId
            }));
          });
        }
        else if (req.method === 'GET' && url.pathname === '/status') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            sessions: Array.from(this.sessions.entries()),
            costs: this.costs,
            uptime: process.uptime()
          }));
        }
        else if (req.method === 'GET' && url.pathname === '/') {
          res.writeHead(200, { 'Content-Type': 'text/html' });
          res.end(`<!DOCTYPE html>
<html>
<head><title>Working Continuum</title></head>
<body>
  <h1>Working Continuum</h1>
  <p>POST /task - Send task to role</p>
  <p>GET /status - System status</p>
  
  <h2>Test Interface</h2>
  <input id="role" placeholder="Role (e.g., Planner)" />
  <input id="task" placeholder="Task description" />
  <button onclick="sendTask()">Send Task</button>
  <div id="result"></div>
  
  <script>
    async function sendTask() {
      const role = document.getElementById('role').value;
      const task = document.getElementById('task').value;
      
      const response = await fetch('/task', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ role, task })
      });
      
      const result = await response.json();
      document.getElementById('result').innerHTML = 
        '<pre>' + JSON.stringify(result, null, 2) + '</pre>';
    }
  </script>
</body>
</html>`);
        }
        else {
          res.writeHead(404);
          res.end('Not found');
        }
      } catch (error) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: error.message }));
      }
    });

    server.listen(this.port, () => {
      console.log(`🌐 Working Continuum at http://localhost:${this.port}`);
      console.log('📋 POST /task - Send task to role');
      console.log('📊 GET /status - System status');
    });
  }
}

new WorkingContinuum().start();