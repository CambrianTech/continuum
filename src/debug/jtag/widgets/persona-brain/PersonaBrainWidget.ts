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

        ${this.selectedModule ? `<div class="module-details">${this.renderModuleDetails()}</div>` : ''}

        <div class="brain-stats">
          ${this.renderStats()}
        </div>
      </div>
    `;
  }

  private renderBrainSVG(): string {
    return `
      <svg viewBox="0 0 600 500" class="brain-svg" xmlns="http://www.w3.org/2000/svg">
        <defs>
          <!-- Glow filters -->
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
          <!-- Brain texture gradient -->
          <linearGradient id="brainGrad" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" style="stop-color:rgba(0,40,50,0.95)"/>
            <stop offset="100%" style="stop-color:rgba(0,20,30,0.98)"/>
          </linearGradient>
        </defs>

        <!-- Large brain outline (side profile) - centered -->
        <path class="brain-outline" d="
          M150 200
          C 130 140, 180 70, 280 50
          C 380 30, 480 60, 520 120
          C 550 170, 555 220, 545 270
          C 535 320, 500 370, 440 400
          L 400 415
          C 370 430, 330 445, 300 445
          L 295 480
          L 275 480
          L 270 445
          C 200 445, 150 420, 120 380
          C 90 340, 90 280, 120 230
          C 100 210, 110 190, 150 200
          Z"
          fill="url(#brainGrad)" stroke="rgba(0,212,255,0.4)" stroke-width="2"/>

        <!-- Brain surface texture (gyri/folds) -->
        <g class="brain-folds" opacity="0.25">
          <path d="M180 120 Q220 100 270 115 Q320 105 360 120" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
          <path d="M370 90 Q420 75 470 95 Q500 85 520 110" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
          <path d="M160 180 Q200 165 250 180 Q290 170 330 185" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
          <path d="M350 160 Q400 145 450 165 Q490 155 520 180" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
          <path d="M140 250 Q180 235 220 250" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
          <path d="M470 240 Q510 225 540 250" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
          <path d="M200 300 Q250 285 300 305 Q340 295 380 310" fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
        </g>

        <!-- MIND module box (top left, outside brain) -->
        <g class="brain-module module-mind ${this.getModuleClass('mind')}" data-module="mind">
          <rect x="20" y="30" width="100" height="70" rx="6" class="module-shape module-fill"/>
          <text x="70" y="55" class="module-label">MIND</text>
          <text x="70" y="75" class="module-sublabel">Cognition</text>
          <!-- Circuit connection to brain -->
          <path d="M120 65 L160 65 L200 100" class="circuit-connection" fill="none"/>
          <circle cx="200" cy="100" r="4" class="connection-node"/>
        </g>

        <!-- SOUL module box (top right, outside brain) -->
        <g class="brain-module module-soul ${this.getModuleClass('soul')}" data-module="soul">
          <rect x="480" y="30" width="100" height="70" rx="6" class="module-shape module-fill"/>
          <text x="530" y="55" class="module-label">SOUL</text>
          <text x="530" y="75" class="module-sublabel">Values</text>
          <!-- Circuit connection to brain -->
          <path d="M480 65 L440 65 L400 100" class="circuit-connection" fill="none"/>
          <circle cx="400" cy="100" r="4" class="connection-node"/>
        </g>

        <!-- HIPPOCAMPUS module box (left side, outside brain) -->
        <g class="brain-module module-hippocampus ${this.getModuleClass('hippocampus')}" data-module="hippocampus">
          <rect x="10" y="200" width="110" height="90" rx="6" class="module-shape module-fill"/>
          <text x="65" y="230" class="module-label">HIPPO</text>
          <text x="65" y="250" class="module-sublabel">Memory</text>
          <text x="65" y="275" class="module-stat">${this.moduleStats.hippocampus.memoryCount}</text>
          <!-- Circuit connection to brain -->
          <path d="M120 245 L150 245 L180 260" class="circuit-connection" fill="none"/>
          <circle cx="180" cy="260" r="4" class="connection-node"/>
        </g>

        <!-- BODY module box (right side, outside brain) -->
        <g class="brain-module module-body ${this.getModuleClass('body')}" data-module="body">
          <rect x="480" y="200" width="110" height="90" rx="6" class="module-shape module-fill"/>
          <text x="535" y="230" class="module-label">BODY</text>
          <text x="535" y="250" class="module-sublabel">Tools</text>
          <text x="535" y="275" class="module-stat">${this.moduleStats.body.toolsAvailable}</text>
          <!-- Circuit connection to brain -->
          <path d="M480 245 L450 245 L420 260" class="circuit-connection" fill="none"/>
          <circle cx="420" cy="260" r="4" class="connection-node"/>
        </g>

        <!-- CNS module box (bottom center, outside brain) -->
        <g class="brain-module module-cns ${this.getModuleClass('cns')}" data-module="cns">
          <rect x="245" y="420" width="110" height="70" rx="6" class="module-shape module-fill"/>
          <text x="300" y="445" class="module-label">CNS</text>
          <text x="300" y="465" class="module-sublabel">Integration</text>
          <!-- Circuit connection up to brain stem -->
          <path d="M300 420 L300 395 L285 380" class="circuit-connection" fill="none"/>
          <circle cx="285" cy="380" r="4" class="connection-node"/>
        </g>

        <!-- Internal brain regions (subtle highlights inside brain) -->
        <g opacity="0.4">
          <!-- Frontal area glow -->
          <ellipse cx="220" cy="150" rx="60" ry="50" fill="rgba(0,212,255,0.1)" class="brain-region region-mind"/>
          <!-- Limbic area glow -->
          <ellipse cx="320" cy="250" rx="50" ry="40" fill="rgba(0,212,255,0.1)" class="brain-region region-soul"/>
          <!-- Temporal area glow -->
          <ellipse cx="200" cy="300" rx="45" ry="35" fill="rgba(0,212,255,0.1)" class="brain-region region-hippo"/>
          <!-- Cerebellum area glow -->
          <ellipse cx="450" cy="300" rx="50" ry="45" fill="rgba(0,212,255,0.1)" class="brain-region region-body"/>
          <!-- Brain stem area glow -->
          <ellipse cx="300" cy="400" rx="30" ry="40" fill="rgba(0,212,255,0.1)" class="brain-region region-cns"/>
        </g>

        <!-- Internal connection network -->
        <g class="neural-network" opacity="0.5">
          <path d="M220 150 Q270 200 320 250" fill="none" class="neural-path"/>
          <path d="M320 250 L200 300" fill="none" class="neural-path"/>
          <path d="M320 250 L450 300" fill="none" class="neural-path"/>
          <path d="M320 250 L300 400" fill="none" class="neural-path"/>
          <path d="M220 150 Q260 280 300 400" fill="none" class="neural-path"/>
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
    flex: 1;
  }

  .brain-svg {
    width: 100%;
    max-width: 700px;
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
    fill: rgba(255, 255, 255, 0.9);
    font-size: 14px;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
  }

  .module-sublabel {
    fill: rgba(255, 255, 255, 0.5);
    font-size: 10px;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
  }

  .module-stat {
    fill: #00d4ff;
    font-size: 12px;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
  }

  /* Circuit connections from modules to brain */
  .circuit-connection {
    stroke: rgba(0, 212, 255, 0.5);
    stroke-width: 2;
    stroke-dasharray: 8 4;
    animation: data-pulse 2s linear infinite;
  }

  .connection-node {
    fill: #00d4ff;
    stroke: rgba(0, 212, 255, 0.8);
    stroke-width: 2;
  }

  @keyframes data-pulse {
    0% { stroke-dashoffset: 0; }
    100% { stroke-dashoffset: -24; }
  }

  /* Neural paths inside brain */
  .neural-path {
    stroke: rgba(0, 212, 255, 0.3);
    stroke-width: 2;
    stroke-dasharray: 4 4;
  }

  /* Brain region highlights */
  .brain-region {
    transition: all 0.3s ease;
  }

  .brain-module:hover ~ .brain-region {
    fill: rgba(0, 212, 255, 0.2);
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
