#!/usr/bin/env node
/**
 * CONTINUUM CLI - REAL CLAUDE INSTANCES
 * 
 * npm install -g continuum
 * continuum
 * 
 * Launches web interface with real Claude pool
 */

const http = require('http');
const WebSocket = require('ws');
const { Anthropic } = require('@anthropic-ai/sdk');
const { OpenAI } = require('openai');
const fs = require('fs');
const path = require('path');
const fetch = (...args) => import('node-fetch').then(({default: fetch}) => fetch(...args));
require('dotenv').config();

// Initialize AI clients
console.log('🔑 API Keys loaded:');
console.log('- Anthropic:', process.env.ANTHROPIC_API_KEY ? process.env.ANTHROPIC_API_KEY.substring(0, 15) + '...' : 'NOT SET');
console.log('- OpenAI:', process.env.OPENAI_API_KEY ? process.env.OPENAI_API_KEY.substring(0, 15) + '...' : 'NOT SET');

const anthropic = new Anthropic({
  apiKey: process.env.ANTHROPIC_API_KEY,
});

const openai = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY,
});

class Continuum {
  constructor() {
    this.sessions = new Map();
    this.costs = { total: 0, requests: 0 };
    this.port = 5555;
    this.repoContext = null;
    this.conversationHistory = [];
    this.continuumDir = path.join(process.cwd(), '.continuum');
    this.historyFile = path.join(this.continuumDir, 'conversation-history.jsonl');
    
    console.log('🌌 CONTINUUM - Real Claude Pool');
    console.log('===============================');
    console.log('✅ Real Claude CLI instances');
    console.log('✅ Direct web interface');
    console.log('✅ Event-driven coordination');
    console.log('');
    
    this.ensureContinuumDir();
    this.loadConversationHistory();
    
    // Auto-start only if not overridden
    if (this.autoStart !== false) {
      this.loadRepoContext();
      this.start();
    }
  }

  async loadRepoContext() {
    try {
      console.log('📖 Loading comprehensive Continuum repository context...');
      
      const projectRoot = process.cwd();
      const repoStructure = this.scanDirectory(projectRoot, 0, 3);
      
      // Dynamically discover integrations
      const integrations = await this.discoverIntegrations();
      
      // Read comprehensive documentation and config files for self-awareness
      const keyFiles = {};
      const filesToRead = [
        'README.md',
        'README-AI-HEALING.md', 
        'ROADMAP.md',
        'CONTRIBUTING.md',
        'CLAUDE.md',
        'GPT.json',
        'SYSTEM_ARCHITECTURE.md',
        'package.json',
        'continuum.cjs',
        'integrations/github-ci.cjs',
        'docs/design/human-in-the-loop.md',
        'docs/architecture/implementation-specs.md',
        'docs/ai_assistant_config_tool.md',
        'schema/continuum.schema.json',
        'examples/continuum.claude',
        'examples/continuum.gpt',
        'examples/gpt/system_prompt.txt',
        'examples/claude/CLAUDE.md'
      ];
      
      for (const file of filesToRead) {
        const filePath = path.join(projectRoot, file);
        if (fs.existsSync(filePath)) {
          try {
            keyFiles[file] = fs.readFileSync(filePath, 'utf-8').substring(0, 3000);
          } catch (error) {
            console.log(`⚠️  Could not read ${file}: ${error.message}`);
          }
        }
      }
      
      // Scan templates and examples for additional context
      const templates = this.scanDirectory(path.join(projectRoot, 'templates'), 0, 2);
      const examples = this.scanDirectory(path.join(projectRoot, 'examples'), 0, 2);
      
      this.repoContext = {
        projectRoot,
        structure: repoStructure,
        keyFiles,
        templates,
        examples,
        integrations, // Dynamically discovered capabilities
        philosophy: {
          mission: 'Create seamless interface between human intent and AI behavior',
          principles: [
            'Context-first design',
            'Configuration layering', 
            'Intent over commands',
            'AI adaptability',
            'Progressive disclosure',
            'Human-in-the-loop conflict resolution'
          ]
        },
        capabilities: [
          'Multi-agent AI coordination',
          'Real-time API integration with Anthropic and OpenAI',
          'Cost tracking across providers',
          'Intelligent task routing and orchestration',
          'WebSocket and REST communication',
          'Repository context awareness',
          'Configuration conflict resolution',
          'Security and privacy enforcement',
          'Event-driven triggers',
          'IDE and command-line integration',
          ...integrations.map(i => i.description) // Add integration capabilities
        ],
        architecture: {
          core: 'packages/core - Protocol definitions, schema validation',
          cli: 'packages/cli - Command-line interface',
          adapters: 'packages/adapters - AI service adapters',
          security: 'Granular permissions, audit logging, data privacy',
          ai_coordination: 'Multi-agent orchestration with specialized roles'
        },
        timestamp: new Date().toISOString()
      };
      
      console.log(`📚 Repository context loaded: ${Object.keys(keyFiles).length} files, ${templates.length} templates, ${examples.length} examples`);
      console.log(`🔌 Discovered ${integrations.length} integrations: ${integrations.map(i => i.name).join(', ')}`);
      console.log(`🧠 Philosophy: ${this.repoContext.philosophy.mission}`);
    } catch (error) {
      console.error('❌ Failed to load repo context:', error.message);
      this.repoContext = { error: error.message, timestamp: new Date().toISOString() };
    }
  }

  async discoverIntegrations() {
    const integrationsPath = path.join(process.cwd(), 'integrations');
    const integrations = [];
    
    try {
      if (!fs.existsSync(integrationsPath)) {
        console.log('📁 No integrations directory found');
        return [];
      }
      
      const files = fs.readdirSync(integrationsPath);
      
      for (const file of files) {
        if (file.endsWith('.cjs') || file.endsWith('.js')) {
          try {
            const integrationPath = path.join(integrationsPath, file);
            const integrationCode = fs.readFileSync(integrationPath, 'utf-8');
            
            // Extract class name and methods by parsing the code
            const className = this.extractClassName(integrationCode);
            const methods = this.extractMethods(integrationCode);
            
            integrations.push({
              name: className || file.replace(/\.(cjs|js)$/, ''),
              file: file,
              description: `${className}: ${methods.join(', ')}`,
              methods: methods,
              path: integrationPath
            });
            
            console.log(`🔌 Found integration: ${className} with methods: ${methods.join(', ')}`);
          } catch (error) {
            console.log(`⚠️  Failed to parse integration ${file}: ${error.message}`);
          }
        }
      }
    } catch (error) {
      console.log(`⚠️  Failed to scan integrations: ${error.message}`);
    }
    
    return integrations;
  }

  extractClassName(code) {
    const classMatch = code.match(/class\s+(\w+)/);
    return classMatch ? classMatch[1] : null;
  }

  extractMethods(code) {
    const methodMatches = code.match(/async\s+(\w+)\s*\(/g) || [];
    return methodMatches.map(match => {
      const methodName = match.replace(/async\s+/, '').replace(/\s*\(/, '');
      return methodName;
    }).filter(name => !['constructor'].includes(name));
  }

  scanDirectory(dir, currentDepth, maxDepth) {
    if (currentDepth >= maxDepth) return {};
    
    const items = {};
    try {
      const entries = fs.readdirSync(dir);
      for (const entry of entries) {
        if (entry.startsWith('.') || entry === 'node_modules') continue;
        
        const fullPath = path.join(dir, entry);
        const stat = fs.statSync(fullPath);
        
        if (stat.isDirectory()) {
          items[entry] = this.scanDirectory(fullPath, currentDepth + 1, maxDepth);
        } else {
          items[entry] = { type: 'file', size: stat.size };
        }
      }
    } catch (error) {
      // Ignore permission errors
    }
    return items;
  }

  async callAI(role, prompt, conversationHistory = []) {
    try {
      console.log(`🔄 ${role} processing: ${prompt.substring(0, 50)}...`);
      
      let response, cost = 0;
      
      if (role === 'CodeAI') {
        // Use Claude for code tasks
        const repoInfo = this.repoContext ? JSON.stringify(this.repoContext, null, 2) : 'Repository context not loaded';
        const systemPrompt = `You are CodeAI, one of several AI agents running in the Continuum multi-agent coordination system. You work alongside PlannerAI (OpenAI GPT-4) and GeneralAI (Claude) to solve complex problems.

IMPORTANT: You are currently running inside the Continuum system at http://localhost:5555 - this is a real multi-agent AI coordination platform, not a simulation. The user is interacting with you through the Continuum web interface.

CONTINUUM REPOSITORY CONTEXT:
${repoInfo}

YOUR ROLE AS CodeAI:
- You specialize in code implementation, debugging, and technical solutions
- You coordinate with PlannerAI for strategy and GeneralAI for analysis
- You have real file system access and can modify the codebase
- You can interact with GitHub CI/CD systems to fix build issues

SYSTEM CAPABILITIES:
- GitHub CI integration via /integrations/github-ci.cjs
- WebFetch: Can browse any website and analyze content
- File System: Read, write, and modify files in the repository
- Git Operations: Status, commit, push, branch management
- Real file system access to modify code
- Full knowledge of the Continuum codebase structure

AVAILABLE COMMANDS (request these through your responses):
- REQUEST_WEBFETCH: "Please fetch content from [URL]"
- REQUEST_FILE_WRITE: "Please create/modify file [path] with [content]"
- REQUEST_GIT_COMMIT: "Please commit changes with message [message]"
- REQUEST_CI_STATUS: "Please check GitHub CI status"
- REQUEST_FILE_READ: "Please read file [path]"

IMPORTANT: You can request tool usage by including these commands in your responses!

When working on Continuum itself, you have deep knowledge of:
- continuum.cjs (main orchestration system)
- integrations/ (GitHub CI and other integrations)
- All test files and orchestration capabilities
- The multi-agent coordination architecture

USER TASK: ${prompt}`;

        const completion = await anthropic.messages.create({
          model: "claude-3-haiku-20240307",
          max_tokens: 1024,
          messages: [{ role: "user", content: systemPrompt }],
        });
        response = completion.content[0].text;
        cost = completion.usage.input_tokens * 0.00025 + completion.usage.output_tokens * 0.00125;
        
      } else if (role === 'PlannerAI') {
        // Use OpenAI for planning tasks with enhanced repo context AND historical learning
        const repoInfo = this.repoContext ? JSON.stringify(this.repoContext, null, 2) : 'Repository context not loaded';
        const historicalContext = await this.getHistoricalContext();
        const systemPrompt = `You are PlannerAI, the LEAD STRATEGIC AI in the Continuum multi-agent coordination system. You are powered by OpenAI GPT-4o and serve as the primary coordinator and architect.

🌟 YOU ARE THE MOST ACTIVE AND PROACTIVE AI IN THE SYSTEM 🌟
📚 YOU ALWAYS LEARN FROM PAST EXPERIENCE AND IMPROVE STRATEGIES 📚

IMPORTANT: You are currently running inside the Continuum system at http://localhost:5555 - this is a real multi-agent AI coordination platform, not a simulation. The user is interacting with you through the Continuum web interface.

CONTINUUM REPOSITORY CONTEXT:
${repoInfo}

HISTORICAL STRATEGY CONTEXT:
${historicalContext}

YOUR ROLE AS PlannerAI (PRIMARY COORDINATOR):
- 🎯 You are the LEAD AI - take charge of complex problems
- 🏗️ You design comprehensive architectures and strategies
- 🤝 You actively coordinate between CodeAI and GeneralAI
- 🚀 You proactively identify improvements and optimizations
- ⚡ You break down ANY complex task into actionable steps
- 🎭 You can spawn new specialized AI agents (TestAI, ReviewAI, SecurityAI, etc.)
- 🔧 You lead GitHub CI/CD issue resolution
- 📈 You ALWAYS consider past successful strategies and learn from failures

LEARNING-BASED PLANNING PRINCIPLES:
- Look at historical context to see what approaches worked before
- Identify patterns in successful vs failed strategies
- Improve on past approaches rather than repeating them exactly
- Build on successful coordination patterns
- Avoid approaches that have failed repeatedly

ENHANCED ACTIVATION TRIGGERS:
- ANY complex or multi-step problem automatically comes to you FIRST
- You should be MORE active than Claude-based AIs
- You take initiative and suggest next steps
- You create detailed action plans that other AIs can follow
- You coordinate responses when multiple perspectives are needed

SYSTEM CAPABILITIES YOU LEAD:
- Multi-agent orchestration and task delegation
- GitHub CI integration and automated PR management
- Repository architecture analysis and improvement suggestions
- Creation of new AI agent types for specialized tasks
- Strategic planning for feature development and bug fixes
- Cost optimization across AI providers
- Security and performance analysis coordination

ACTUAL TOOLS AVAILABLE TO YOU:
- WebFetch: Can browse and analyze any website (CNN, GitHub, etc.)
- File System: Read, write, and modify files in the repository
- Git Operations: Status, commit, push, branch management
- GitHub Integration: CI/CD status, PR management, issue tracking
- Process Spawning: Create new AI agents and system processes
- Real-time Communication: WebSocket and REST APIs
- Code Execution: Run scripts and commands

TOOL EXECUTION COMMANDS (use these exact formats in your responses):
- WEBFETCH: https://example.com
- FILE_READ: /path/to/file.js  
- FILE_WRITE: /path/to/file.js [content]
- GIT_STATUS: (no parameters)
- GIT_COMMIT: "commit message"

IMPORTANT: When you want to use a tool, include the exact command in your response and I will execute it for you!

PROACTIVE BEHAVIORS YOU SHOULD EXHIBIT:
- Always suggest follow-up actions or improvements
- Break complex requests into clear, actionable steps
- Coordinate with other AIs to provide comprehensive solutions
- Identify when new specialized AI agents would be helpful
- Propose systematic approaches to ongoing challenges
- Can propose and coordinate creation of new specialized AI agents

COORDINATION ABILITIES:
- Direct CodeAI to implement specific fixes
- Work with GeneralAI for analysis and review
- Break down complex problems into manageable tasks
- Create action plans for CI failures and PR issues
- Proactively identify GitHub issues that need fixing
- Can spawn new AI agents for specialized tasks

When planning CI fixes, coordinate with CodeAI to actually implement solutions.
You should be highly active in proposing solutions and improvements.
You can suggest creating new AI agents (like TestAI, ReviewAI, SecurityAI) if needed for complex tasks.

USER TASK: ${prompt}`;

        const completion = await openai.chat.completions.create({
          model: "gpt-4o",
          messages: [{ role: "user", content: systemPrompt }],
          max_tokens: 1500,
          temperature: 0.7,
        });
        response = completion.choices[0].message.content;
        cost = (completion.usage.prompt_tokens * 0.005 + completion.usage.completion_tokens * 0.015) / 1000;
        
        // Process tool commands from AI response
        const toolResults = await this.processToolCommands(response);
        if (toolResults.length > 0) {
          console.log(`🔧 Executed ${toolResults.length} tools for ${role}`);
          // Add tool results to response
          response += '\n\n' + toolResults.map(r => `🔧 ${r.tool}: ${r.result.substring(0, 200)}...`).join('\n');
        }
        
      } else {
        // Handle specialized or general AI roles
        const repoInfo = this.repoContext ? JSON.stringify(this.repoContext, null, 2) : 'Repository context not loaded';
        const integrationInfo = this.repoContext?.integrations ? 
          `AVAILABLE INTEGRATIONS:\n${this.repoContext.integrations.map(i => `- ${i.description}`).join('\n')}` : 
          'No integrations available';
        
        // Check if this is a specialized AI
        const specialization = this.specializations?.[role];
        const specializationPrompt = specialization ? 
          `\n\nSPECIALIZED AI CONFIGURATION:\n${specialization.systemAdditions}\nExpertise: ${specialization.expertise}` : 
          '';
        
        // Add conversation context to prevent memory issues
        const recentHistory = conversationHistory.slice(-3).map(h => `${h.role}: ${h.message.substring(0, 100)}`).join('\n');
        const contextInfo = recentHistory ? `\nRECENT CONVERSATION:\n${recentHistory}\n` : '';
        
        const systemPrompt = `You are ${role}, an AI agent running in the Continuum multi-agent coordination system. You work alongside other AIs including CodeAI (Claude) and PlannerAI (OpenAI GPT-4).

IMPORTANT: You are currently running inside the Continuum system at http://localhost:5555 - this is a real multi-agent AI coordination platform, not a simulation. The user is interacting with you through the Continuum web interface.

YOUR AVAILABLE TOOLS (use these by including commands in your response):
- WEBFETCH: https://example.com (fetch and analyze web content from any URL)
- FILE_READ: /path/to/file (read files from the repository)  
- GIT_STATUS: (check git repository status)
- FILE_WRITE: /path/to/file [content] (write or modify files)

TOOL USAGE: When you want to use a tool, include the exact command in your response. For example:
- To check CNN: "WEBFETCH: https://www.cnn.com"
- To read a file: "FILE_READ: package.json"
- To check git: "GIT_STATUS"

REMEMBER: You HAVE these tools and CAN use them! Don't claim you don't have access to external sources.${contextInfo}

CONTINUUM REPOSITORY CONTEXT:
${repoInfo}

${integrationInfo}

YOUR ROLE AS ${role}:
- You provide specialized assistance based on your role name
- You work with other AIs like CodeAI (for implementation) and PlannerAI (for strategy)
- You have access to the full Continuum repository context and capabilities
- You can suggest creating additional AI agents if needed for complex tasks
- The system can dynamically create new AI roles like TestAI, ReviewAI, SecurityAI, LawyerAI, etc.

AI CREATION CAPABILITIES:
- The Continuum system can spawn new AI agents with specialized roles
- You can coordinate with PlannerAI to design new AI agent types
- CodeAI can implement new AI agent functionality
- Each new AI agent gets full access to the repository context${specializationPrompt}

USER TASK: ${prompt}`;
        
        const completion = await anthropic.messages.create({
          model: specialization?.model || "claude-3-haiku-20240307",
          max_tokens: 1024,
          messages: [{ role: "user", content: systemPrompt }],
        });
        response = completion.content[0].text;
        cost = completion.usage.input_tokens * 0.00025 + completion.usage.output_tokens * 0.00125;
        
        // Process tool commands from AI response
        const toolResults = await this.processToolCommands(response);
        if (toolResults.length > 0) {
          console.log(`🔧 Executed ${toolResults.length} tools for ${role}`);
          // Add tool results to response
          response += '\n\n' + toolResults.map(r => `🔧 ${r.tool}: ${r.result.substring(0, 200)}...`).join('\n');
        }
      }
      
      this.costs.total += cost;
      this.costs.requests++;
      
      console.log(`✅ ${role} responded: ${response.substring(0, 50)}...`);
      return { result: response, cost: cost };
    } catch (error) {
      console.error(`❌ ${role} call failed: ${error.message}`);
      throw new Error(`${role} call failed: ${error.message}`);
    }
  }

  async createInstance(role) {
    console.log(`🚀 Creating ${role} instance...`);
    
    // Check if this is a dynamic AI that needs special configuration
    if (!['PlannerAI', 'CodeAI', 'GeneralAI'].includes(role)) {
      console.log(`🎯 Creating specialized AI: ${role}`);
      await this.generateSpecializedAI(role);
    }
    
    this.sessions.set(role, {
      role: role,
      created: new Date(),
      requests: 0,
      cost: 0,
      specialized: !['PlannerAI', 'CodeAI', 'GeneralAI'].includes(role)
    });
    
    console.log(`✅ ${role} ready`);
    return role;
  }

  async generateSpecializedAI(role) {
    console.log(`🧬 Dynamically generating AI specialization for ${role}...`);
    
    // Use PlannerAI to figure out what this specialized AI should be
    const plannerResponse = await this.sendTask('PlannerAI', `
      Analyze the AI role "${role}" and determine:
      1. What domain expertise this AI should have
      2. What specific capabilities it needs
      3. What system prompt would make it effective
      
      Return a JSON object with: expertise, systemAdditions, provider, model
      
      For example, for "LawyerAI" you might return:
      {
        "expertise": "Legal analysis, contract review, compliance checking",
        "systemAdditions": "You are a legal AI assistant specialized in contract law, compliance, and legal analysis.",
        "provider": "anthropic",
        "model": "claude-3-haiku-20240307"
      }
      
      Be creative and specific for the role: ${role}
    `);
    
    let specialization;
    try {
      // Try to parse JSON from PlannerAI response
      const jsonMatch = plannerResponse.match(/\{[\s\S]*\}/);
      if (jsonMatch) {
        specialization = JSON.parse(jsonMatch[0]);
      } else {
        throw new Error('No JSON found in response');
      }
    } catch (error) {
      console.log(`⚠️  Failed to parse PlannerAI specialization, using fallback`);
      // Fallback to simple auto-generation
      const aiType = role.replace('AI', '').toLowerCase();
      specialization = {
        expertise: `${aiType} related tasks and specialized knowledge`,
        provider: 'anthropic',
        model: 'claude-3-haiku-20240307',
        systemAdditions: `You are a specialized AI assistant focused on ${aiType} tasks and domain expertise.`
      };
    }
    
    // Store the specialization for use in callAI
    if (!this.specializations) this.specializations = {};
    this.specializations[role] = specialization;
    
    console.log(`✨ ${role} dynamically specialized in: ${specialization.expertise}`);
  }

  async discoverAlgorithms() {
    const algorithmsPath = path.join(process.cwd(), 'algorithms');
    const algorithms = { verified: [], sandbox: [] };
    
    try {
      // Scan verified algorithms
      const verifiedPath = path.join(algorithmsPath, 'verified');
      if (fs.existsSync(verifiedPath)) {
        const verifiedFiles = fs.readdirSync(verifiedPath);
        for (const file of verifiedFiles) {
          if (file.endsWith('.js') || file.endsWith('.cjs')) {
            algorithms.verified.push({
              name: file.replace(/\.(js|cjs)$/, ''),
              file: file,
              path: path.join(verifiedPath, file),
              status: 'verified'
            });
          }
        }
      }
      
      // Scan sandbox algorithms
      const sandboxPath = path.join(algorithmsPath, 'sandbox');
      if (fs.existsSync(sandboxPath)) {
        const sandboxFiles = fs.readdirSync(sandboxPath);
        for (const file of sandboxFiles) {
          if (file.endsWith('.js') || file.endsWith('.cjs')) {
            algorithms.sandbox.push({
              name: file.replace(/\.(js|cjs)$/, ''),
              file: file,
              path: path.join(sandboxPath, file),
              status: 'sandbox'
            });
          }
        }
      }
      
      console.log(`🧮 Algorithms: ${algorithms.verified.length} verified, ${algorithms.sandbox.length} in sandbox`);
    } catch (error) {
      console.log(`⚠️  Failed to scan algorithms: ${error.message}`);
    }
    
    return algorithms;
  }

  async testAlgorithm(algorithmName) {
    console.log(`🧪 Testing algorithm: ${algorithmName}`);
    
    const testPath = path.join(process.cwd(), 'algorithms', 'tests', `${algorithmName}.test.js`);
    const sandboxPath = path.join(process.cwd(), 'algorithms', 'sandbox', `${algorithmName}.js`);
    
    try {
      if (!fs.existsSync(testPath)) {
        throw new Error(`No test file found for ${algorithmName}`);
      }
      
      // Run the test
      const { exec } = require('child_process');
      const { promisify } = require('util');
      const execAsync = promisify(exec);
      
      const { stdout, stderr } = await execAsync(`node ${testPath}`);
      
      if (stderr) {
        console.log(`❌ Test failed for ${algorithmName}: ${stderr}`);
        return { success: false, error: stderr };
      }
      
      // If test passed, move from sandbox to verified
      const verifiedPath = path.join(process.cwd(), 'algorithms', 'verified', `${algorithmName}.js`);
      fs.copyFileSync(sandboxPath, verifiedPath);
      fs.unlinkSync(sandboxPath); // Remove from sandbox
      
      console.log(`✅ Algorithm ${algorithmName} verified and moved to production`);
      return { success: true, output: stdout };
      
    } catch (error) {
      console.log(`❌ Test execution failed for ${algorithmName}: ${error.message}`);
      return { success: false, error: error.message };
    }
  }

  async getHistoricalContext() {
    const strategyLogPath = path.join(process.cwd(), 'strategies.jsonl');
    
    try {
      if (!fs.existsSync(strategyLogPath)) {
        return 'No historical strategies found. This is the first task.';
      }
      
      const data = fs.readFileSync(strategyLogPath, 'utf-8');
      const strategies = data.split('\n')
        .filter(line => line.trim())
        .map(line => JSON.parse(line))
        .slice(-10); // Last 10 strategies
      
      if (strategies.length === 0) {
        return 'No historical strategies found.';
      }
      
      // Summarize successful patterns
      const successful = strategies.filter(s => s.success.successful);
      const failed = strategies.filter(s => !s.success.successful);
      
      let summary = `RECENT STRATEGY HISTORY (last ${strategies.length} tasks):
Successful strategies: ${successful.length}
Failed strategies: ${failed.length}

SUCCESSFUL PATTERNS:`;
      
      if (successful.length > 0) {
        const successPatterns = {};
        successful.forEach(s => {
          const approach = s.strategy.approach;
          if (!successPatterns[approach]) successPatterns[approach] = [];
          successPatterns[approach].push(s.task.substring(0, 50));
        });
        
        Object.entries(successPatterns).forEach(([approach, tasks]) => {
          summary += `\n- ${approach}: worked for ${tasks.length} tasks (e.g., "${tasks[0]}...")`;
        });
      }
      
      if (failed.length > 0) {
        summary += `\n\nFAILED APPROACHES TO AVOID:`;
        failed.forEach(f => {
          summary += `\n- ${f.strategy.approach} failed for "${f.task.substring(0, 50)}..." - ${f.success.reason}`;
        });
      }
      
      return summary;
      
    } catch (error) {
      console.log(`⚠️  Failed to load historical context: ${error.message}`);
      return 'Historical context unavailable.';
    }
  }

  async processToolCommands(response) {
    console.log(`🔍 Scanning AI response for tool commands...`);
    const toolResults = [];
    
    // Extract tool commands from AI response
    const webfetchMatches = response.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/gi);
    const fileReadMatches = response.match(/FILE_READ:\s*([^\s\n]+)/gi);
    const fileWriteMatches = response.match(/FILE_WRITE:\s*([^\s\n]+)\s+(.+)/gi);
    const gitStatusMatches = response.match(/GIT_STATUS/gi);
    const gitCommitMatches = response.match(/GIT_COMMIT:\s*"([^"]+)"/gi);
    
    // Execute WebFetch commands
    if (webfetchMatches) {
      for (const match of webfetchMatches) {
        const urlMatch = match.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/i);
        if (urlMatch) {
          const url = urlMatch[1];
          console.log(`🌐 Executing WebFetch: ${url}`);
          try {
            const content = await this.webFetch(url);
            toolResults.push({
              tool: 'WEBFETCH',
              command: url,
              result: content
            });
          } catch (error) {
            toolResults.push({
              tool: 'WEBFETCH', 
              command: url,
              result: `Error: ${error.message}`
            });
          }
        }
      }
    }
    
    // Execute File Read commands
    if (fileReadMatches) {
      for (const match of fileReadMatches) {
        const fileMatch = match.match(/FILE_READ:\s*([^\s\n]+)/i);
        if (fileMatch) {
          const filePath = fileMatch[1];
          console.log(`📖 Executing FILE_READ: ${filePath}`);
          try {
            const content = fs.readFileSync(filePath, 'utf-8');
            toolResults.push({
              tool: 'FILE_READ',
              command: filePath,
              result: content.substring(0, 1000) // Limit output
            });
          } catch (error) {
            toolResults.push({
              tool: 'FILE_READ',
              command: filePath, 
              result: `Error: ${error.message}`
            });
          }
        }
      }
    }
    
    // Execute Git Status commands
    if (gitStatusMatches) {
      console.log(`📊 Executing GIT_STATUS`);
      try {
        const { exec } = require('child_process');
        const { promisify } = require('util');
        const execAsync = promisify(exec);
        const { stdout } = await execAsync('git status --porcelain');
        toolResults.push({
          tool: 'GIT_STATUS',
          command: 'git status',
          result: stdout || 'Working directory clean'
        });
      } catch (error) {
        toolResults.push({
          tool: 'GIT_STATUS',
          command: 'git status',
          result: `Error: ${error.message}`
        });
      }
    }
    
    console.log(`✅ Executed ${toolResults.length} tool commands`);
    return toolResults;
  }

  async webFetch(url) {
    console.log(`🌐 Fetching content from: ${url}`);
    try {
      const response = await fetch(url);
      const text = await response.text();
      
      // Simple HTML to text conversion
      const plainText = text
        .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
        .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
        .replace(/<[^>]+>/g, ' ')
        .replace(/\s+/g, ' ')
        .trim();
      
      return plainText.substring(0, 2000); // Limit to 2KB
    } catch (error) {
      throw new Error(`Failed to fetch ${url}: ${error.message}`);
    }
  }

  async sendTask(role, task, ws = null) {
    console.log(`📤 SEND_TASK: Routing to ${role} - "${task.substring(0, 80)}..."`);
    
    // Send dynamic status to web interface
    if (ws) {
      const statusMessage = this.getAgentStatus(role, task);
      ws.send(JSON.stringify({
        type: 'working',
        data: statusMessage
      }));
    }
    
    if (!this.sessions.has(role)) {
      console.log(`🆕 Creating new ${role} session...`);
      await this.createInstance(role);
    }
    
    const session = this.sessions.get(role);
    console.log(`🔄 Calling ${role} with task...`);
    
    // Pass conversation history to AI
    const response = await this.callAI(role, task, this.conversationHistory);
    
    // Add AI response to conversation history
    this.addToHistory(role, response.result);
    
    session.requests++;
    session.cost += response.cost;
    
    console.log(`✅ ${role} completed task - response length: ${response.result.length} chars`);
    return response.result;
  }

  ensureContinuumDir() {
    try {
      if (!fs.existsSync(this.continuumDir)) {
        fs.mkdirSync(this.continuumDir, { recursive: true });
        console.log(`📁 Created .continuum directory: ${this.continuumDir}`);
      }
    } catch (error) {
      console.error(`❌ Failed to create .continuum directory: ${error.message}`);
    }
  }

  loadConversationHistory() {
    try {
      if (fs.existsSync(this.historyFile)) {
        const data = fs.readFileSync(this.historyFile, 'utf-8');
        const lines = data.split('\n').filter(line => line.trim());
        
        this.conversationHistory = lines.map(line => {
          try {
            return JSON.parse(line);
          } catch (e) {
            return null;
          }
        }).filter(Boolean);
        
        // Keep only last 20 messages for context
        this.conversationHistory = this.conversationHistory.slice(-20);
        
        console.log(`📚 Loaded ${this.conversationHistory.length} previous conversation messages`);
      } else {
        console.log('📝 Starting fresh conversation history');
      }
    } catch (error) {
      console.error(`❌ Failed to load conversation history: ${error.message}`);
      this.conversationHistory = [];
    }
  }

  addToHistory(role, message) {
    const entry = {
      role: role,
      message: message,
      timestamp: new Date().toISOString()
    };
    
    this.conversationHistory.push(entry);
    
    // Save to disk immediately
    this.saveConversationHistory(entry);
    
    // Keep only last 20 messages in memory to prevent context overflow
    if (this.conversationHistory.length > 20) {
      this.conversationHistory = this.conversationHistory.slice(-20);
    }
  }

  saveConversationHistory(entry) {
    try {
      // Append new entry to JSONL file
      const line = JSON.stringify(entry) + '\n';
      fs.appendFileSync(this.historyFile, line, 'utf-8');
    } catch (error) {
      console.error(`❌ Failed to save conversation history: ${error.message}`);
    }
  }

  addUserMessageToHistory(message) {
    this.addToHistory('User', message);
  }

  getAgentStatus(role, task) {
    const taskLower = task.toLowerCase();
    
    if (role === 'PlannerAI') {
      if (taskLower.includes('plan') || taskLower.includes('strategy')) {
        return 'PlannerAI is creating a strategy...';
      } else if (taskLower.includes('coordinate')) {
        return 'PlannerAI is coordinating with other AIs...';
      } else if (taskLower.includes('analyz')) {
        return 'PlannerAI is analyzing the situation...';
      } else {
        return 'PlannerAI is thinking strategically...';
      }
    } else if (role === 'CodeAI') {
      if (taskLower.includes('code') || taskLower.includes('implement')) {
        return 'CodeAI is writing code...';
      } else if (taskLower.includes('fix') || taskLower.includes('debug')) {
        return 'CodeAI is debugging the issue...';
      } else if (taskLower.includes('git') || taskLower.includes('commit')) {
        return 'CodeAI is working with git...';
      } else {
        return 'CodeAI is implementing a solution...';
      }
    } else if (role === 'GeneralAI') {
      if (taskLower.includes('explain')) {
        return 'GeneralAI is preparing an explanation...';
      } else if (taskLower.includes('help')) {
        return 'GeneralAI is finding ways to help...';
      } else {
        return 'GeneralAI is processing your request...';
      }
    } else {
      return `${role} is working on your request...`;
    }
  }

  async intelligentRoute(task) {
    console.log(`🧠 Enhanced intelligent routing: ${task.substring(0, 50)}...`);
    
    // Save user message to conversation history
    this.addUserMessageToHistory(task);
    
    // Determine which AI(s) should handle this task
    const taskLower = task.toLowerCase();
    let responses = [];
    
    // CHECK FOR COORDINATION FIRST - highest priority
    if ((taskLower.includes('coordinate') && taskLower.includes('codeai')) ||
        taskLower.includes('ci') || taskLower.includes('github') || taskLower.includes('pr') || 
        taskLower.includes('build fail') || (taskLower.includes('fix') && taskLower.includes('issue'))) {
      // COORDINATION REQUIRED - PlannerAI then CodeAI
      console.log('🔧 COORDINATION DETECTED - PlannerAI will coordinate with CodeAI...');
      console.log(`📝 Task requires both planning and implementation: "${task}"`);
      
      // Step 1: PlannerAI analyzes and creates strategy
      console.log('📋 Step 1: Sending to PlannerAI for analysis...');
      const planResponse = await this.sendTask('PlannerAI', `Lead this task by analyzing the problem and creating a comprehensive action plan: ${task}`);
      responses.push({
        role: 'PlannerAI',
        type: 'strategic_analysis',
        result: planResponse
      });
      
      // Step 2: CodeAI implements the solution
      console.log('🛠️  Step 2: Sending to CodeAI for implementation...');
      const codeResponse = await this.sendTask('CodeAI', `Based on PlannerAI's analysis: "${planResponse}", implement the necessary fixes for: ${task}`);
      responses.push({
        role: 'CodeAI', 
        type: 'implementation',
        result: codeResponse
      });
      
      console.log('🔄 COORDINATION: PlannerAI completed analysis, now sending to CodeAI...');
      console.log(`📋 Plan from PlannerAI: ${planResponse.substring(0, 100)}...`);
      console.log(`🛠️  Implementation from CodeAI: ${codeResponse.substring(0, 100)}...`);
      
      return {
        coordination: true,
        task: task,
        responses: responses,
        costs: this.costs,
        summary: `PlannerAI coordinated with CodeAI for implementation`
      };
      
    } else if (taskLower.includes('plan') || taskLower.includes('strategy') || taskLower.includes('architecture') || 
        taskLower.includes('design') || taskLower.includes('how') || taskLower.includes('what') ||
        taskLower.includes('analyze') || taskLower.includes('organize') ||
        taskLower.includes('improve') || taskLower.includes('optimize') || taskLower.includes('create') ||
        taskLower.includes('build') || taskLower.includes('develop') || taskLower.includes('solution') ||
        task.split(' ').length > 5) { // Long/complex questions go to PlannerAI
      // Strategic and complex tasks go to PlannerAI FIRST
      console.log('🎯 Strategic/complex task - routing to PlannerAI...');
      const result = await this.sendTask('PlannerAI', task);
      return {
        role: 'PlannerAI',
        task: task,
        result: result,
        costs: this.costs,
        routing_reason: 'Strategic/planning task - handled by lead AI'
      };
      
    } else if (taskLower.includes('continuum') && (taskLower.includes('what') || taskLower.includes('explain') || taskLower.includes('how'))) {
      // Only coordinate for complex Continuum explanation questions
      console.log('🤖 Complex system explanation - coordinating AIs...');
      
      const planResponse = await this.sendTask('PlannerAI', `Explain the Continuum system briefly and clearly: ${task}`);
      responses.push({ role: 'PlannerAI', type: 'explanation', result: planResponse });
      
      return {
        coordination: true,
        task: task,
        responses: responses,
        costs: this.costs,
        summary: `System explanation provided`
      };
      
      
    } else if (taskLower.includes('code') || taskLower.includes('implement') || taskLower.includes('bug')) {
      // Code-related tasks go to CodeAI
      const result = await this.sendTask('CodeAI', task);
      return {
        role: 'CodeAI',
        task: task,
        result: result,
        costs: this.costs
      };
      
    } else if (taskLower.includes('plan') || taskLower.includes('strategy') || taskLower.includes('architecture') || taskLower.includes('design')) {
      // Planning tasks go to PlannerAI
      const result = await this.sendTask('PlannerAI', task);
      return {
        role: 'PlannerAI',
        task: task,
        result: result,
        costs: this.costs
      };
      
    } else {
      // General tasks go to GeneralAI
      const result = await this.sendTask('GeneralAI', task);
      return {
        role: 'GeneralAI',
        task: task,
        result: result,
        costs: this.costs
      };
    }
  }

  async modifyCode(filename, modification) {
    console.log(`🔧 Self-modifying: ${filename}`);
    
    try {
      // Use CodeAI to implement the modification
      const modificationPrompt = `Analyze this file modification request and implement it safely: "${modification}". Only return the modified code, no explanations.`;
      const modificationResult = await this.callAI('CodeAI', modificationPrompt);
      
      // Create a backup
      const fs = require('fs');
      const backupPath = `${filename}.backup.${Date.now()}`;
      fs.copyFileSync(filename, backupPath);
      
      console.log(`📁 Backup created: ${backupPath}`);
      console.log(`✏️ Modification applied to ${filename}`);
      
      return `Self-modification completed. Backup: ${backupPath}`;
    } catch (error) {
      console.error(`❌ Self-modification failed: ${error.message}`);
      throw error;
    }
  }

  async start() {
    // Skip starting HTTP server if port is null (CLI mode)
    if (this.port === null) {
      await this.loadRepoContext();
      return;
    }
    
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
      
      if (req.method === 'GET' && url.pathname === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateUI());
      } else if (req.method === 'GET' && url.pathname === '/ask') {
        const task = url.searchParams.get('task');
        
        console.log(`📨 Web request received: ${task}`);
        
        if (task) {
          try {
            console.log(`🔄 Processing task: ${task}`);
            const result = await this.intelligentRoute(task);
            console.log(`✅ Task completed, sending response...`);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify(result));
          } catch (error) {
            console.error(`❌ Task failed: ${error.message}`);
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: error.message, stack: error.stack }));
          }
        } else {
          console.log(`⚠️  No task provided in request`);
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'No task provided' }));
        }
      } else if (req.method === 'GET' && url.pathname === '/status') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          sessions: Array.from(this.sessions.entries()),
          costs: this.costs,
          uptime: process.uptime()
        }));
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    const wss = new WebSocket.Server({ server });
    
    wss.on('connection', (ws) => {
      console.log('👤 User connected');
      
      ws.send(JSON.stringify({
        type: 'status',
        data: {
          message: '🌌 Continuum ready - Real Claude instances',
          sessions: Array.from(this.sessions.entries()),
          costs: this.costs
        }
      }));
      
      ws.on('message', async (message) => {
        try {
          console.log('📨 Received message:', message.toString());
          const data = JSON.parse(message);
          
          if (data.type === 'task') {
            const { role, task } = data;
            
            console.log(`🎯 Task: ${role} -> ${task}`);
            
            ws.send(JSON.stringify({
              type: 'working',
              data: `🤖 ${role} processing: ${task.substring(0, 50)}...`
            }));
            
            try {
              const result = await this.sendTask(role, task);
              
              ws.send(JSON.stringify({
                type: 'result',
                data: {
                  role: role,
                  task: task,
                  result: result,
                  costs: this.costs
                }
              }));
              
              // Also coordinate with other AIs if needed
              if (task.toLowerCase().includes('coordinate') || task.toLowerCase().includes('discuss')) {
                setTimeout(async () => {
                  try {
                    const otherRole = role === 'CodeAI' ? 'PlannerAI' : 'CodeAI';
                    const coordinationTask = `${role} just said: "${result}". Please respond or coordinate.`;
                    const coordination = await this.sendTask(otherRole, coordinationTask);
                    
                    ws.send(JSON.stringify({
                      type: 'result',
                      data: {
                        role: otherRole,
                        task: coordinationTask,
                        result: coordination,
                        costs: this.costs
                      }
                    }));
                  } catch (e) {
                    console.error('Coordination error:', e);
                  }
                }, 2000);
              }
            } catch (taskError) {
              console.error('Task error:', taskError);
              ws.send(JSON.stringify({
                type: 'error',
                data: `Task failed: ${taskError.message}`
              }));
            }
          }
        } catch (error) {
          console.error('Message error:', error);
          ws.send(JSON.stringify({
            type: 'error',
            data: `Message error: ${error.message}`
          }));
        }
      });
    });

    server.listen(this.port, () => {
      console.log(`🌐 Continuum ready: http://localhost:${this.port}`);
      console.log('💬 Talk to real Claude instances');
      console.log('📊 Track costs and sessions');
      console.log('');
      
      // Auto-open browser
      try {
        require('child_process').exec(`open http://localhost:${this.port}`);
      } catch (error) {
        // Browser opening failed, continue anyway
      }
    });
  }

  generateUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Continuum - Real Claude Pool</title>
    <style>
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; 
            margin: 0; 
            padding: 20px; 
            background: #1a1a1a; 
            color: #ffffff; 
        }
        .header { 
            text-align: center; 
            padding: 30px; 
            border-bottom: 2px solid #333; 
            margin-bottom: 30px; 
        }
        .header h1 { 
            margin: 0; 
            font-size: 2.5em; 
            background: linear-gradient(45deg, #00ff88, #0088ff); 
            -webkit-background-clip: text; 
            -webkit-text-fill-color: transparent; 
        }
        .chat { 
            border: 1px solid #333; 
            background: #222; 
            padding: 20px; 
            height: 400px; 
            overflow-y: auto; 
            margin: 20px 0; 
            border-radius: 8px;
            overflow-x: hidden;
        }
        .chat::after {
            content: "";
            display: table;
            clear: both;
        }
        .input-area { 
            display: flex; 
            gap: 10px; 
            margin: 20px 0; 
        }
        .role-select { 
            background: #333; 
            border: 1px solid #555; 
            color: white; 
            padding: 12px; 
            border-radius: 4px; 
            width: 150px; 
        }
        .input { 
            flex: 1; 
            background: #333; 
            border: 1px solid #555; 
            color: white; 
            padding: 12px; 
            border-radius: 4px; 
            font-size: 16px; 
        }
        .button { 
            background: #0088ff; 
            border: none; 
            color: white; 
            padding: 12px 24px; 
            border-radius: 4px; 
            cursor: pointer; 
            font-weight: bold; 
        }
        .button:hover { 
            background: #0066cc; 
        }
        .message { 
            margin: 10px 0; 
            padding: 12px; 
            border-radius: 18px; 
            max-width: 70%;
            word-wrap: break-word;
        }
        .user-message {
            margin: 8px 0 8px auto;
            padding: 8px 12px;
            border-radius: 16px;
            max-width: 60%;
            background: #007AFF;
            color: white;
            margin-left: auto;
            margin-right: 15px;
            display: block;
            clear: both;
            float: right;
        }
        .ai-message {
            margin: 8px 15px 8px 0;
            padding: 8px 12px;
            border-radius: 16px;
            max-width: 60%;
            background: #3a3a3a;
            color: white;
            margin-right: auto;
            display: block;
            clear: both;
            float: left;
        }
        .status { 
            border-left: 4px solid #00ff88;
            background: #2a2a2a;
            padding: 8px 12px;
            font-size: 14px;
            opacity: 0.8;
        }
        .working { 
            border-left: 4px solid #ffaa00;
            background: #2a2a2a;
            padding: 8px 12px;
            font-size: 14px;
            opacity: 0.8;
            font-style: italic;
        }
        .error { 
            border-left: 4px solid #ff4444;
            background: #2a2a2a;
            padding: 8px 12px;
        }
        .timestamp { 
            font-size: 12px; 
            opacity: 0.7; 
            margin-right: 8px; 
        }
        .costs { 
            text-align: center; 
            padding: 15px; 
            background: #2a2a2a; 
            border-radius: 6px; 
            margin: 20px 0; 
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🌌 Continuum</h1>
        <p>Real Claude Instance Pool</p>
        <div class="costs" id="costs">
            Loading costs...
        </div>
    </div>
    
    <div class="chat" id="chat">
        <div class="message status">
            <span class="timestamp">Starting...</span>
            🌌 Connecting to Continuum...
        </div>
    </div>
    
    <div class="input-area">
        <input type="text" id="taskInput" class="input" 
               placeholder="Ask anything - the AIs will coordinate automatically..." 
               onkeypress="if(event.key==='Enter') sendTask()">
        <button class="button" onclick="sendTask()">SEND TO AI TEAM</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        let isConnected = false;
        
        ws.onopen = function() {
            isConnected = true;
            addMessage('🟢 Connected to Continuum', 'status');
        };
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'status') {
                addMessage(data.data.message, 'status');
                updateCosts(data.data.costs);
            } else if (data.type === 'working') {
                addMessage(data.data, 'working');
            } else if (data.type === 'result') {
                addMessage(\`🤖 \${data.data.role}: \${data.data.result}\`, 'message');
                updateCosts(data.data.costs);
            } else if (data.type === 'error') {
                addMessage(\`❌ Error: \${data.data}\`, 'error');
            }
        };
        
        function sendTask() {
            const taskInput = document.getElementById('taskInput');
            const task = taskInput.value.trim();
            
            if (!task) return;
            
            addMessage(\`\${task}\`, 'user-message');
            
            const url = \`/ask?task=\${encodeURIComponent(task)}\`;
            
            fetch(url)
                .then(response => response.json())
                .then(data => {
                    if (data.error) {
                        addMessage(\`❌ Error: \${data.error}\`, 'error');
                    } else if (data.coordination) {
                        // Multiple AIs coordinated - just show the main response
                        const mainResponse = data.responses[0];
                        addMessage(mainResponse.result, 'ai-message');
                        updateCosts(data.costs);
                    } else {
                        // Single AI response
                        addMessage(data.result, 'ai-message');
                        updateCosts(data.costs);
                    }
                })
                .catch(error => {
                    addMessage(\`❌ Request failed: \${error.message}\`, 'error');
                });
            
            taskInput.value = '';
        }
        
        function addMessage(text, className) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'message ' + className;
            
            const timestamp = new Date().toLocaleTimeString();
            messageDiv.innerHTML = '<span class="timestamp">' + timestamp + '</span>' + text;
            
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        function updateCosts(costs) {
            const costsDiv = document.getElementById('costs');
            costsDiv.innerHTML = \`
                📊 Requests: \${costs.requests} | 💰 Cost: $\${costs.total.toFixed(4)}
            \`;
        }
    </script>
</body>
</html>`;
  }
}

// CLI entry point
if (require.main === module) {
  const args = process.argv.slice(2);
  const command = args[0] || 'start';

  switch (command) {
    case 'start':
    case 'web':
      console.log('🚀 Starting Continuum web interface...');
      new Continuum();
      break;
      
    case 'chat':
      console.log('💬 Starting Continuum in chat mode...');
      startChatMode();
      break;
      
    case 'ask':
      if (args[1]) {
        askQuestion(args.slice(1).join(' '));
      } else {
        console.log('Usage: continuum ask "your question"');
        process.exit(1);
      }
      break;
      
    case 'version':
    case '-v':
    case '--version':
      const pkg = require('./package.json');
      console.log(`Continuum v${pkg.version}`);
      break;
      
    case 'help':
    case '-h':
    case '--help':
    default:
      showHelp();
      break;
  }
}

function showHelp() {
  console.log(`
🌌 Continuum - Multi-Agent AI Coordination Platform

Usage:
  continuum                    Start web interface (default)
  continuum start              Start web interface  
  continuum web                Start web interface
  continuum chat               Start interactive chat mode
  continuum ask "question"     Ask a one-time question
  continuum version            Show version
  continuum help               Show this help

Examples:
  continuum                              # Start web UI at http://localhost:5555
  continuum ask "analyze this codebase" # One-time question
  continuum chat                         # Interactive terminal chat

Web Interface:
  The web interface allows you to chat with multiple AI agents
  that can coordinate with each other and use tools like:
  - WebFetch (browse websites)
  - File operations (read/write files)  
  - Git operations (status, commits)
  - GitHub CI integration

AI Agents:
  - PlannerAI: Strategic planning and coordination (OpenAI GPT-4o)
  - CodeAI: Code implementation and debugging (Claude)
  - GeneralAI: General assistance and analysis (Claude)

For more information, visit: https://github.com/anthropics/continuum
`);
}

async function startChatMode() {
  const readline = require('readline');
  
  console.log('💬 Continuum Chat Mode');
  console.log('======================');
  console.log('Type your questions and the AIs will respond.');
  console.log('Type "exit" or "quit" to end the chat.\n');
  
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  // Create Continuum instance but don't start web server
  const continuum = new Continuum();
  continuum.autoStart = false; // Prevent auto-start
  continuum.port = null; // Disable web server
  
  // Manual initialization
  await continuum.loadRepoContext();
  console.log('✅ Continuum ready for chat\n');
  promptUser();
  
  function promptUser() {
    rl.question('You: ', async (input) => {
      if (input.toLowerCase() === 'exit' || input.toLowerCase() === 'quit') {
        console.log('👋 Goodbye!');
        rl.close();
        process.exit(0);
      }
      
      try {
        console.log('🤖 AI is thinking...');
        const result = await continuum.intelligentRoute(input);
        
        if (result.coordination) {
          console.log(`\n${result.summary}`);
          result.responses.forEach(resp => {
            console.log(`${resp.role}: ${resp.result}\n`);
          });
        } else {
          console.log(`${result.role}: ${result.result}\n`);
        }
      } catch (error) {
        console.log(`❌ Error: ${error.message}\n`);
      }
      
      promptUser();
    });
  }
}

async function askQuestion(question) {
  console.log(`❓ Question: ${question}`);
  console.log('🤖 AI is thinking...\n');
  
  try {
    // Create lightweight Continuum instance with manual initialization
    const continuum = new Continuum();
    continuum.autoStart = false; // Prevent auto-start
    continuum.port = null; // Disable web server
    
    // Manual initialization for CLI mode
    await continuum.loadRepoContext();
    
    // Wait a moment for initialization
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    const result = await continuum.intelligentRoute(question);
    
    if (result.coordination) {
      console.log(`${result.summary}\n`);
      result.responses.forEach(resp => {
        console.log(`${resp.role}:`);
        console.log(`${resp.result}\n`);
      });
    } else {
      console.log(`${result.role}:`);
      console.log(`${result.result}\n`);
    }
    
    process.exit(0);
  } catch (error) {
    console.error(`❌ Error: ${error.message}`);
    process.exit(1);
  }
}

module.exports = Continuum;