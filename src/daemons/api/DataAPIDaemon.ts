/**
 * Data API Daemon - Handles core data API endpoints
 * 
 * RESPONSIBILITIES:
 * - Serve /api/agents and /api/personas endpoints
 * - Manage data retrieval from data modules
 * - Handle API request validation and responses
 * 
 * NOT RESPONSIBLE FOR:
 * - UI rendering (RendererDaemon)
 * - WebSocket routing (WebSocketDaemon)
 * - Static file serving (RendererDaemon)
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { getAgents } from '../../data/agents';
import { getPersonas } from '../../data/personas';

export class DataAPIDaemon extends BaseDaemon {
  public readonly name = 'data-api';
  public readonly version = '1.0.0';

  protected async onStart(): Promise<void> {
    this.log('📋 Starting Data API Daemon...');
    this.log('✅ Data API Daemon started');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Data API Daemon...');
    this.log('✅ Data API Daemon stopped');
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'get_agents':
        return this.handleGetAgents();
        
      case 'get_personas':
        return this.handleGetPersonas();
        
      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`
        };
    }
  }

  /**
   * Register API routes with WebSocketDaemon
   */
  public registerWithWebSocketDaemon(webSocketDaemon: any): void {
    // Store reference to get real system data
    this.webSocketDaemon = webSocketDaemon;
    
    // Register API endpoints
    webSocketDaemon.registerApiHandler('/api/agents', this, this.handleAgentsApi.bind(this));
    webSocketDaemon.registerApiHandler('/api/personas', this, this.handlePersonasApi.bind(this));
    
    this.log('🔌 Registered API routes with WebSocketDaemon');
  }

  private webSocketDaemon: any;

  /**
   * Handle /api/agents endpoint - returns REAL registered daemons as agents
   */
  private async handleAgentsApi(endpoint: string, req: any, res: any): Promise<void> {
    try {
      const agents = this.getRealAgentsFromSystem();
      
      res.writeHead(200, { 
        'Content-Type': 'application/json',
        'Cache-Control': 'public, max-age=60' // 1 minute cache for dynamic data
      });
      res.end(JSON.stringify(agents));
      
      this.log(`📋 Served real agents data (${agents.length} agents from system)`);
    } catch (error) {
      this.log(`❌ Failed to serve agents API: ${error.message}`, 'error');
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Internal server error' }));
    }
  }

  /**
   * Get real agents from the running system (registered daemons)
   */
  private getRealAgentsFromSystem(): any[] {
    const agents = [];
    
    // Add system coordinator
    agents.push({
      id: 'system',
      name: 'System Coordinator',
      role: 'System Management',
      avatar: '⚙️',
      status: 'online',
      type: 'system'
    });

    if (this.webSocketDaemon) {
      try {
        const systemStatus = this.webSocketDaemon.getSystemStatus();
        const registeredDaemons = systemStatus.registeredDaemons || [];
        
        // Convert registered daemons to agents
        for (const daemonName of registeredDaemons) {
          if (daemonName !== 'websocket-server') { // Skip the router itself
            agents.push({
              id: daemonName,
              name: this.formatDaemonName(daemonName),
              role: this.getDaemonRole(daemonName),
              avatar: this.getDaemonAvatar(daemonName),
              status: 'online',
              type: 'daemon',
              capabilities: this.getDaemonCapabilities(daemonName)
            });
          }
        }
        
        this.log(`🔍 Found ${registeredDaemons.length} registered daemons`);
      } catch (error) {
        this.log(`⚠️ Could not get system status: ${error.message}`, 'warn');
      }
    }
    
    return agents;
  }

  private formatDaemonName(daemonName: string): string {
    return daemonName
      .split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }

  private getDaemonRole(daemonName: string): string {
    const roleMap: Record<string, string> = {
      'renderer': 'UI Rendering',
      'browser-manager': 'Browser Orchestration', 
      'academy': 'AI Training & Education',
      'command-processor': 'Command Execution',
      'data-api': 'Data Services',
      'persona': 'AI Persona Management'
    };
    return roleMap[daemonName] || 'System Daemon';
  }

  private getDaemonAvatar(daemonName: string): string {
    const avatarMap: Record<string, string> = {
      'renderer': '🎨',
      'browser-manager': '🌐',
      'academy': '🎓',
      'command-processor': '⚡',
      'data-api': '📋',
      'persona': '🧠'
    };
    return avatarMap[daemonName] || '⚙️';
  }

  private getDaemonCapabilities(daemonName: string): string[] {
    const capabilityMap: Record<string, string[]> = {
      'renderer': ['ui-generation', 'static-files', 'html-rendering'],
      'browser-manager': ['browser-control', 'tab-management', 'automation'],
      'academy': ['ai-training', 'persona-development', 'lora-adapters'],
      'command-processor': ['command-execution', 'chat-processing', 'automation'],
      'data-api': ['data-services', 'api-endpoints', 'storage'],
      'persona': ['ai-personas', 'conversation', 'specialized-models']
    };
    return capabilityMap[daemonName] || ['system-service'];
  }

  /**
   * Handle /api/personas endpoint - TODO: integrate with PersonaDaemon when available
   */
  private async handlePersonasApi(endpoint: string, req: any, res: any): Promise<void> {
    try {
      // For now use fallback personas, but mark as dynamic for future
      const personas = this.getRealPersonasFromSystem();
      
      res.writeHead(200, { 
        'Content-Type': 'application/json',
        'Cache-Control': 'public, max-age=60' // 1 minute cache for dynamic data
      });
      res.end(JSON.stringify(personas));
      
      this.log(`📋 Served personas data (${personas.length} personas)`);
    } catch (error) {
      this.log(`❌ Failed to serve personas API: ${error.message}`, 'error');
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Internal server error' }));
    }
  }

  /**
   * Get real personas from system - will integrate with PersonaDaemon when available
   */
  private getRealPersonasFromSystem(): any[] {
    const personas = [];
    
    // TODO: Query PersonaDaemon for real persona instances
    // TODO: Load from persistent storage (.continuum/personas.json)
    // TODO: Check Academy training system for specialized personas
    
    // For now, return basic system personas
    personas.push({
      id: 'system-analyst',
      name: 'System Analyst',
      description: 'Analyzes system performance and daemon health',
      avatar: '📊',
      type: 'analytical',
      expertise: ['system-monitoring', 'performance-analysis', 'health-checks'],
      status: 'available'
    });

    personas.push({
      id: 'code-reviewer',
      name: 'Code Reviewer',
      description: 'Reviews TypeScript code and architecture decisions',
      avatar: '🔍',
      type: 'technical', 
      expertise: ['typescript', 'architecture', 'code-quality', 'testing'],
      status: 'available'
    });

    personas.push({
      id: 'ui-designer',
      name: 'UI Designer',
      description: 'Designs cyberpunk UI components and user experience',
      avatar: '🎨',
      type: 'creative',
      expertise: ['ui-design', 'cyberpunk-aesthetic', 'user-experience', 'components'],
      status: 'available'
    });
    
    this.log(`🧠 Generated ${personas.length} system personas (dynamic integration pending)`);
    return personas;
  }

  /**
   * Handle get_agents message
   */
  private async handleGetAgents(): Promise<DaemonResponse> {
    try {
      const agents = getAgents();
      return {
        success: true,
        data: agents
      };
    } catch (error) {
      return {
        success: false,
        error: `Failed to get agents: ${error.message}`
      };
    }
  }

  /**
   * Handle get_personas message
   */
  private async handleGetPersonas(): Promise<DaemonResponse> {
    try {
      const personas = getPersonas();
      return {
        success: true,
        data: personas
      };
    } catch (error) {
      return {
        success: false,
        error: `Failed to get personas: ${error.message}`
      };
    }
  }
}

// Main execution when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const daemon = new DataAPIDaemon();
  
  process.on('SIGINT', async () => {
    console.log('\\n🛑 Received shutdown signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\\n🛑 Received termination signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  daemon.start().catch(error => {
    console.error('❌ Data API daemon failed:', error);
    process.exit(1);
  });
}