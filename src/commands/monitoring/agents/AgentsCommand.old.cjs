/**
 * AgentsCommand - Show agent-specific development help and guidance
 */

const InfoCommand = require('../../core/info/InfoCommand.cjs');

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
    
    // Route to specific sections
    if (section === 'roadmap') {
      return await this.displayRoadmapSection(filter, sort, continuum);
    } else if (section === 'broken') {
      return await this.displayBrokenSection(continuum);
    } else if (section === 'restoration') {
      return await this.displayRestorationSection(continuum);
    }
    
    // Default overview section
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
`);

    // Display command registry using parent method
    this.displayCommandRegistry();

    console.log(`
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
Archaeological recovery beats recreating from scratch.
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

  static async displayRoadmapSection(filter, sort, continuum) {
    console.log(`🗺️ STRATEGIC ROADMAP - Filtered by: ${filter}, Sorted by: ${sort}\n`);
    
    const roadmapItems = await this.getRoadmapItems();
    const filteredItems = this.filterRoadmapItems(roadmapItems, filter);
    const sortedItems = this.sortRoadmapItems(filteredItems, sort);
    
    console.log(`📊 Showing ${sortedItems.length} roadmap items (${roadmapItems.length} total)\n`);
    
    // Display strategic guidance
    this.displayStrategicGuidance();
    
    // Display roadmap items by category
    const categories = this.groupByCategory(sortedItems);
    
    for (const [category, items] of Object.entries(categories)) {
      console.log(`\n🎯 ${category.toUpperCase()}:`);
      items.forEach(item => {
        const riskColor = this.getRiskColor(item.risk);
        const complexity = this.getComplexityIcon(item.complexity);
        const impact = this.getImpactIcon(item.impact);
        
        console.log(`   ${riskColor} ${item.title}`);
        console.log(`     ${complexity} Complexity: ${item.complexity} | ${impact} Impact: ${item.impact} | ⏱️ ${item.timeline}`);
        
        if (item.dependencies && item.dependencies.length > 0) {
          console.log(`     🔗 Requires: ${item.dependencies.join(', ')}`);
        }
        
        if (item.commands && item.commands.length > 0) {
          console.log(`     💻 Commands: ${item.commands.join(', ')}`);
        }
        
        console.log(`     📋 ${item.description}`);
        console.log('');
      });
    }
    
    // Show next recommended action
    this.displayNextAction(sortedItems);
    
    return this.createSuccessResult({ 
      section: 'roadmap', 
      filter, 
      sort, 
      itemsShown: sortedItems.length,
      totalItems: roadmapItems.length
    }, 'Roadmap displayed successfully');
  }

  static async displayBrokenSection(continuum) {
    console.log(`🚨 BROKEN COMMANDS - Strategic Fix Order\n`);
    
    const brokenCommands = await this.getBrokenCommands();
    const sortedByPriority = this.sortByDependencyPriority(brokenCommands);
    
    console.log(`📊 ${brokenCommands.length} broken commands found\n`);
    
    if (brokenCommands.length === 0) {
      console.log(`🎉 No broken commands found! System is healthy.`);
      return this.createSuccessResult({ section: 'broken', brokenCount: 0 }, 'No broken commands');
    }
    
    console.log(`🎯 RECOMMENDED FIX ORDER (by dependency impact):\n`);
    
    sortedByPriority.forEach((cmd, index) => {
      const urgency = this.getUrgencyLevel(cmd);
      console.log(`${index + 1}. ${urgency} ${cmd.name}`);
      console.log(`   📋 Issue: ${cmd.issue}`);
      console.log(`   🔗 Blocks: ${cmd.blockedCommands ? cmd.blockedCommands.join(', ') : 'none'}`);
      console.log(`   💻 Test: python3 python-client/ai-portal.py --cmd ${cmd.name}`);
      console.log('');
    });
    
    // Show debugging strategy
    console.log(`🔧 DEBUGGING STRATEGY:\n`);
    console.log(`1. Start with highest dependency impact (${sortedByPriority[0]?.name || 'none'})`);
    console.log(`2. Use: python3 python-client/trust_the_process.py --debug`);
    console.log(`3. Check logs: .continuum/logs/`);
    console.log(`4. Validate: Run dashboard after each fix`);
    
    return this.createSuccessResult({ 
      section: 'broken', 
      brokenCount: brokenCommands.length 
    }, 'Broken commands analysis complete');
  }

  static async displayRestorationSection(continuum) {
    console.log(`🏛️ ARCHAEOLOGICAL RESTORATION - Safe Recovery Strategy\n`);
    
    console.log(`📋 RESTORATION STRATEGY:\n`);
    console.log(`🔥 PRIMARY RESOURCE: RESTORATION-STRATEGY.md`);
    console.log(`   cat RESTORATION-STRATEGY.md | grep -A 5 "Phase 1"`);
    console.log(`   cat RESTORATION-STRATEGY.md | grep -A 10 "Safety-First"`);
    console.log('');
    
    // Phase-by-phase breakdown with risk assessment
    const restorationPhases = this.getRestorationPhases();
    
    restorationPhases.forEach(phase => {
      const riskColor = this.getRiskColor(phase.risk);
      console.log(`${riskColor} ${phase.name} (${phase.timeline}, ${phase.risk} risk)`);
      console.log(`   📋 ${phase.description}`);
      console.log(`   💻 ${phase.startCommand}`);
      console.log(`   ✅ Validation: ${phase.validation}`);
      console.log('');
    });
    
    console.log(`🛡️ SAFETY PROTOCOLS:\n`);
    console.log(`1. ALWAYS validate with: python3 python-client/trust_the_process.py`);
    console.log(`2. Check git status before any restoration`);
    console.log(`3. Use rollback commands if validation fails`);
    console.log(`4. ONE PHASE AT A TIME - never skip phases`);
    console.log('');
    
    console.log(`🏆 ARCHAEOLOGICAL DISCOVERIES ACTIVE:\n`);
    console.log(`✅ Hierarchical LoRA System - FULLY FUNCTIONAL`);
    console.log(`✅ Global Competition Network - INFRASTRUCTURE READY`);
    console.log(`✅ Academy Scoring System - WORKING`);
    console.log(`🔄 Mass Effect UI - Git recoverable (commits 4ffb32e, 41c02a2)`);
    console.log(`🔄 Academy Training - Git recoverable (commit f0e2fb9)`);
    console.log(`🔄 Intelligent Routing - Git recoverable (commit 72c5684)`);
    
    return this.createSuccessResult({ 
      section: 'restoration' 
    }, 'Restoration strategy displayed');
  }

  static displayStrategicGuidance() {
    console.log(`🎯 STRATEGIC GUIDANCE:\n`);
    console.log(`🟢 SAFE BETS: Start with low-risk, high-impact items`);
    console.log(`🟡 BUILD UP: Complete dependencies before dependent items`);
    console.log(`🔴 HIGH RISK: Save for when you have more context`);
    console.log(`🛡️ VALIDATION: Always run trust_the_process.py after changes`);
    console.log('');
  }

  static displayNextAction(sortedItems) {
    if (sortedItems.length === 0) return;
    
    const nextItem = sortedItems[0];
    console.log(`🚀 RECOMMENDED NEXT ACTION:\n`);
    console.log(`📋 ${nextItem.title}`);
    console.log(`🎯 Why: ${nextItem.justification || 'Highest priority based on current filters'}`);
    
    if (nextItem.commands && nextItem.commands.length > 0) {
      console.log(`💻 Start with: ${nextItem.commands[0]}`);
    }
    
    console.log('');
  }

  // Roadmap data management methods
  static async getRoadmapItems() {
    // Parse ROADMAP.md to extract actionable items
    const fs = require('fs');
    const path = require('path');
    
    try {
      const roadmapPath = path.join(process.cwd(), 'ROADMAP.md');
      const roadmap = fs.readFileSync(roadmapPath, 'utf8');
      
      return this.parseRoadmapMarkdown(roadmap);
    } catch (error) {
      console.log('⚠️ Could not read ROADMAP.md, using default items');
      return this.getDefaultRoadmapItems();
    }
  }

  static parseRoadmapMarkdown(roadmap) {
    const items = [];
    const lines = roadmap.split('\n');
    let currentSection = 'General';
    
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();
      
      // Track sections
      if (line.startsWith('## ') || line.startsWith('### ')) {
        currentSection = line.replace(/^#+\s*/, '').replace(/\*\*.*?\*\*/, '').trim();
        continue;
      }
      
      // Parse todo items
      if (line.match(/^-\s*\[\s*\]\s*/)) {
        const title = line.replace(/^-\s*\[\s*\]\s*/, '').replace(/\*\*.*?\*\*/, '').trim();
        
        // Extract metadata from the line
        const complexity = this.extractComplexity(line);
        const risk = this.extractRisk(currentSection);
        const impact = this.extractImpact(title);
        const timeline = this.extractTimeline(currentSection);
        
        items.push({
          title,
          description: title,
          category: currentSection,
          status: 'pending',
          complexity,
          risk,
          impact,
          timeline,
          dependencies: this.extractDependencies(title),
          commands: this.extractCommands(title)
        });
      }
    }
    
    return items;
  }

  static getDefaultRoadmapItems() {
    return [
      {
        title: 'Fix broken spawn command',
        description: 'exec command does not actually execute, blocks agent observation workflow',
        category: 'Critical Fixes',
        status: 'pending',
        complexity: 'Medium',
        risk: 'Low',
        impact: 'High',
        timeline: '2-4 hours',
        dependencies: [],
        commands: ['spawn'],
        justification: 'Blocks automation workflow'
      },
      {
        title: 'Restore Mass Effect UI components',
        description: 'Recover slideout panels and agent selection interface',
        category: 'UI Restoration',
        status: 'pending',
        complexity: 'Low',
        risk: 'Low',
        impact: 'High',
        timeline: '2-4 hours',
        dependencies: [],
        commands: ['git show 4ffb32e:src/ui/components/AgentSelector.js'],
        justification: 'High visual impact, low risk, git recoverable'
      },
      {
        title: 'Restore Academy adversarial training',
        description: 'Recover TestingDroid vs ProtocolSheriff system',
        category: 'Academy Restoration',
        status: 'pending',
        complexity: 'High',
        risk: 'Medium',
        impact: 'High',
        timeline: '4-8 hours',
        dependencies: ['Mass Effect UI'],
        commands: ['git show f0e2fb9:src/core/Academy.cjs'],
        justification: 'Core platform capability'
      }
    ];
  }

  // Utility methods for categorization and sorting
  static filterRoadmapItems(items, filter) {
    if (filter === 'all') return items;
    
    switch (filter) {
      case 'risk':
        return items.filter(item => item.risk === 'High');
      case 'complexity':
        return items.filter(item => item.complexity === 'Low');
      case 'impact':
        return items.filter(item => item.impact === 'High');
      case 'category':
        return items.filter(item => item.category.includes('Critical'));
      default:
        return items;
    }
  }

  static sortRoadmapItems(items, sort) {
    switch (sort) {
      case 'dependency':
        return this.sortByDependencies(items);
      case 'risk':
        return items.sort((a, b) => this.getRiskValue(a.risk) - this.getRiskValue(b.risk));
      case 'impact':
        return items.sort((a, b) => this.getImpactValue(b.impact) - this.getImpactValue(a.impact));
      case 'timeline':
        return items.sort((a, b) => this.getTimelineValue(a.timeline) - this.getTimelineValue(b.timeline));
      default:
        return items;
    }
  }

  static sortByDependencies(items) {
    // Topological sort - items with no dependencies first
    const sorted = [];
    const remaining = [...items];
    
    while (remaining.length > 0) {
      const nextItem = remaining.find(item => 
        !item.dependencies || 
        item.dependencies.length === 0 ||
        item.dependencies.every(dep => 
          sorted.some(sortedItem => sortedItem.title.toLowerCase().includes(dep.toLowerCase()))
        )
      );
      
      if (nextItem) {
        sorted.push(nextItem);
        remaining.splice(remaining.indexOf(nextItem), 1);
      } else {
        // Circular dependency or missing dependency - add remaining items
        sorted.push(...remaining);
        break;
      }
    }
    
    return sorted;
  }

  static groupByCategory(items) {
    const groups = {};
    items.forEach(item => {
      if (!groups[item.category]) {
        groups[item.category] = [];
      }
      groups[item.category].push(item);
    });
    return groups;
  }

  // Visual indicators and helpers
  static getRiskColor(risk) {
    switch (risk) {
      case 'Low': return '🟢';
      case 'Medium': return '🟡';
      case 'High': return '🔴';
      default: return '⚪';
    }
  }

  static getComplexityIcon(complexity) {
    switch (complexity) {
      case 'Low': return '🚀';
      case 'Medium': return '⚙️';
      case 'High': return '🧩';
      default: return '❓';
    }
  }

  static getImpactIcon(impact) {
    switch (impact) {
      case 'Low': return '📝';
      case 'Medium': return '⚡';
      case 'High': return '💥';
      default: return '❓';
    }
  }

  static getUrgencyLevel(cmd) {
    if (cmd.blockedCommands && cmd.blockedCommands.length > 3) return '🚨';
    if (cmd.blockedCommands && cmd.blockedCommands.length > 1) return '⚠️';
    return '🔴';
  }

  // Value mapping for sorting
  static getRiskValue(risk) {
    switch (risk) {
      case 'Low': return 1;
      case 'Medium': return 2;
      case 'High': return 3;
      default: return 2;
    }
  }

  static getImpactValue(impact) {
    switch (impact) {
      case 'Low': return 1;
      case 'Medium': return 2;
      case 'High': return 3;
      default: return 2;
    }
  }

  static getTimelineValue(timeline) {
    if (timeline.includes('hour')) {
      const hours = parseInt(timeline.match(/(\d+)/)?.[1] || '4');
      return hours;
    }
    if (timeline.includes('day')) {
      const days = parseInt(timeline.match(/(\d+)/)?.[1] || '3');
      return days * 24;
    }
    if (timeline.includes('week')) {
      const weeks = parseInt(timeline.match(/(\d+)/)?.[1] || '2');
      return weeks * 7 * 24;
    }
    return 24; // Default to 1 day
  }

  // Extraction helpers for parsing roadmap
  static extractComplexity(line) {
    if (line.includes('Low') || line.includes('🟢') || line.includes('simple')) return 'Low';
    if (line.includes('High') || line.includes('🔴') || line.includes('complex')) return 'High';
    return 'Medium';
  }

  static extractRisk(section) {
    if (section.includes('Critical') || section.includes('Fix')) return 'Low';
    if (section.includes('Restoration') || section.includes('Academy')) return 'Medium';
    if (section.includes('Advanced') || section.includes('Ecosystem')) return 'High';
    return 'Medium';
  }

  static extractImpact(title) {
    if (title.includes('broken') || title.includes('critical') || title.includes('blocks')) return 'High';
    if (title.includes('enhance') || title.includes('improve') || title.includes('add')) return 'Medium';
    return 'Low';
  }

  static extractTimeline(section) {
    if (section.includes('Phase 1') || section.includes('Quick')) return '2-4 hours';
    if (section.includes('Phase 2') || section.includes('Academy')) return '4-8 hours';
    if (section.includes('Phase 3') || section.includes('Advanced')) return '1-2 days';
    return '4-8 hours';
  }

  static extractDependencies(title) {
    const deps = [];
    if (title.toLowerCase().includes('ui') && title.toLowerCase().includes('academy')) {
      deps.push('Mass Effect UI');
    }
    if (title.toLowerCase().includes('integration') || title.toLowerCase().includes('connect')) {
      deps.push('Core Components');
    }
    return deps;
  }

  static extractCommands(title) {
    const commands = [];
    if (title.includes('git show')) {
      const match = title.match(/git show [a-f0-9]+:[^\s]+/);
      if (match) commands.push(match[0]);
    }
    if (title.toLowerCase().includes('spawn')) commands.push('spawn');
    if (title.toLowerCase().includes('academy')) commands.push('academy');
    if (title.toLowerCase().includes('screenshot')) commands.push('screenshot');
    return commands;
  }

  static getRestorationPhases() {
    return [
      {
        name: 'Phase 1: UI Renaissance',
        description: 'Restore Mass Effect-style slideout panels and agent selection',
        timeline: '2-4 hours',
        risk: 'Low',
        startCommand: 'git show 4ffb32e:src/ui/components/AgentSelector.js > src/ui/components/AgentSelector.js',
        validation: 'python3 python-client/trust_the_process.py --screenshot'
      },
      {
        name: 'Phase 2: Academy Resurrection', 
        description: 'Restore Matrix-inspired adversarial AI training system',
        timeline: '4-8 hours',
        risk: 'Medium',
        startCommand: 'git show f0e2fb9:src/core/Academy.cjs > src/core/Academy.cjs',
        validation: 'python3 python-client/trust_the_process.py --academy'
      },
      {
        name: 'Phase 3: Routing Revival',
        description: 'Restore intelligent agent selection and process management',
        timeline: '3-6 hours', 
        risk: 'Medium',
        startCommand: 'git show 72c5684:src/core/intelligent-routing.cjs > src/core/intelligent-routing.cjs',
        validation: 'python3 python-client/trust_the_process.py --routing'
      }
    ];
  }

  static sortByDependencyPriority(brokenCommands) {
    // Sort broken commands by how many other commands they block
    return brokenCommands.sort((a, b) => {
      const aBlocks = (a.blockedCommands || []).length;
      const bBlocks = (b.blockedCommands || []).length;
      return bBlocks - aBlocks; // Higher blocking count first
    });
  }
}

module.exports = AgentsCommand;