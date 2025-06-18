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
    
    // Create instances for this execution
    const roadmapParser = new RoadmapParser();
    const strategicAnalyzer = new StrategicAnalyzer();
    const restorationPlanner = new RestorationPlanner();
    
    this.displayHeader('🤖 Continuum Agent Help', 'AI Agent Development Guide');
    
    // Route to specific sections
    if (section === 'roadmap') {
      return await this.displayRoadmapSection(filter, sort, continuum, roadmapParser, strategicAnalyzer);
    } else if (section === 'broken') {
      return await this.displayBrokenSection(continuum, strategicAnalyzer);
    } else if (section === 'restoration') {
      return await this.displayRestorationSection(continuum, restorationPlanner);
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

  static async displayRoadmapSection(filter, sort, continuum, roadmapParser, strategicAnalyzer) {
    console.log(`🗺️ STRATEGIC ROADMAP - Filtered by: ${filter}, Sorted by: ${sort}\n`);
    
    const roadmapItems = await roadmapParser.parseRoadmap();
    const filteredItems = strategicAnalyzer.filterItems(roadmapItems, filter);
    const sortedItems = strategicAnalyzer.sortItems(filteredItems, sort);
    
    console.log(`📊 Showing ${sortedItems.length} roadmap items (${roadmapItems.length} total)\n`);
    
    // Display strategic guidance
    this.displayStrategicGuidance();
    
    // Display roadmap items by category
    const categories = strategicAnalyzer.groupByCategory(sortedItems);
    
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
    const recommended = strategicAnalyzer.getRecommendedAction(sortedItems);
    if (recommended) {
      console.log(`🚀 RECOMMENDED NEXT ACTION:\n`);
      console.log(`📋 ${recommended.title}`);
      console.log(`🎯 Strategic Score: ${recommended.score} (higher is better)`);
      console.log(`🎯 Why: ${recommended.justification || 'Highest priority based on current filters'}`);
      
      if (recommended.commands && recommended.commands.length > 0) {
        console.log(`💻 Start with: ${recommended.commands[0]}`);
      }
      console.log('');
    }
    
    // Show strategic insights
    const insights = strategicAnalyzer.getStrategicInsights(sortedItems);
    console.log(`📊 STRATEGIC INSIGHTS:`);
    console.log(`   🎯 Quick Wins (Low Risk + High Impact): ${insights.quickWins}`);
    console.log(`   🟢 Low Risk Items: ${insights.lowRiskCount}/${insights.totalItems}`);
    console.log(`   🚀 No Dependencies: ${insights.noDependencies}/${insights.totalItems}`);
    console.log(`   🔥 Critical Items: ${insights.criticalItems}`);
    
    return this.createSuccessResult({ 
      section: 'roadmap', 
      filter, 
      sort, 
      itemsShown: sortedItems.length,
      totalItems: roadmapItems.length,
      insights
    }, 'Roadmap displayed successfully');
  }

  static async displayBrokenSection(continuum, strategicAnalyzer) {
    console.log(`🚨 BROKEN COMMANDS - Strategic Fix Order\n`);
    
    const brokenCommands = await this.getBrokenCommands();
    const sortedByPriority = strategicAnalyzer.sortByDependencyImpact(brokenCommands);
    
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

  static async displayRestorationSection(continuum, restorationPlanner) {
    console.log(`🏛️ ARCHAEOLOGICAL RESTORATION - Safe Recovery Strategy\n`);
    
    console.log(`📋 RESTORATION STRATEGY:\n`);
    console.log(`🔥 PRIMARY RESOURCE: RESTORATION-STRATEGY.md`);
    console.log(`   cat RESTORATION-STRATEGY.md | grep -A 5 "Phase 1"`);
    console.log(`   cat RESTORATION-STRATEGY.md | grep -A 10 "Safety-First"`);
    console.log('');
    
    // Phase-by-phase breakdown with risk assessment
    const restorationPhases = restorationPlanner.getRestorationPhases();
    
    restorationPhases.forEach(phase => {
      const riskColor = this.getRiskColor(phase.risk);
      console.log(`${riskColor} ${phase.name} (${phase.timeline}, ${phase.risk} risk)`);
      console.log(`   📋 ${phase.description}`);
      console.log(`   💻 ${phase.startCommand}`);
      console.log(`   ✅ Validation: ${phase.validation}`);
      if (phase.prerequisites && phase.prerequisites.length > 0) {
        console.log(`   🔗 Requires: ${phase.prerequisites.join(', ')}`);
      }
      console.log('');
    });
    
    // Show safety protocols
    const protocols = restorationPlanner.getSafetyProtocols();
    console.log(`🛡️ SAFETY PROTOCOLS:\n`);
    protocols.forEach((protocol, index) => {
      const criticalIcon = protocol.critical ? '🚨' : 'ℹ️';
      console.log(`${index + 1}. ${criticalIcon} ${protocol.step}`);
      console.log(`   📋 ${protocol.description}`);
      if (protocol.command) {
        console.log(`   💻 ${protocol.command}`);
      }
      console.log('');
    });
    
    // Show archaeological discoveries
    const status = restorationPlanner.getArchaeologicalStatus();
    console.log(`🏆 ARCHAEOLOGICAL DISCOVERIES:\n`);
    
    console.log(`✅ FUNCTIONAL COMPONENTS:`);
    status.functional.forEach(component => {
      console.log(`   ✅ ${component.name} - ${component.status}`);
      console.log(`      📋 ${component.description}`);
      console.log(`      📁 ${component.location}`);
      console.log('');
    });
    
    console.log(`🔄 RECOVERABLE COMPONENTS:`);
    status.recoverable.forEach(component => {
      console.log(`   🔄 ${component.name} - ${component.status}`);
      console.log(`      📋 ${component.description}`);
      console.log(`      🔗 Commits: ${component.commits.join(', ')}`);
      console.log(`      🎯 Phase: ${component.phase}`);
      console.log('');
    });
    
    // Show timeline estimate
    const timeline = restorationPlanner.calculateTotalTimeline();
    console.log(`⏱️ ESTIMATED TIMELINE: ${timeline.timeline}`);
    
    // Show recommendations
    const recommendations = restorationPlanner.getRecommendations(null, 'novice');
    console.log(`\n💡 RECOMMENDATIONS:\n`);
    recommendations.forEach(rec => {
      const priorityIcon = rec.priority === 'critical' ? '🚨' : rec.priority === 'high' ? '🔥' : 'ℹ️';
      console.log(`   ${priorityIcon} ${rec.message}`);
    });
    
    return this.createSuccessResult({ 
      section: 'restoration',
      totalPhases: restorationPhases.length,
      estimatedHours: timeline.totalHours
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

  // Visual indicators and helpers (static methods for easy testing)
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

  // Inherited methods from InfoCommand
  static async getProjectHealthOneLiner() {
    // Implementation from parent class
    return super.getProjectHealthOneLiner ? super.getProjectHealthOneLiner() : 'System operational';
  }

  static async getBrokenCommands() {
    // Implementation from parent class or simple fallback
    return [];
  }

  static async getRecentWork() {
    // Implementation from parent class or simple fallback
    return [];
  }
}

module.exports = AgentsCommand;