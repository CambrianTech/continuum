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
  6. UPDATE: Document discoveries and update roadmap status`);

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
}

module.exports = AgentsCommand;
