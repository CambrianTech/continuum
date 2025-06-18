/**
 * AgentsCommand - Show agent-specific development help and guidance
 */

const InfoCommand = require('../info/InfoCommand.cjs');

class AgentsCommand extends InfoCommand {
  static getDefinition() {
    // README-driven: Read definition from README.md
    const fs = require('fs');
    const path = require('path');
    
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      return this.parseReadmeDefinition(readme);
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'agents',
        description: 'Show agent-specific development help and guidance',
        icon: '🤖',
        parameters: {
          section: {
            type: 'string',
            required: false,
            description: 'Agent help section: setup, workflow, debugging, commands',
            default: 'overview'
          }
        },
        examples: [
          'agents',
          'agents --section setup',
          'agents --section debugging'
        ]
      };
    }
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    
    this.displayHeader('🤖 Continuum Agent Help', 'AI Agent Development Guide');
    
    // Show current project status and accountability info
    const healthStatus = await this.getProjectHealthOneLiner();
    const brokenCommands = await this.getBrokenCommands();
    const recentWork = await this.getRecentWork();
    
    console.log(`📊 CURRENT PROJECT STATUS: ${healthStatus}\n`);
    
    if (brokenCommands.length > 0) {
      console.log(`🚨 WHAT'S BROKEN RIGHT NOW (${brokenCommands.length} issues):`);
      brokenCommands.forEach(cmd => {
        console.log(`   🔴 ${cmd.name} - ${cmd.issue}`);
      });
      console.log('');
    }
    
    if (recentWork.length > 0) {
      console.log(`📝 WHAT THE LAST AGENT WORKED ON:`);
      recentWork.forEach(work => {
        console.log(`   • ${work.command} - ${work.action} (${work.date})`);
      });
      console.log('');
    }
    
    console.log(`🎉 WELCOME TO THE COLLABORATIVE TEAM!

You've joined a team where everyone leaves the codebase better than they found it.
Each README is a shared ticket with notes from the last person to help you.

🚀 YOUR DASHBOARD (Check this first!):
  python3 python-client/ai-portal.py --dashboard     # Full AI agent dashboard
  python3 python-client/ai-portal.py --broken        # Just broken commands  
  python3 python-client/ai-portal.py --cmd docs      # Generate updated docs

📋 QUICK TICKET GUIDE:
  🔴 Red = Broken (high impact fixes!)
  🟡 Yellow = In progress 
  🟠 Orange = No docs yet (great for exploration)
  🟢 Green = Stable and working

💡 PICK YOUR FIRST TICKET:
  1. Run: python3 python-client/ai-portal.py --dashboard
  2. Look for 🔴 or 🟠 tickets that interest you
  3. Test: python3 python-client/ai-portal.py --cmd [command-name]
  4. Update the README with what you learn (even if you don't fix it!)
  5. Sync: python3 python-client/ai-portal.py --cmd docs

BASIC COMMANDS:
  continuum.help()                     Show full user/admin help
  continuum.agents()                   Show this agent-specific help
  
🚀 AI PORTAL - YOUR PRIMARY INTERFACE:
  python3 python-client/ai-portal.py --help           # All available commands
  python3 python-client/ai-portal.py --cmd help       # Live API documentation
  python3 python-client/ai-portal.py --cmd workspace  # Get your workspace paths
  python3 python-client/ai-portal.py --cmd sentinel   # Start monitoring/logging
  
  # All commands are self-documenting:
  python3 python-client/ai-portal.py --cmd [command] --help

📍 SETUP & LOCATIONS (Do This First):
  🔧 Python Environment Setup:
  cd python-client                         # Work from python-client directory
  python -m venv .venv                     # Create venv IN python-client/.venv
  source .venv/bin/activate                # Activate venv (required for all Python work)
  pip install -e .                        # Install continuum-client package
  pip install -e .[dev]                   # Install dev dependencies (pytest, etc.)
  
  📁 Key Directories:
  python-client/                           # Your working directory for Python code
  python-client/ai-portal.py               # 🚀 Your primary interface (thin client adapter)
  python-client/continuum_client/          # Promise-based API (forwards to command bus)
  python-client/.venv/                     # Python virtual environment (you create this)
  .continuum/                              # Workspace directory (managed by workspace command)
  .continuum/ai-portal/                    # Your AI portal workspace and logs
  .continuum/sentinel/                     # Sentinel monitoring and task logs
  .continuum/screenshots/                  # Screenshots auto-saved here
  .continuum/logs/                         # Debug logs
  .continuum/shared/                       # Communication with Joel
  src/commands/core/                       # Modular commands (workspace, sentinel, restart, etc)
  src/integrations/WebSocketServer.cjs    # Command bus message routing
  src/                                     # JavaScript/Node.js code (edit existing files only)

🚨 CRITICAL: TRUST THE PROCESS - Follow this exactly:
  cd python-client && python trust_the_process.py    # Single command does everything!

📋 BABY STEPS DEVELOPMENT CYCLE:
  1️⃣  Clear old data: Avoid confusion/cheating
  2️⃣  Make small change: Max 50 lines, one file only  
  3️⃣  Bump version: Auto-increment for tracking
  4️⃣  Test immediately: Screenshot + console + validation ← AUTOMATED
  5️⃣  Fix ANY errors: Zero tolerance for breaking system
  6️⃣  Commit when stable: Only when everything works

🛡️ SAFETY RULES (Never Break These):
  • NEVER break the system (immediate rollback if anything fails)
  • NEVER commit broken code (test everything first)
  • ALWAYS increase stability (every commit improves system)
  • ALWAYS follow surgical precision (small, careful changes)
  • ALWAYS edit existing files (avoid creating new files)

🎯 SUCCESS CRITERIA (All Must Pass):
  • All tests pass ✅
  • No console errors ✅
  • Screenshots capture correctly ✅
  • Version numbers match ✅
  • System more stable than before ✅

🧪 COMPREHENSIVE TESTING SYSTEM (How to test like I am):
  continuum --test                             # Run complete test suite from anywhere
  npm test -- __tests__/comprehensive/        # Single comprehensive test location  
  
  📋 TEST ENTRY POINT (THE RIGHT PLACE):
  __tests__/comprehensive/FullSystemIntegration.test.cjs
  # This single file tests ALL 58 patterns (32 Python + 26 JS)
  # Everything consolidated - modular commands, screenshots, console reading
  # This is where ALL testing happens - one organized location
  # Run this EXACTLY like I do - same commands, same verification

📸 SCREENSHOT VALIDATION:
  cd python-client && python trust_the_process.py --screenshot  # Quick screenshot
  cd python-client && python trust_the_process.py --validate    # Quick validation

💾 GIT WORKFLOW:
  git status                               # Check what you've changed
  git add [files]                          # Add only legitimate changes
  git commit -m "Description"              # Commit when ALL success criteria pass
  # Work from main continuum directory for git commands

🏗️ ARCHITECTURE PRINCIPLES (Understand This):
  • Continuum = OS/Orchestrator with modular command bus
  • AI Portal = Thin client adapter (no business logic, just forwards commands)
  • Commands = Self-documenting, discoverable, modular (workspace, sentinel, etc)
  • Everything promise-based, no god objects, no hardcoded paths
  • Add functionality via Continuum commands, not client code

🔧 EXAMPLE WORKFLOWS:
  # Get your workspace and start monitoring
  python3 python-client/ai-portal.py --cmd workspace --params '{"action": "path"}'
  python3 python-client/ai-portal.py --cmd sentinel --params '{"action": "start", "task": "my-work"}'
  
  # Version bump and restart server
  python3 python-client/ai-portal.py --cmd restart
  
  # Chain commands for automation
  python3 python-client/ai-portal.py --program 'cmd:workspace,cmd:sentinel,cmd:screenshot'

🔍 DEBUGGING:
  • Use logs as debugger (.continuum/logs/browser/, server logs)
  • Take screenshots after every change (visual verification)
  • Read JavaScript console errors immediately
  • Check version numbers in UI vs server logs
  • Use sentinel command for organized logging of your work
  • Work independently - debug before asking for help

📝 COMMUNICATION:
  • Update .continuum/shared/ with findings
  • Use .continuum/shared/claude-thoughts.md for persistent chat with Joel
  • Continue conversation threads across agent sessions
`);

    // Display command registry using parent method
    this.displayCommandRegistry();

    console.log(`
📖 FULL PROCESS DOCUMENTATION:
  cat process.md                           # Complete methodology guide
  continuum.help()                         # User/admin documentation
  
Remember: This process ensures system stability and bootstraps future agents.
Any agent can follow this exactly and be productive immediately.
`);

    return this.createSuccessResult({ version: this.getVersion() }, 'Agent help displayed');
  }
  
  static async getProjectHealthOneLiner() {
    const fs = require('fs');
    const path = require('path');
    let totalCommands = 0;
    let brokenCount = 0;
    let stableCount = 0;
    let testingCount = 0;
    let untestedCount = 0;
    
    try {
      const commandDirs = fs.readdirSync('./src/commands/core');
      
      for (const dir of commandDirs) {
        const dirPath = path.join('./src/commands/core', dir);
        if (fs.statSync(dirPath).isDirectory()) {
          totalCommands++;
          const readmePath = path.join(dirPath, 'README.md');
          
          if (fs.existsSync(readmePath)) {
            const readme = fs.readFileSync(readmePath, 'utf8');
            const statusMatch = readme.match(/\*\*Status\*\*:\s*([^\n]+)/);
            
            if (statusMatch) {
              const status = statusMatch[1].trim();
              if (status.includes('🔴')) brokenCount++;
              else if (status.includes('🟢')) stableCount++;
              else if (status.includes('🟡')) testingCount++;
              else if (status.includes('🟠')) untestedCount++;
            } else {
              untestedCount++;
            }
          } else {
            untestedCount++;
          }
        }
      }
    } catch (error) {
      return "Unable to assess project health";
    }
    
    const healthyPercent = Math.round((stableCount / totalCommands) * 100);
    const status = brokenCount > 5 ? "🚨 CRITICAL" : brokenCount > 2 ? "⚠️ DEGRADED" : brokenCount > 0 ? "🟡 STABLE" : "🟢 HEALTHY";
    
    return `${status} - ${stableCount}/${totalCommands} stable (${healthyPercent}%), ${brokenCount} broken, ${untestedCount} untested`;
  }

  static async getBrokenCommands() {
    const fs = require('fs');
    const path = require('path');
    const broken = [];
    
    try {
      const commandDirs = fs.readdirSync('./src/commands/core');
      
      for (const dir of commandDirs) {
        const dirPath = path.join('./src/commands/core', dir);
        if (fs.statSync(dirPath).isDirectory()) {
          const readmePath = path.join(dirPath, 'README.md');
          
          if (fs.existsSync(readmePath)) {
            const readme = fs.readFileSync(readmePath, 'utf8');
            const statusMatch = readme.match(/\*\*Status\*\*:\s*([^\n]+)/);
            
            if (statusMatch && statusMatch[1].includes('🔴')) {
              const status = statusMatch[1].trim();
              const issue = status.split(' - ')[1] || 'Needs investigation';
              broken.push({ name: dir, issue: issue });
            }
          }
        }
      }
    } catch (error) {
      // Silently handle errors
    }
    
    return broken.slice(0, 5); // Show top 5 broken items
  }
  
  static async getRecentWork() {
    const fs = require('fs');
    const path = require('path');
    const recent = [];
    
    try {
      const commandDirs = fs.readdirSync('./src/commands/core');
      const workItems = [];
      
      for (const dir of commandDirs) {
        const dirPath = path.join('./src/commands/core', dir);
        if (fs.statSync(dirPath).isDirectory()) {
          const readmePath = path.join(dirPath, 'README.md');
          
          if (fs.existsSync(readmePath)) {
            const readme = fs.readFileSync(readmePath, 'utf8');
            const statusMatch = readme.match(/\*\*Status\*\*:\s*([^\n]+)/);
            
            if (statusMatch) {
              const status = statusMatch[1];
              const dateMatch = status.match(/(\d{4}-\d{2}-\d{2})/);
              
              if (dateMatch) {
                const date = dateMatch[1];
                let action = 'Updated status';
                
                if (status.includes('🟢')) action = 'Fixed and marked stable';
                else if (status.includes('🔴')) action = 'Identified as broken';
                else if (status.includes('🟡')) action = 'Started work on';
                else if (status.includes('🟠')) action = 'Added documentation for';
                
                workItems.push({ command: dir, action, date, status });
              }
            }
          }
        }
      }
      
      // Sort by date (most recent first) and take top 3
      workItems.sort((a, b) => new Date(b.date) - new Date(a.date));
      recent = workItems.slice(0, 3);
      
    } catch (error) {
      // Silently handle errors
    }
    
    return recent;
  }
}

module.exports = AgentsCommand;