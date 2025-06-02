#!/usr/bin/env node
/**
 * GUARDIAN CONTINUUM
 * 
 * Architecture:
 * - Guardian Instance: Never modified, always stable
 * - Can spawn experimental instances that modify their own code
 * - A/B/C/D test different variations
 * - Always has fallback to known working code
 */
const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const fs = require('fs');
const path = require('path');
const http = require('server');

const execAsync = promisify(exec);

class GuardianContinuum {
  constructor() {
    this.guardianVersion = fs.readFileSync(__filename, 'utf8'); // Save original
    this.experimentalInstances = new Map();
    this.testResults = new Map();
    this.costs = { total: 0, requests: 0 };
    
    console.log('🛡️ GUARDIAN CONTINUUM - Self-Modifying Pool');
    console.log('===========================================');
    console.log('✅ Guardian instance: NEVER modified');
    console.log('🧪 Experimental instances: Can modify code');
    console.log('📊 A/B/C/D testing: Compare variations');
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

  async createGuardianInstance() {
    const prompt = `You are the Guardian instance of Continuum. Your job:
1. NEVER modify your own code
2. Monitor experimental instances
3. Revert to stable code if experiments fail
4. Coordinate between experimental instances
5. Always maintain system stability

You are the immortal overseer. Respond with your status.`;

    const response = await this.callClaude(prompt);
    this.guardianSessionId = response.session_id;
    
    console.log(`🛡️ Guardian Instance Created: ${this.guardianSessionId}`);
    return response;
  }

  async createExperimentalInstance(name, modificationTask) {
    const prompt = `You are ${name}, an experimental instance. Your task:
${modificationTask}

You CAN modify code and create variations of yourself.
You should test your changes and report results.
If your changes fail, the Guardian will revert to stable code.

Start by analyzing the current continuum code and proposing modifications.`;

    const response = await this.callClaude(prompt);
    
    this.experimentalInstances.set(name, {
      sessionId: response.session_id,
      task: modificationTask,
      created: new Date(),
      status: 'active',
      modifications: [],
      testResults: []
    });
    
    console.log(`🧪 Experimental Instance Created: ${name} (${response.session_id})`);
    return response;
  }

  async forkInstance(originalSessionId, forkName, variation) {
    // Get current state from original instance
    const statePrompt = `Save your current state and context. I'm about to fork you.`;
    const stateResponse = await this.callClaude(statePrompt, originalSessionId);
    
    // Create fork with variation
    const forkPrompt = `You are a fork of another instance. 
    
Original state: ${stateResponse.result}

Your variation: ${variation}

Continue from where the original left off, but with your specific variation applied.`;

    const forkResponse = await this.callClaude(forkPrompt);
    
    this.experimentalInstances.set(forkName, {
      sessionId: forkResponse.session_id,
      forkedFrom: originalSessionId,
      variation: variation,
      created: new Date(),
      status: 'active'
    });
    
    console.log(`🍴 Forked Instance: ${forkName} from original`);
    return forkResponse;
  }

  async testVariations(testCases) {
    console.log(`🧪 Running A/B/C/D tests on ${testCases.length} test cases`);
    
    const results = new Map();
    
    for (const [instanceName, instance] of this.experimentalInstances) {
      console.log(`📊 Testing ${instanceName}...`);
      
      const instanceResults = [];
      for (const testCase of testCases) {
        try {
          const startTime = Date.now();
          const response = await this.callClaude(testCase, instance.sessionId);
          const duration = Date.now() - startTime;
          
          instanceResults.push({
            test: testCase,
            response: response.result,
            duration: duration,
            cost: response.cost_usd,
            success: true
          });
        } catch (error) {
          instanceResults.push({
            test: testCase,
            error: error.message,
            success: false
          });
        }
      }
      
      results.set(instanceName, instanceResults);
    }
    
    this.testResults.set(Date.now(), results);
    return results;
  }

  async guardianRevert() {
    console.log('🛡️ Guardian reverting to stable code...');
    
    // Kill all experimental instances
    for (const [name, instance] of this.experimentalInstances) {
      console.log(`❌ Terminating experimental instance: ${name}`);
    }
    this.experimentalInstances.clear();
    
    // Restore original code
    fs.writeFileSync(__filename, this.guardianVersion);
    
    console.log('✅ System reverted to stable state');
    return 'Reverted to stable guardian version';
  }

  async start() {
    // Create guardian instance
    await this.createGuardianInstance();
    
    // Create initial experimental instances
    await this.createExperimentalInstance('ModifierClaude', 
      'Analyze the current continuum system and propose improvements to coordination');
    
    await this.createExperimentalInstance('OptimizerClaude',
      'Focus on optimizing performance and reducing costs');
    
    // Start web interface
    this.startWebInterface();
    
    console.log('🎉 Guardian Continuum ready!');
  }

  startWebInterface() {
    const server = http.createServer((req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      
      if (req.method === 'OPTIONS') {
        res.writeHead(200);
        res.end();
        return;
      }

      const url = new URL(req.url, 'http://localhost:5565');
      
      if (req.method === 'POST' && url.pathname === '/fork') {
        let body = '';
        req.on('data', chunk => body += chunk);
        req.on('end', async () => {
          try {
            const { originalInstance, forkName, variation } = JSON.parse(body);
            const original = this.experimentalInstances.get(originalInstance);
            
            if (!original) {
              res.writeHead(404, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ error: 'Original instance not found' }));
              return;
            }
            
            const forkResult = await this.forkInstance(original.sessionId, forkName, variation);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ success: true, fork: forkResult }));
          } catch (error) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: error.message }));
          }
        });
      }
      else if (req.method === 'POST' && url.pathname === '/test') {
        let body = '';
        req.on('data', chunk => body += chunk);
        req.on('end', async () => {
          try {
            const { testCases } = JSON.parse(body);
            const results = await this.testVariations(testCases);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ results: Array.from(results.entries()) }));
          } catch (error) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: error.message }));
          }
        });
      }
      else if (req.method === 'POST' && url.pathname === '/revert') {
        try {
          const result = await this.guardianRevert();
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ success: true, message: result }));
        } catch (error) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: error.message }));
        }
      }
      else if (req.method === 'GET' && url.pathname === '/status') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          guardian: this.guardianSessionId,
          experimental: Array.from(this.experimentalInstances.entries()),
          costs: this.costs,
          testResults: Array.from(this.testResults.entries())
        }));
      }
      else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    server.listen(5565, () => {
      console.log('🌐 Guardian Continuum API at http://localhost:5565');
      console.log('📋 POST /fork - Fork an instance with variation');
      console.log('🧪 POST /test - Run A/B/C/D tests');
      console.log('🛡️ POST /revert - Guardian revert to stable');
      console.log('📊 GET /status - System status');
    });
  }
}

// Start if run directly
if (require.main === module) {
  const guardian = new GuardianContinuum();
  guardian.start().catch(console.error);
}

module.exports = GuardianContinuum;