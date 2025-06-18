/**
 * Roadmap Command - Parse and analyze project roadmap
 * Self-contained command for roadmap management and analysis
 */

const BaseCommand = require('../../BaseCommand.cjs');
const fs = require('fs');
const path = require('path');

class RoadmapCommand extends BaseCommand {
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
        name: 'roadmap',
        description: 'Parse and analyze project roadmap items',
        icon: '🗺️',
        category: 'planning',
        parameters: {
          action: { type: 'string', required: false, description: 'Action: list, analyze, filter, status' },
          filter: { type: 'string', required: false, description: 'Filter by: high-impact, low-complexity, ready' },
          format: { type: 'string', required: false, description: 'Output format: json, table, summary' },
          file: { type: 'string', required: false, description: 'Custom roadmap file path' }
        },
        examples: [
          'roadmap',
          'roadmap --action list --format json',
          'roadmap --filter high-impact',
          'roadmap --action status'
        ]
      };
    }
  }

  static async execute(params, continuum) {
    try {
      const parsedParams = this.parseParams(params);
      const action = parsedParams.action || 'list';
      const format = parsedParams.format || 'table';
      const filter = parsedParams.filter || 'all';
      const file = parsedParams.file || path.join(process.cwd(), 'ROADMAP.md');

      const parser = new RoadmapCommand();
      parser.roadmapPath = file;

      switch (action) {
        case 'list':
          return await parser.listItems(filter, format);
        case 'analyze':
          return await parser.analyzeRoadmap(format);
        case 'filter':
          return await parser.filterItems(filter, format);
        case 'status':
          return await parser.getRoadmapStatus(format);
        default:
          return parser.formatError(`Unknown action: ${action}. Use list, analyze, filter, or status.`);
      }
    } catch (error) {
      return this.formatError(`Roadmap command failed: ${error.message}`);
    }
  }

  constructor(roadmapPath = null) {
    super();
    this.roadmapPath = roadmapPath || path.join(process.cwd(), 'ROADMAP.md');
  }

  /**
   * Parse ROADMAP.md and extract actionable items
   */
  async parseRoadmap() {
    try {
      const roadmap = fs.readFileSync(this.roadmapPath, 'utf8');
      return this.parseMarkdownContent(roadmap);
    } catch (error) {
      console.log('⚠️ Could not read ROADMAP.md, using default items');
      return this.getDefaultItems();
    }
  }

  /**
   * Parse markdown content into structured roadmap items
   */
  parseMarkdownContent(roadmap) {
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
        const item = this.parseTodoItem(line, currentSection);
        if (item) {
          items.push(item);
        }
      }
    }
    
    return items;
  }

  /**
   * Parse individual todo item line
   */
  parseTodoItem(line, section) {
    const title = line.replace(/^-\s*\[\s*\]\s*/, '').replace(/\*\*.*?\*\*/, '').trim();
    
    if (!title) return null;

    return {
      title,
      description: title,
      category: section,
      status: 'pending',
      complexity: this.extractComplexity(line, title),
      risk: this.extractRisk(section),
      impact: this.extractImpact(title),
      timeline: this.extractTimeline(section),
      dependencies: this.extractDependencies(title),
      commands: this.extractCommands(title)
    };
  }

  /**
   * Extract complexity from line content
   */
  extractComplexity(line, title) {
    // Check for explicit markers
    if (line.includes('Low') || line.includes('🟢') || line.includes('simple')) return 'Low';
    if (line.includes('High') || line.includes('🔴') || line.includes('complex')) return 'High';
    
    // Infer from content
    if (title.includes('fix') || title.includes('restore') || title.includes('git show')) return 'Low';
    if (title.includes('implement') || title.includes('create') || title.includes('build')) return 'High';
    
    return 'Medium';
  }

  /**
   * Extract risk level from section context
   */
  extractRisk(section) {
    const lowerSection = section.toLowerCase();
    
    if (lowerSection.includes('critical') || lowerSection.includes('fix')) return 'Low';
    if (lowerSection.includes('restoration') || lowerSection.includes('academy')) return 'Medium';
    if (lowerSection.includes('advanced') || lowerSection.includes('ecosystem')) return 'High';
    
    return 'Medium';
  }

  /**
   * Extract impact from title content
   */
  extractImpact(title) {
    const lowerTitle = title.toLowerCase();
    
    if (lowerTitle.includes('broken') || lowerTitle.includes('critical') || lowerTitle.includes('blocks')) return 'High';
    if (lowerTitle.includes('enhance') || lowerTitle.includes('improve') || lowerTitle.includes('add')) return 'Medium';
    
    return 'Low';
  }

  /**
   * Extract timeline from section context
   */
  extractTimeline(section) {
    const lowerSection = section.toLowerCase();
    
    if (lowerSection.includes('phase 1') || lowerSection.includes('quick')) return '2-4 hours';
    if (lowerSection.includes('phase 2') || lowerSection.includes('academy')) return '4-8 hours';
    if (lowerSection.includes('phase 3') || lowerSection.includes('advanced')) return '1-2 days';
    
    return '4-8 hours';
  }

  /**
   * Extract dependencies from title
   */
  extractDependencies(title) {
    const deps = [];
    const lowerTitle = title.toLowerCase();
    
    if (lowerTitle.includes('ui') && lowerTitle.includes('academy')) {
      deps.push('Mass Effect UI');
    }
    if (lowerTitle.includes('integration') || lowerTitle.includes('connect')) {
      deps.push('Core Components');
    }
    
    return deps;
  }

  /**
   * Extract actionable commands from title
   */
  extractCommands(title) {
    const commands = [];
    
    // Git recovery commands
    const gitMatch = title.match(/git show [a-f0-9]+:[^\s]+/);
    if (gitMatch) commands.push(gitMatch[0]);
    
    // Command names
    const lowerTitle = title.toLowerCase();
    if (lowerTitle.includes('spawn')) commands.push('spawn');
    if (lowerTitle.includes('academy')) commands.push('academy');
    if (lowerTitle.includes('screenshot')) commands.push('screenshot');
    
    return commands;
  }

  /**
   * Default roadmap items for fallback
   */
  getDefaultItems() {
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
}

module.exports = RoadmapParser;