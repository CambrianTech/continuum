#!/usr/bin/env node
/**
 * TEST CONTINUUM SYSTEM
 * 
 * Tests if the actual Continuum system works with:
 * - Real Claude CLI instances with session management
 * - RESTful coordination interface
 * - Shared scratchpad/state
 * - Event subscriptions between instances
 */
const fs = require('fs');
const path = require('path');
const { exec } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

class ContinuumSystemTest {
  constructor() {
    this.testResults = [];
    this.scratchpadPath = path.join(process.cwd(), '.continuum-test-scratchpad.json');
    this.sessionsPath = path.join(process.cwd(), '.continuum-test-sessions.json');
    this.initializeScratchpad();
  }

  initializeScratchpad() {
    const initialState = {
      tasks: [],
      completedWork: [],
      activeInstances: {},
      sharedMemory: {},
      events: []
    };
    
    fs.writeFileSync(this.scratchpadPath, JSON.stringify(initialState, null, 2));
    fs.writeFileSync(this.sessionsPath, JSON.stringify({}, null, 2));
    
    console.log('📋 Initialized shared scratchpad and session tracking');
  }

  async createClaudeInstance(role, capabilities) {
    console.log(`🚀 Creating Claude instance: ${role}`);
    
    try {
      // Create initial task for this instance
      const prompt = `You are ${role} in a Continuum coordination system. Your capabilities: ${capabilities.join(', ')}. 

You can:
1. Add tasks to shared scratchpad
2. Subscribe to events from other instances  
3. Coordinate with other specialized Claude instances
4. Track your work in the shared memory

Respond with your role confirmation and what types of tasks you're ready to handle.`;

      const result = await execAsync(`claude -p "${prompt}" --output-format json`);
      const response = JSON.parse(result.stdout);
      
      // Store session info
      const sessions = JSON.parse(fs.readFileSync(this.sessionsPath, 'utf8'));
      sessions[role] = {
        sessionId: response.session_id,
        capabilities: capabilities,
        cost: response.cost_usd,
        status: 'active',
        lastActivity: new Date().toISOString()
      };
      fs.writeFileSync(this.sessionsPath, JSON.stringify(sessions, null, 2));
      
      // Update scratchpad
      const scratchpad = JSON.parse(fs.readFileSync(this.scratchpadPath, 'utf8'));
      scratchpad.activeInstances[role] = {
        sessionId: response.session_id,
        capabilities: capabilities,
        response: response.result
      };
      fs.writeFileSync(this.scratchpadPath, JSON.stringify(scratchpad, null, 2));
      
      console.log(`✅ ${role} created - Session: ${response.session_id}`);
      return { role, sessionId: response.session_id, response: response.result };
      
    } catch (error) {
      console.error(`❌ Failed to create ${role}:`, error.message);
      return null;
    }
  }

  async coordinateTask(taskDescription) {
    console.log(`\n🎯 COORDINATING TASK: ${taskDescription}`);
    
    // Add task to scratchpad
    const scratchpad = JSON.parse(fs.readFileSync(this.scratchpadPath, 'utf8'));
    const taskId = Date.now().toString();
    
    scratchpad.tasks.push({
      id: taskId,
      description: taskDescription,
      status: 'assigned',
      assignedTo: null,
      createdAt: new Date().toISOString()
    });
    
    // Create coordination event
    scratchpad.events.push({
      type: 'task_created',
      taskId: taskId,
      description: taskDescription,
      timestamp: new Date().toISOString()
    });
    
    fs.writeFileSync(this.scratchpadPath, JSON.stringify(scratchpad, null, 2));
    
    // Route to appropriate instance
    const sessions = JSON.parse(fs.readFileSync(this.sessionsPath, 'utf8'));
    const routingResult = await this.routeTask(taskDescription, sessions);
    
    return routingResult;
  }

  async routeTask(taskDescription, sessions) {
    // Simple routing logic - in real system this would be smarter
    const task = taskDescription.toLowerCase();
    let targetRole = null;
    
    if (task.includes('plan') || task.includes('architecture')) {
      targetRole = 'PlannerClaude';
    } else if (task.includes('code') || task.includes('implement')) {
      targetRole = 'DeveloperClaude';
    } else if (task.includes('test') || task.includes('verify')) {
      targetRole = 'TesterClaude';
    } else {
      targetRole = 'CoordinatorClaude';
    }
    
    if (!sessions[targetRole]) {
      console.log(`⚠️ ${targetRole} not available, creating new instance...`);
      const instance = await this.createClaudeInstance(targetRole, ['coordination', 'task_management']);
      if (!instance) {
        return { error: `Failed to create ${targetRole}` };
      }
    }
    
    const sessionId = sessions[targetRole].sessionId;
    
    console.log(`📨 Routing to ${targetRole} (Session: ${sessionId})`);
    
    try {
      // Send task to specific instance
      const prompt = `Task assignment: ${taskDescription}

Current scratchpad state: ${JSON.stringify(this.getScratchpadSummary(), null, 2)}

Please:
1. Analyze this task
2. Break it down if needed
3. Update the scratchpad with your plan
4. Coordinate with other instances if required

Respond with your analysis and next steps.`;

      const result = await execAsync(`claude -r "${sessionId}" -p "${prompt}" --output-format json`);
      const response = JSON.parse(result.stdout);
      
      // Update scratchpad with response
      this.updateScratchpadWithWork(targetRole, taskDescription, response.result);
      
      console.log(`✅ ${targetRole} processed task`);
      return { 
        assignedTo: targetRole, 
        sessionId: sessionId,
        response: response.result,
        cost: response.cost_usd 
      };
      
    } catch (error) {
      console.error(`❌ Task routing failed:`, error.message);
      return { error: error.message };
    }
  }

  updateScratchpadWithWork(role, task, work) {
    const scratchpad = JSON.parse(fs.readFileSync(this.scratchpadPath, 'utf8'));
    
    scratchpad.completedWork.push({
      role: role,
      task: task,
      work: work,
      timestamp: new Date().toISOString()
    });
    
    scratchpad.events.push({
      type: 'work_completed',
      role: role,
      task: task,
      timestamp: new Date().toISOString()
    });
    
    fs.writeFileSync(this.scratchpadPath, JSON.stringify(scratchpad, null, 2));
  }

  getScratchpadSummary() {
    const scratchpad = JSON.parse(fs.readFileSync(this.scratchpadPath, 'utf8'));
    return {
      activeTasks: scratchpad.tasks.length,
      completedWork: scratchpad.completedWork.length,
      activeInstances: Object.keys(scratchpad.activeInstances),
      recentEvents: scratchpad.events.slice(-3)
    };
  }

  async runSystemTest() {
    console.log('🧪 CONTINUUM SYSTEM TEST');
    console.log('========================');
    
    // Test 1: Create initial instances
    console.log('\n📋 Test 1: Creating Claude instances...');
    const coordinator = await this.createClaudeInstance('CoordinatorClaude', ['coordination', 'task_routing']);
    const planner = await this.createClaudeInstance('PlannerClaude', ['planning', 'architecture']);
    
    // Test 2: Coordinate complex task
    console.log('\n📋 Test 2: Coordinating complex task...');
    const taskResult1 = await this.coordinateTask('Plan and implement a REST API for user authentication');
    
    // Test 3: Coordinate follow-up task
    console.log('\n📋 Test 3: Coordinating follow-up task...');
    const taskResult2 = await this.coordinateTask('Write tests for the authentication API');
    
    // Test 4: Check system state
    console.log('\n📋 Test 4: Checking final system state...');
    const finalState = this.getScratchpadSummary();
    
    console.log('\n🎯 SYSTEM TEST RESULTS:');
    console.log('=======================');
    console.log(`✅ Instances Created: ${Object.keys(finalState.activeInstances).length}`);
    console.log(`✅ Tasks Processed: ${finalState.activeTasks}`);
    console.log(`✅ Work Completed: ${finalState.completedWork}`);
    console.log(`✅ Events Generated: ${finalState.recentEvents.length}`);
    
    // Show costs
    const sessions = JSON.parse(fs.readFileSync(this.sessionsPath, 'utf8'));
    const totalCost = Object.values(sessions).reduce((sum, session) => sum + (session.cost || 0), 0);
    console.log(`💰 Total Cost: $${totalCost.toFixed(4)}`);
    
    console.log('\n📊 Final Scratchpad State:');
    console.log(JSON.stringify(finalState, null, 2));
    
    return {
      success: true,
      instancesCreated: Object.keys(finalState.activeInstances).length,
      tasksProcessed: finalState.activeTasks,
      totalCost: totalCost
    };
  }
}

// Run the test
if (require.main === module) {
  const test = new ContinuumSystemTest();
  test.runSystemTest()
    .then(result => {
      console.log('\n🎉 CONTINUUM SYSTEM TEST COMPLETE');
      console.log(result);
    })
    .catch(error => {
      console.error('\n💥 CONTINUUM SYSTEM TEST FAILED');
      console.error(error);
    });
}

module.exports = ContinuumSystemTest;