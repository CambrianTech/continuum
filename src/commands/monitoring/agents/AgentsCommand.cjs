/**
 * AgentsCommand - Modular agent dashboard with strategic guidance
 * Refactored to use focused, testable modules
 */

const InfoCommand = require('../info/InfoCommand.cjs');

class AgentsCommand extends InfoCommand {
  constructor() {
    super();
  }

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
            description: 'Agent help section: overview, roadmap, broken, restoration',
            default: 'overview'
          },
          filter: {
            type: 'string',
            required: false,
            description: 'Filter roadmap items: risk, complexity, impact, category',
            default: 'all'
          },
          sort: {
            type: 'string',
            required: false,
            description: 'Sort by: dependency, risk, impact, timeline',
            default: 'dependency'
          }
        },
        examples: [
          'agents',
          'agents --section roadmap',
          'agents --section roadmap --filter risk --sort dependency',
          'agents --section broken',
          'agents --section restoration'
        ]
      };
    }
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    const section = options.section || 'overview';
    const filter = options.filter || 'all';
    const sort = options.sort || 'dependency';
    
    this.displayHeader('🤖 Continuum Agent Help', 'AI Agent Development Guide');
    
    // Route to specific sections using new planning commands
    if (section === 'roadmap') {
      return await this.displayRoadmapSection(filter, sort, continuum);
    } else if (section === 'broken') {
      return await this.displayBrokenSection(continuum);
    } else if (section === 'restoration') {
      return await this.displayRestorationSection(continuum);
    }
    
    // Default overview section
    return await this.displayOverviewSection(continuum);
  }

  static async displayOverviewSection(continuum) {
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

🚀 YOUR STRATEGIC DASHBOARD:
  python3 python-client/ai-portal.py --cmd agents --section roadmap    # 🗺️ Strategic roadmap with filters
  python3 python-client/ai-portal.py --cmd agents --section broken     # 🚨 Broken commands by dependency impact  
  python3 python-client/ai-portal.py --cmd agents --section restoration # 🏛️ Archaeological restoration guide
  python3 python-client/ai-portal.py --dashboard                       # 📊 Full system health dashboard

🏛️ RESTORATION OPPORTUNITIES (High Impact!):
  cat RESTORATION-STRATEGY.md                        # 🔥 CRITICAL: Complete restoration plan
  cat FILES.md | grep -A 20 "Archaeological"         # Lost treasures map
  cat README.md | grep -A 10 "Recovery Commands"     # Quick recovery commands
  cat docs/ACADEMY_ARCHITECTURE.md                   # Academy system details

📋 DEVELOPMENT PATHS:
  🔴 Red = Broken (high impact fixes!)
  🟡 Yellow = In progress 
  🟠 Orange = No docs yet (great for exploration)
  🟢 Green = Stable and working
  🏛️ Archaeological = Sophisticated capabilities ready for restoration

💡 STRATEGIC APPROACHES:
  🔧 QUICK FIXES: Use --section broken --sort dependency for priority order
  🏛️ RESTORATION: Use --section restoration for phase-by-phase guide
  🗺️ ROADMAP: Use --section roadmap --filter impact --sort dependency for strategic planning
  🎯 FOCUSED: Use --section roadmap --filter complexity (low-risk wins)
  🔥 CRITICAL: Use --section roadmap --filter category (critical fixes first)
  
🎯 STRATEGIC WORKFLOW:
  1. ASSESS: python3 python-client/ai-portal.py --cmd agents --section roadmap
  2. CHOOSE: Pick item based on risk/complexity/impact analysis
  3. VALIDATE: python3 python-client/trust_the_process.py (BEFORE changes)
  4. EXECUTE: Follow the provided commands and guidance
  5. TEST: python3 python-client/trust_the_process.py (AFTER changes)
  6. UPDATE: Document discoveries and update roadmap status

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
  src/                                     # JavaScript/Node.js code (edit existing files only)`);

    // Display command registry using parent method
    this.displayCommandRegistry();

    console.log(`
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

🏛️ ARCHAEOLOGICAL OPPORTUNITIES (HUGE IMPACT!):
  🎓 ACADEMY SYSTEM: Matrix-inspired adversarial AI training (f0e2fb9)
     • TestingDroid vs ProtocolSheriff battles
     • LoRA fine-tuning with 190,735x storage reduction
     • Boot camp graduation and deployment
     
  🎮 MASS EFFECT UI: Cyberpunk slideout panels (4ffb32e, 41c02a2)
     • Glass morphism: rgba(0, 255, 136, 0.15)
     • Multi-agent selection with avatars
     • Slideout panels with >> arrow interactions
     
  🤖 INTELLIGENT ROUTING: Self-improving agent selection (72c5684)
     • Smart routing optimization
     • Process lifecycle management
     • Multi-agent session coordination
     
  📖 READ THIS: RESTORATION-STRATEGY.md - Complete step-by-step plan
     • 5-phase restoration with exact git commands
     • Safety-first methodology with rollback procedures
     • Archaeological recovery instead of recreation

📖 FULL PROCESS DOCUMENTATION:
  cat RESTORATION-STRATEGY.md              # 🔥 PRIMARY: Complete restoration plan
  cat FILES.md                             # Archaeological map with Agent Study Guide
  cat README.md                            # System overview and quick start
  cat docs/ACADEMY_ARCHITECTURE.md         # Academy system technical details
  continuum.help()                         # User/admin documentation
  
🎯 PRIORITY READING:
  1. RESTORATION-STRATEGY.md - Your roadmap to high-impact work
  2. FILES.md Agent Study Guide - Archaeological discoveries
  3. Trust the process: python python-client/trust_the_process.py
  
Remember: Follow the restoration strategy for maximum impact.
Archaeological recovery beats recreating from scratch.`);

    return this.createSuccessResult({ section: 'overview' }, 'Agent overview displayed');
  }

  static async displayRoadmapSection(filter, sort, continuum) {
    console.log(`🗺️ STRATEGIC ROADMAP - Filtered by: ${filter}, Sorted by: ${sort}\n`);
    
    // Use the new roadmap command
    const RoadmapCommand = require('../../planning/roadmap/RoadmapCommand.cjs');
    const roadmapResult = await RoadmapCommand.execute(`--action list --filter ${filter} --format table`, continuum);
    
    // Use the new analyze command for strategic analysis
    const AnalyzeCommand = require('../../planning/analyze/AnalyzeCommand.cjs');
    const analysisResult = await AnalyzeCommand.execute(`--target roadmap --filter ${filter} --format summary`, continuum);
    
    console.log(roadmapResult.message || roadmapResult);
    console.log(analysisResult.message || analysisResult);
    
    return this.createSuccessResult({ section: 'roadmap', filter, sort }, 'Roadmap section displayed');
  }

  static async displayBrokenSection(continuum) {
    console.log(`🚨 BROKEN COMMANDS - Strategic Fix Order\n`);
    
    // Use the new analyze command for broken analysis
    const AnalyzeCommand = require('../../planning/analyze/AnalyzeCommand.cjs');
    const analysisResult = await AnalyzeCommand.execute('--target codebase --format summary', continuum);
    console.log(analysisResult.message || analysisResult);
    
    return this.createSuccessResult({ section: 'broken' }, 'Broken commands analysis complete');
  }

  static async displayRestorationSection(continuum) {
    console.log(`🏛️ ARCHAEOLOGICAL RESTORATION - Safe Recovery Strategy\n`);
    
    // Use the new restore command
    const RestoreCommand = require('../../planning/restore/RestoreCommand.cjs');
    const restoreResult = await RestoreCommand.execute('--action list --format table', continuum);
    console.log(restoreResult.message || restoreResult);
    
    return this.createSuccessResult({ section: 'restoration' }, 'Restoration section displayed');
  }

  // Essential helper methods that were missing
  static async getProjectHealthOneLiner() {
    try {
      // Simple health check - just return basic status
      return "System loading, commands available";
    } catch (error) {
      return "Status check unavailable";
    }
  }

  static async getBrokenCommands() {
    // Return empty array for now - could be enhanced later
    return [];
  }

  static async getRecentWork() {
    // Return empty array for now - could be enhanced later  
    return [];
  }
}

module.exports = AgentsCommand;
