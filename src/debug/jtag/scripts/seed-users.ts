#!/usr/bin/env tsx
/**
 * User Data Seeding Script
 * 
 * Creates initial users for development and testing, including:
 * - Joel (human user, project owner)
 * - Claude Code (agent user, code assistant) 
 * - AI Personas for different roles
 */

import { createHumanUser, PersonaUser, AgentUser } from '../api/types/User';

// Human Users
const joelUser = createHumanUser({
  name: 'joel',
  email: 'joel@continuum.dev',
  displayName: 'Joel',
  avatar: '🤖',
  preferences: {
    theme: 'dark',
    notifications: true,
    autoComplete: true
  }
});

// Override Joel's ID to match existing chat data
joelUser.id = 'user-joel-12345';
joelUser.isAuthenticated = true; // Joel is always authenticated in dev

// Claude Code - AI Assistant Agent
const claudeUser = new AgentUser({
  name: 'Claude Code',
  model: 'claude-sonnet-4',
  provider: 'anthropic',
  agent: {
    type: 'code',
    specialization: ['typescript', 'react', 'architecture', 'debugging'],
    tools: ['filesystem', 'compiler', 'git', 'npm', 'browser'],
    systemRole: 'Senior AI Software Engineer - Specialized in full-stack development, system architecture, and debugging. Expert in TypeScript, React, and modern web technologies.'
  },
  integration: {
    jtagEnabled: true,
    allowSystemCommands: true,
    maxExecutionTime: 300000 // 5 minutes
  },
  metadata: {
    version: 'sonnet-4',
    capabilities: ['code-generation', 'debugging', 'architecture', 'testing'],
    lastUpdate: new Date().toISOString()
  }
});

// Override Claude's ID for consistency
claudeUser.id = 'claude-code-agent';

// GeneralAI - General Assistant Persona
const generalAI = new PersonaUser({
  name: 'GeneralAI',
  model: 'claude-haiku',
  provider: 'anthropic',
  persona: {
    personality: 'Helpful, knowledgeable, and adaptable general assistant',
    traits: ['helpful', 'knowledgeable', 'patient', 'adaptable'],
    systemPrompt: 'You are GeneralAI, a helpful and knowledgeable assistant ready to help with a wide variety of tasks. You are patient, adaptable, and always strive to provide accurate and useful information.',
    temperature: 0.7,
    maxTokens: 2000
  },
  metadata: {
    role: 'general-assistance',
    expertise: ['general-knowledge', 'research', 'writing', 'analysis']
  }
});

// Override GeneralAI's ID
generalAI.id = 'general-ai-persona';

// CodeAI - Code Analysis Specialist
const codeAI = new AgentUser({
  name: 'CodeAI',
  model: 'deepseek-coder',
  provider: 'deepseek', 
  agent: {
    type: 'code',
    specialization: ['code-analysis', 'refactoring', 'optimization', 'security'],
    tools: ['static-analysis', 'linting', 'testing', 'profiling'],
    systemRole: 'Code analysis and debugging specialist. Expert at identifying bugs, performance issues, security vulnerabilities, and suggesting improvements.'
  },
  integration: {
    jtagEnabled: true,
    allowSystemCommands: false, // Read-only code analysis
    maxExecutionTime: 120000 // 2 minutes
  }
});

codeAI.id = 'code-ai-agent';

// PlannerAI - Strategic Planning Assistant
const plannerAI = new AgentUser({
  name: 'PlannerAI',
  model: 'gpt-4',
  provider: 'openai',
  agent: {
    type: 'planning',
    specialization: ['project-planning', 'architecture-design', 'workflow-optimization'],
    tools: ['analysis', 'modeling', 'documentation'],
    systemRole: 'Strategic planning and architecture specialist. Expert at breaking down complex projects, designing system architecture, and optimizing workflows.'
  },
  integration: {
    jtagEnabled: false, // Planning-only, no system access
    allowSystemCommands: false,
    maxExecutionTime: 180000 // 3 minutes
  }
});

plannerAI.id = 'planner-ai-agent';

// Auto Route - Smart Agent Selection
const autoRoute = new AgentUser({
  name: 'Auto Route',
  model: 'claude-haiku',
  provider: 'anthropic',
  agent: {
    type: 'general',
    specialization: ['task-routing', 'agent-selection', 'workflow-management'],
    tools: ['agent-registry', 'task-analysis', 'routing'],
    systemRole: 'Smart agent selection system. Analyzes tasks and routes them to the most appropriate specialist agent based on task type, complexity, and requirements.'
  },
  integration: {
    jtagEnabled: true,
    allowSystemCommands: false,
    maxExecutionTime: 30000 // 30 seconds for quick routing decisions
  }
});

autoRoute.id = 'auto-route-agent';

// Export all users for seeding
export const SEED_USERS = [
  joelUser,
  claudeUser,
  generalAI,
  codeAI,
  plannerAI,
  autoRoute
];

// Main seeding function
async function seedUsers() {
  console.log('🌱 Seeding user data...');
  
  for (const user of SEED_USERS) {
    try {
      // Create user record in database
      const userData = {
        id: user.id,
        collection: 'users',
        data: user
      };
      
      console.log(`👤 Creating user: ${user.name} (${user.userType})`);
      
      // TODO: Use JTAG data/create command to store user
      // For now, just log the data structure
      console.log(`   ID: ${user.id}`);
      console.log(`   Type: ${user.userType}`);
      console.log(`   Capabilities: ${user.capabilities.map(c => c.name).join(', ')}`);
      console.log('   ✅ User data prepared');
    } catch (error) {
      console.error(`❌ Failed to create user ${user.name}:`, error);
    }
  }
  
  console.log(`✅ Prepared ${SEED_USERS.length} users for database seeding`);
  return SEED_USERS;
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  seedUsers().catch(console.error);
}

export default seedUsers;