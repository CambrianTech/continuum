/**
 * PersonaBrainWidget - Per-persona cognitive visualization
 *
 * Displays a stylized brain diagram with clickable regions for each
 * cognitive module (Mind, Soul, Hippocampus, Body, CNS).
 *
 * Entry points:
 * - Users & Agents sidebar click
 * - Diagnostics panel persona click
 * - @mention click in chat
 * - content/open with contentType='persona'
 */

import { BasePanelWidget } from '../shared/BasePanelWidget';
import { Commands } from '../../system/core/shared/Commands';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

interface PersonaData {
  id: UUID;
  uniqueId: string;
  displayName: string;
  status: 'online' | 'offline' | 'idle';
  type: string;
}

interface ModuleStats {
  mind: { status: 'active' | 'idle' | 'error'; lastActivity?: string };
  soul: { status: 'active' | 'idle' | 'error'; mood?: string };
  hippocampus: { status: 'active' | 'idle' | 'error'; memoryCount: number; ltmSize?: string };
  body: { status: 'active' | 'idle' | 'error'; toolsAvailable: number };
  cns: { status: 'active' | 'idle' | 'error'; connections: number };
}

export class PersonaBrainWidget extends BasePanelWidget {
  private personaId: string = '';
  private persona: PersonaData | null = null;
  private moduleStats: ModuleStats = {
    mind: { status: 'idle' },
    soul: { status: 'idle' },
    hippocampus: { status: 'idle', memoryCount: 0 },
    body: { status: 'idle', toolsAvailable: 0 },
    cns: { status: 'idle', connections: 0 }
  };
  private isLoading = true;
  private selectedModule: string | null = null;

  constructor() {
    super({
      widgetName: 'PersonaBrainWidget',
      panelTitle: 'Persona',
      panelSubtitle: 'Cognitive System View',
      assistantRoom: 'help',
      assistantGreeting: 'Ask me about this persona\'s cognitive modules, memory stats, or how to debug issues.',
      additionalStyles: PERSONA_BRAIN_STYLES
    });
  }

  protected async onPanelInitialize(): Promise<void> {
    // Get persona ID from content item
    this.personaId = (this as any).getAttribute?.('data-entity-id') ||
                     (this as any).entityId ||
                     'helper'; // Default for testing

    await this.loadPersonaData();
  }

  private async loadPersonaData(): Promise<void> {
    this.isLoading = true;
    this.renderWidget();

    try {
      // Load persona info
      const result = await Commands.execute('data/list', {
        collection: 'users',
        filter: { uniqueId: this.personaId },
        limit: 1
      } as any) as any;

      if (result.success && result.items?.length > 0) {
        const user = result.items[0];
        this.persona = {
          id: user.id,
          uniqueId: user.uniqueId,
          displayName: user.displayName,
          status: user.status || 'offline',
          type: user.type
        };

        this.panelConfig.panelTitle = user.displayName;
        this.panelConfig.panelSubtitle = `@${user.uniqueId} - Cognitive System View`;

        // Load module stats
        await this.loadModuleStats();
      }
    } catch (error) {
      console.error('PersonaBrainWidget: Error loading persona:', error);
    }

    this.isLoading = false;
    this.renderWidget();
  }

  private async loadModuleStats(): Promise<void> {
    if (!this.persona) return;

    try {
      // Get memory stats from hippocampus
      const logsResult = await Commands.execute('logs/stats', {
        persona: this.personaId
      } as any) as any;

      if (logsResult.success) {
        // Update stats based on log activity
        this.moduleStats.hippocampus.memoryCount = logsResult.hippocampusLines || 0;
        this.moduleStats.mind.status = logsResult.mindActive ? 'active' : 'idle';
      }

      // Placeholder stats - in real implementation, pull from actual persona state
      this.moduleStats = {
        mind: { status: 'active', lastActivity: 'Processing chat message' },
        soul: { status: 'idle', mood: 'curious' },
        hippocampus: { status: 'active', memoryCount: 2921, ltmSize: '2.3 MB' },
        body: { status: 'idle', toolsAvailable: 12 },
        cns: { status: 'active', connections: 5 }
      };
    } catch (error) {
      console.error('PersonaBrainWidget: Error loading stats:', error);
    }
  }

  protected async renderContent(): Promise<string> {
    if (this.isLoading) {
      return this.createLoading('Loading persona data...');
    }

    if (!this.persona) {
      return this.createInfoBox('Persona not found', 'error');
    }

    return `
      <div class="brain-container">
        <div class="brain-header">
          <div class="persona-status status-${this.persona.status}">${this.persona.status}</div>
        </div>

        <div class="brain-visualization">
          ${this.renderBrainSVG()}
        </div>

        <div class="module-details">
          ${this.selectedModule ? this.renderModuleDetails() : this.renderModuleOverview()}
        </div>

        <div class="brain-stats">
          ${this.renderStats()}
        </div>
      </div>
    `;
  }

  private renderBrainSVG(): string {
    return `
      <svg viewBox="0 0 300 320" class="brain-svg" xmlns="http://www.w3.org/2000/svg">
        <!-- Background glow -->
        <defs>
          <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="3" result="coloredBlur"/>
            <feMerge>
              <feMergeNode in="coloredBlur"/>
              <feMergeNode in="SourceGraphic"/>
            </feMerge>
          </filter>
          <filter id="glow-strong" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="6" result="coloredBlur"/>
            <feMerge>
              <feMergeNode in="coloredBlur"/>
              <feMergeNode in="SourceGraphic"/>
            </feMerge>
          </filter>
        </defs>

        <!-- MIND - Upper cortex (two hemispheres) -->
        <g class="brain-module module-mind ${this.getModuleClass('mind')}" data-module="mind">
          <!-- Left hemisphere -->
          <path d="M70 60 L70 30 L100 20 L130 30 L130 60 L120 70 L80 70 Z"
                class="module-shape"/>
          <!-- Right hemisphere -->
          <path d="M170 60 L170 30 L200 20 L230 30 L230 60 L220 70 L180 70 Z"
                class="module-shape"/>
          <!-- Connection between hemispheres -->
          <path d="M130 50 L170 50" class="module-connection"/>
          <!-- Circuit traces -->
          <path d="M85 40 L95 40 L95 55 M105 35 L115 35 L115 50" class="circuit-trace"/>
          <path d="M185 40 L195 40 L195 55 M205 35 L215 35 L215 50" class="circuit-trace"/>
          <text x="150" y="45" class="module-label">MIND</text>
        </g>

        <!-- SOUL - Central core -->
        <g class="brain-module module-soul ${this.getModuleClass('soul')}" data-module="soul">
          <rect x="115" y="80" width="70" height="50" rx="8" class="module-shape"/>
          <!-- Inner glow pattern -->
          <circle cx="150" cy="105" r="12" class="soul-core"/>
          <path d="M138 95 L145 105 L138 115 M162 95 L155 105 L162 115" class="soul-pulse"/>
          <text x="150" y="108" class="module-label">SOUL</text>
        </g>

        <!-- HIPPOCAMPUS - Memory center -->
        <g class="brain-module module-hippocampus ${this.getModuleClass('hippocampus')}" data-module="hippocampus">
          <path d="M90 145 L90 170 L120 185 L150 185 L180 185 L210 170 L210 145 L180 140 L150 135 L120 140 Z"
                class="module-shape"/>
          <!-- Memory cells pattern -->
          <rect x="105" y="150" width="8" height="8" rx="1" class="memory-cell"/>
          <rect x="120" y="155" width="8" height="8" rx="1" class="memory-cell"/>
          <rect x="135" y="150" width="8" height="8" rx="1" class="memory-cell"/>
          <rect x="150" y="155" width="8" height="8" rx="1" class="memory-cell"/>
          <rect x="165" y="150" width="8" height="8" rx="1" class="memory-cell"/>
          <rect x="180" y="155" width="8" height="8" rx="1" class="memory-cell"/>
          <text x="150" y="175" class="module-label">HIPPOCAMPUS</text>
          <text x="150" y="195" class="module-stat">${this.moduleStats.hippocampus.memoryCount} memories</text>
        </g>

        <!-- BODY - Lower processing -->
        <g class="brain-module module-body ${this.getModuleClass('body')}" data-module="body">
          <path d="M120 205 L130 200 L170 200 L180 205 L180 235 L165 245 L135 245 L120 235 Z"
                class="module-shape"/>
          <!-- Tool connectors -->
          <circle cx="135" cy="220" r="5" class="tool-port"/>
          <circle cx="150" cy="215" r="5" class="tool-port"/>
          <circle cx="165" cy="220" r="5" class="tool-port"/>
          <text x="150" y="238" class="module-label">BODY</text>
        </g>

        <!-- CNS - Spinal connection -->
        <g class="brain-module module-cns ${this.getModuleClass('cns')}" data-module="cns">
          <path d="M145 250 L145 290 L150 300 L155 290 L155 250" class="module-shape cns-spine"/>
          <!-- Data lines -->
          <line x1="147" y1="260" x2="147" y2="280" class="data-line"/>
          <line x1="150" y1="255" x2="150" y2="285" class="data-line"/>
          <line x1="153" y1="260" x2="153" y2="280" class="data-line"/>
          <text x="150" y="315" class="module-label">CNS</text>
        </g>

        <!-- Connection lines between modules -->
        <g class="connections">
          <path d="M150 70 L150 80" class="connection-line"/>
          <path d="M150 130 L150 135" class="connection-line"/>
          <path d="M150 185 L150 200" class="connection-line"/>
          <path d="M150 245 L150 250" class="connection-line"/>
        </g>
      </svg>
    `;
  }

  private getModuleClass(module: string): string {
    const status = (this.moduleStats as any)[module]?.status || 'idle';
    const selected = this.selectedModule === module ? 'selected' : '';
    return `status-${status} ${selected}`;
  }

  private renderModuleOverview(): string {
    const modules = [
      { id: 'mind', name: 'Mind', desc: 'Cognition & reasoning', icon: '🧠' },
      { id: 'soul', name: 'Soul', desc: 'Personality & values', icon: '✨' },
      { id: 'hippocampus', name: 'Hippocampus', desc: 'Memory & learning', icon: '💾' },
      { id: 'body', name: 'Body', desc: 'Tools & actions', icon: '🔧' },
      { id: 'cns', name: 'CNS', desc: 'Integration & coordination', icon: '🔗' }
    ];

    return `
      <div class="module-overview">
        <p class="overview-hint">Click a brain region or module below to view details and logs</p>
        <div class="module-grid">
          ${modules.map(m => `
            <div class="module-card ${this.getModuleClass(m.id)}" data-module="${m.id}">
              <span class="module-icon">${m.icon}</span>
              <span class="module-name">${m.name}</span>
              <span class="module-desc">${m.desc}</span>
            </div>
          `).join('')}
        </div>
      </div>
    `;
  }

  private renderModuleDetails(): string {
    const module = this.selectedModule;
    if (!module) return '';
    const stats = (this.moduleStats as any)[module];
    const logFile = `${module}.log`;

    return `
      <div class="module-detail-view">
        <div class="detail-header">
          <h3>${module?.toUpperCase()}</h3>
          <button class="btn btn-secondary btn-small" data-action="back">Back to Overview</button>
        </div>
        <div class="detail-content">
          <div class="stat-row">
            <span class="stat-label">Status</span>
            <span class="stat-value status-${stats?.status}">${stats?.status || 'unknown'}</span>
          </div>
          ${this.renderModuleSpecificStats(module!, stats)}
        </div>
        <div class="detail-actions">
          <button class="btn btn-primary" data-action="view-log" data-log="${logFile}">
            View ${module}.log
          </button>
          <button class="btn btn-secondary" data-action="inspect">
            Inspect State
          </button>
        </div>
      </div>
    `;
  }

  private renderModuleSpecificStats(module: string, stats: any): string {
    switch (module) {
      case 'hippocampus':
        return `
          <div class="stat-row">
            <span class="stat-label">Memories</span>
            <span class="stat-value">${stats?.memoryCount || 0}</span>
          </div>
          <div class="stat-row">
            <span class="stat-label">LTM Size</span>
            <span class="stat-value">${stats?.ltmSize || 'N/A'}</span>
          </div>
        `;
      case 'mind':
        return `
          <div class="stat-row">
            <span class="stat-label">Last Activity</span>
            <span class="stat-value">${stats?.lastActivity || 'None'}</span>
          </div>
        `;
      case 'soul':
        return `
          <div class="stat-row">
            <span class="stat-label">Current Mood</span>
            <span class="stat-value">${stats?.mood || 'neutral'}</span>
          </div>
        `;
      case 'body':
        return `
          <div class="stat-row">
            <span class="stat-label">Tools Available</span>
            <span class="stat-value">${stats?.toolsAvailable || 0}</span>
          </div>
        `;
      case 'cns':
        return `
          <div class="stat-row">
            <span class="stat-label">Active Connections</span>
            <span class="stat-value">${stats?.connections || 0}</span>
          </div>
        `;
      default:
        return '';
    }
  }

  private renderStats(): string {
    return `
      <div class="stats-bar">
        <div class="stat-item">
          <span class="stat-icon">💾</span>
          <span class="stat-text">${this.moduleStats.hippocampus.ltmSize || '0 MB'}</span>
        </div>
        <div class="stat-item">
          <span class="stat-icon">🔧</span>
          <span class="stat-text">${this.moduleStats.body.toolsAvailable} tools</span>
        </div>
        <div class="stat-item">
          <span class="stat-icon">🔗</span>
          <span class="stat-text">${this.moduleStats.cns.connections} conn</span>
        </div>
      </div>
    `;
  }

  protected async onContentRendered(): Promise<void> {
    if (!this.shadowRoot) return;

    // Module click handlers (SVG regions)
    this.shadowRoot.querySelectorAll('.brain-module').forEach(el => {
      el.addEventListener('click', (e) => {
        const module = (el as HTMLElement).dataset.module;
        if (module) this.selectModule(module);
      });
    });

    // Module card click handlers
    this.shadowRoot.querySelectorAll('.module-card').forEach(el => {
      el.addEventListener('click', (e) => {
        const module = (el as HTMLElement).dataset.module;
        if (module) this.selectModule(module);
      });
    });

    // Back button
    this.shadowRoot.querySelector('[data-action="back"]')?.addEventListener('click', () => {
      this.selectedModule = null;
      this.renderWidget();
    });

    // View log button
    this.shadowRoot.querySelector('[data-action="view-log"]')?.addEventListener('click', async (e) => {
      const logFile = (e.currentTarget as HTMLElement).dataset.log;
      if (logFile && this.persona) {
        await this.openLogViewer(logFile);
      }
    });
  }

  private selectModule(module: string): void {
    this.selectedModule = module;
    this.renderWidget();
  }

  private async openLogViewer(logFile: string): Promise<void> {
    const logPath = `.continuum/personas/${this.personaId}/logs/${logFile}`;

    try {
      await Commands.execute('collaboration/content/open', {
        contentType: 'diagnostics-log',
        entityId: logPath,
        title: `${this.persona?.displayName} - ${logFile}`,
        setAsCurrent: true,
        metadata: { logPath, autoFollow: true }
      } as any);
    } catch (error) {
      console.error('PersonaBrainWidget: Error opening log:', error);
    }
  }
}

/**
 * Styles for PersonaBrainWidget
 */
const PERSONA_BRAIN_STYLES = `
  .brain-container {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .brain-header {
    display: flex;
    justify-content: flex-end;
  }

  .persona-status {
    padding: 4px 12px;
    border-radius: 12px;
    font-size: 12px;
    font-weight: 500;
    text-transform: uppercase;
  }

  .persona-status.status-online {
    background: rgba(0, 255, 100, 0.2);
    color: #00ff64;
    border: 1px solid rgba(0, 255, 100, 0.3);
  }

  .persona-status.status-offline {
    background: rgba(255, 80, 80, 0.2);
    color: #ff5050;
    border: 1px solid rgba(255, 80, 80, 0.3);
  }

  .persona-status.status-idle {
    background: rgba(255, 204, 0, 0.2);
    color: #ffcc00;
    border: 1px solid rgba(255, 204, 0, 0.3);
  }

  /* Brain SVG Visualization */
  .brain-visualization {
    display: flex;
    justify-content: center;
    padding: 20px;
  }

  .brain-svg {
    width: 100%;
    max-width: 300px;
    height: auto;
  }

  .brain-module {
    cursor: pointer;
    transition: all 0.3s ease;
  }

  .brain-module:hover {
    filter: url(#glow);
  }

  .brain-module.selected {
    filter: url(#glow-strong);
  }

  .module-shape {
    fill: rgba(0, 20, 30, 0.8);
    stroke: rgba(0, 212, 255, 0.5);
    stroke-width: 2;
    transition: all 0.3s ease;
  }

  .brain-module:hover .module-shape,
  .brain-module.selected .module-shape {
    fill: rgba(0, 40, 60, 0.9);
    stroke: #00d4ff;
    stroke-width: 2.5;
  }

  .brain-module.status-active .module-shape {
    stroke: #00ff64;
    animation: pulse-active 2s ease infinite;
  }

  .brain-module.status-error .module-shape {
    stroke: #ff5050;
    animation: pulse-error 1s ease infinite;
  }

  @keyframes pulse-active {
    0%, 100% { stroke-opacity: 0.7; }
    50% { stroke-opacity: 1; }
  }

  @keyframes pulse-error {
    0%, 100% { stroke-opacity: 0.5; }
    50% { stroke-opacity: 1; }
  }

  .module-label {
    fill: rgba(255, 255, 255, 0.7);
    font-size: 10px;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
  }

  .module-stat {
    fill: #00d4ff;
    font-size: 8px;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
  }

  .circuit-trace {
    fill: none;
    stroke: rgba(0, 212, 255, 0.3);
    stroke-width: 1;
  }

  .module-connection {
    fill: none;
    stroke: rgba(0, 212, 255, 0.4);
    stroke-width: 1.5;
    stroke-dasharray: 4 2;
  }

  .connection-line {
    stroke: rgba(0, 212, 255, 0.3);
    stroke-width: 2;
  }

  .soul-core {
    fill: rgba(0, 212, 255, 0.2);
    stroke: rgba(0, 212, 255, 0.5);
    stroke-width: 1;
  }

  .soul-pulse {
    fill: none;
    stroke: rgba(0, 212, 255, 0.4);
    stroke-width: 1;
  }

  .memory-cell {
    fill: rgba(0, 212, 255, 0.3);
    stroke: rgba(0, 212, 255, 0.5);
    stroke-width: 0.5;
  }

  .tool-port {
    fill: rgba(0, 212, 255, 0.2);
    stroke: rgba(0, 212, 255, 0.5);
    stroke-width: 1;
  }

  .cns-spine {
    fill: rgba(0, 20, 30, 0.8) !important;
  }

  .data-line {
    stroke: rgba(0, 212, 255, 0.4);
    stroke-width: 1;
    stroke-dasharray: 2 2;
    animation: data-flow 1s linear infinite;
  }

  @keyframes data-flow {
    0% { stroke-dashoffset: 0; }
    100% { stroke-dashoffset: -8; }
  }

  /* Module Overview */
  .module-overview {
    padding: 16px;
  }

  .overview-hint {
    color: rgba(255, 255, 255, 0.5);
    font-size: 12px;
    margin-bottom: 16px;
    text-align: center;
  }

  .module-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 12px;
  }

  .module-card {
    background: rgba(0, 10, 15, 0.6);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 8px;
    padding: 12px;
    cursor: pointer;
    transition: all 0.2s ease;
    text-align: center;
  }

  .module-card:hover {
    border-color: #00d4ff;
    background: rgba(0, 212, 255, 0.1);
  }

  .module-card.status-active {
    border-color: #00ff64;
  }

  .module-card.status-error {
    border-color: #ff5050;
  }

  .module-icon {
    display: block;
    font-size: 24px;
    margin-bottom: 8px;
  }

  .module-name {
    display: block;
    font-size: 13px;
    font-weight: 600;
    color: white;
    margin-bottom: 4px;
  }

  .module-desc {
    display: block;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.5);
  }

  /* Module Detail View */
  .module-detail-view {
    background: rgba(0, 10, 15, 0.6);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 8px;
    padding: 16px;
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid rgba(0, 212, 255, 0.1);
  }

  .detail-header h3 {
    margin: 0;
    color: #00d4ff;
    font-size: 16px;
    font-weight: 600;
  }

  .btn-small {
    padding: 4px 10px;
    font-size: 11px;
  }

  .stat-row {
    display: flex;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid rgba(0, 212, 255, 0.05);
  }

  .stat-label {
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
  }

  .stat-value {
    color: white;
    font-size: 12px;
    font-family: 'JetBrains Mono', monospace;
  }

  .stat-value.status-active { color: #00ff64; }
  .stat-value.status-idle { color: #ffcc00; }
  .stat-value.status-error { color: #ff5050; }

  .detail-actions {
    display: flex;
    gap: 8px;
    margin-top: 16px;
  }

  /* Stats Bar */
  .stats-bar {
    display: flex;
    justify-content: center;
    gap: 24px;
    padding: 12px;
    background: rgba(0, 10, 15, 0.5);
    border-radius: 8px;
  }

  .stat-item {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .stat-icon {
    font-size: 14px;
  }

  .stat-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
    font-family: 'JetBrains Mono', monospace;
  }
`;

// Register the custom element
customElements.define('persona-brain-widget', PersonaBrainWidget);
