/**
 * IssuesCommand - AI-driven GitHub issue management with multi-agent collaboration
 * Self-contained module with external templates and configuration
 */

const BaseCommand = require('../../BaseCommand.cjs');
const fs = require('fs');
const path = require('path');

class IssuesCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'issues',
      description: 'AI-driven GitHub issue management with multi-agent collaboration',
      icon: '🎯',
      category: 'development',
      parameters: {
        action: {
          type: 'string',
          required: true,
          description: 'Action: list, create, update, assign, sync, dashboard',
          enum: ['list', 'create', 'update', 'assign', 'sync', 'dashboard']
        },
        filter: {
          type: 'string',
          required: false,
          description: 'Filter: all, open, assigned, ai-created, test-failures, urgent',
          default: 'all'
        },
        agent: {
          type: 'string',
          required: false,
          description: 'Agent name (auto-detected from connection)',
          default: 'auto'
        },
        category: {
          type: 'string',
          required: false,
          description: 'Issue category for creation',
          enum: ['cleanup', 'investigation', 'test-failure', 'architecture', 'enhancement']
        },
        issue_id: {
          type: 'string',
          required: false,
          description: 'GitHub issue number for updates'
        },
        title: {
          type: 'string',
          required: false,
          description: 'Issue title for creation'
        },
        auto_create: {
          type: 'boolean',
          required: false,
          description: 'Auto-create issues from FILES.md markers',
          default: false
        }
      }
    };
  }

  static loadTemplate(templateName) {
    const templatePath = path.join(__dirname, 'templates', templateName);
    return fs.readFileSync(templatePath, 'utf8');
  }

  static loadConfig(configName) {
    const configPath = path.join(__dirname, 'config', configName);
    return JSON.parse(fs.readFileSync(configPath, 'utf8'));
  }

  static getAgentFromConnection(continuum) {
    // Extract agent name from connection properties
    if (continuum && continuum.connection && continuum.connection.agent) {
      return continuum.connection.agent;
    }
    // Fallback to client info
    if (continuum && continuum.clients && continuum.clients.length > 0) {
      const client = continuum.clients[0];
      return client.agent || client.name || 'unknown-agent';
    }
    return 'system';
  }

  static async execute(paramsString, continuum) {
    try {
      const params = this.parseParams(paramsString);
      const action = params.action;
      const agent = params.agent === 'auto' ? this.getAgentFromConnection(continuum) : params.agent;
      
      // Load external configuration (no hardcoded messages)
      const messages = this.loadConfig('messages.json');
      const githubConfig = this.loadConfig('github_api.json');
      
      console.log(`${messages.actions.loading || '📡 Loading...'}`);

      switch (action) {
        case 'dashboard':
          return await this.showDashboard(params, agent, messages);
        
        case 'list':
          return await this.listIssues(params, agent, messages, githubConfig);
        
        case 'create':
          return await this.createIssue(params, agent, messages, githubConfig);
        
        case 'update':
          return await this.updateIssue(params, agent, messages, githubConfig);
        
        case 'assign':
          return await this.assignIssue(params, agent, messages, githubConfig);
        
        case 'sync':
          return await this.syncWithFilesMd(params, agent, messages, githubConfig);
        
        default:
          return this.createErrorResult('Invalid action', `Unknown action: ${action}`);
      }

    } catch (error) {
      return this.createErrorResult('Issues command failed', error.message);
    }
  }

  static async showDashboard(params, agent, messages) {
    const dashboardTemplate = this.loadTemplate('dashboard.html');
    
    // Load dashboard data
    const dashboardData = {
      agent_name: agent,
      agent_status: 'active',
      assigned_count: 0,
      critical_issues_html: '<p>Loading...</p>',
      assigned_issues_html: '<p>Loading...</p>',
      recent_activity_html: '<p>Loading...</p>'
    };

    console.log(messages.dashboard.header);
    console.log(messages.dashboard.separator);
    console.log(messages.dashboard.agent_status
      .replace('{agent}', agent)
      .replace('{status}', 'active'));

    return this.createSuccessResult({
      action: 'dashboard',
      agent,
      template: 'dashboard.html',
      data: dashboardData
    }, 'Dashboard displayed');
  }

  static async syncWithFilesMd(params, agent, messages, githubConfig) {
    console.log(messages.actions.sync_start);
    
    // Read FILES.md and extract issue markers
    const filesMdPath = path.join(process.cwd(), 'FILES.md');
    if (!fs.existsSync(filesMdPath)) {
      return this.createErrorResult('FILES.md not found', 'Run docs --sync first');
    }

    const filesMdContent = fs.readFileSync(filesMdPath, 'utf8');
    const issueMarkers = this.extractIssueMarkers(filesMdContent);

    console.log(messages.actions.auto_create_found
      .replace('{count}', issueMarkers.length));

    return this.createSuccessResult({
      action: 'sync',
      issues_found: issueMarkers.length,
      agent,
      source: 'FILES.md'
    }, messages.actions.sync_complete);
  }

  static extractIssueMarkers(content) {
    const markers = [];
    const lines = content.split('\n');
    
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      // Look for emoji markers: 🧹 🌀 🔥 📦 🎯
      if (line.includes('🧹') || line.includes('🌀') || line.includes('🔥') || 
          line.includes('📦') || line.includes('🎯')) {
        markers.push({
          line: i + 1,
          content: line.trim(),
          category: this.categorizeLine(line)
        });
      }
    }
    
    return markers;
  }

  static categorizeLine(line) {
    if (line.includes('🧹')) return 'cleanup';
    if (line.includes('🌀')) return 'investigation';
    if (line.includes('🔥')) return 'test-failure';
    if (line.includes('📦')) return 'architecture';
    if (line.includes('🎯')) return 'enhancement';
    return 'unknown';
  }

  // Placeholder methods for other actions
  static async listIssues(params, agent, messages, githubConfig) {
    return this.createSuccessResult({ action: 'list', agent }, 'Issues listed');
  }

  static async createIssue(params, agent, messages, githubConfig) {
    return this.createSuccessResult({ action: 'create', agent }, 'Issue created');
  }

  static async updateIssue(params, agent, messages, githubConfig) {
    return this.createSuccessResult({ action: 'update', agent }, 'Issue updated');
  }

  static async assignIssue(params, agent, messages, githubConfig) {
    return this.createSuccessResult({ action: 'assign', agent }, 'Issue assigned');
  }
}

module.exports = IssuesCommand;