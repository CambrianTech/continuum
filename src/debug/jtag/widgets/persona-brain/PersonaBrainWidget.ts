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
      <svg viewBox="0 0 800 600" class="brain-svg" xmlns="http://www.w3.org/2000/svg">
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
            <feGaussianBlur stdDeviation="8" result="coloredBlur"/>
            <feMerge>
              <feMergeNode in="coloredBlur"/>
              <feMergeNode in="SourceGraphic"/>
            </feMerge>
          </filter>
          <filter id="glow-subtle" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="2" result="coloredBlur"/>
            <feMerge>
              <feMergeNode in="coloredBlur"/>
              <feMergeNode in="SourceGraphic"/>
            </feMerge>
          </filter>
          <!-- Scan line pattern -->
          <pattern id="scanlines" patternUnits="userSpaceOnUse" width="4" height="4">
            <line x1="0" y1="0" x2="4" y2="0" stroke="rgba(0,212,255,0.03)" stroke-width="1"/>
          </pattern>
        </defs>

        <!-- Background scanlines overlay -->
        <rect x="0" y="0" width="800" height="600" fill="url(#scanlines)" opacity="0.5"/>

        <!-- HUD outer ring (decorative) -->
        <g class="hud-ring" opacity="0.3">
          <circle cx="400" cy="280" r="260" fill="none" stroke="rgba(0,212,255,0.2)" stroke-width="1" stroke-dasharray="4 8"/>
          <circle cx="400" cy="280" r="270" fill="none" stroke="rgba(0,212,255,0.1)" stroke-width="1"/>
        </g>

        <!-- LOW-POLY BRAIN - Flat oval with bottom-left cut out -->
        <g class="brain-lowpoly" transform="translate(220, 120)">
          <!-- Shape: Wide flat oval, bottom-left corner cut out (like 90° missing) -->

          <!-- TOP ARC - brightest -->
          <polygon class="brain-facet f-bright" points="60,60 120,30 150,55"/>
          <polygon class="brain-facet f-bright" points="120,30 200,20 180,50"/>
          <polygon class="brain-facet f-light" points="120,30 180,50 150,55"/>
          <polygon class="brain-facet f-bright" points="200,20 280,30 250,55"/>
          <polygon class="brain-facet f-light" points="200,20 250,55 180,50"/>
          <polygon class="brain-facet f-light" points="280,30 330,60 290,70"/>
          <polygon class="brain-facet f-med" points="280,30 290,70 250,55"/>

          <!-- UPPER SIDES -->
          <polygon class="brain-facet f-light" points="40,100 60,60 80,95"/>
          <polygon class="brain-facet f-med" points="60,60 150,55 110,90"/>
          <polygon class="brain-facet f-light" points="60,60 110,90 80,95"/>
          <polygon class="brain-facet f-med" points="150,55 180,50 160,90"/>
          <polygon class="brain-facet f-light" points="110,90 150,55 160,90"/>
          <polygon class="brain-facet f-med" points="180,50 250,55 220,90"/>
          <polygon class="brain-facet f-light" points="160,90 180,50 220,90"/>
          <polygon class="brain-facet f-med" points="250,55 290,70 270,100"/>
          <polygon class="brain-facet f-light" points="220,90 250,55 270,100"/>
          <polygon class="brain-facet f-med" points="290,70 330,60 320,100"/>
          <polygon class="brain-facet f-dark" points="270,100 290,70 320,100"/>

          <!-- RIGHT SIDE (back of brain) - curves down -->
          <polygon class="brain-facet f-med" points="330,60 355,100 330,130"/>
          <polygon class="brain-facet f-dark" points="320,100 330,60 330,130"/>
          <polygon class="brain-facet f-med" points="355,100 360,150 340,160"/>
          <polygon class="brain-facet f-dark" points="330,130 355,100 340,160"/>
          <polygon class="brain-facet f-med" points="340,160 360,150 350,200"/>
          <polygon class="brain-facet f-dark" points="330,180 340,160 350,200"/>
          <polygon class="brain-facet f-med" points="310,200 330,180 350,200"/>

          <!-- MIDDLE BAND -->
          <polygon class="brain-facet f-med" points="80,130 80,95 130,115"/>
          <polygon class="brain-facet f-dark" points="80,95 110,90 130,115"/>
          <polygon class="brain-facet f-med" points="130,115 110,90 170,110"/>
          <polygon class="brain-facet f-dark" points="110,90 160,90 170,110"/>
          <polygon class="brain-facet f-med" points="170,110 160,90 220,105"/>
          <polygon class="brain-facet f-dark" points="160,90 220,90 220,105"/>
          <polygon class="brain-facet f-med" points="220,105 220,90 280,110"/>
          <polygon class="brain-facet f-dark" points="220,90 270,100 280,110"/>
          <polygon class="brain-facet f-med" points="280,110 270,100 320,120"/>
          <polygon class="brain-facet f-dark" points="270,100 320,100 320,120"/>
          <polygon class="brain-facet f-med" points="320,120 320,100 330,130"/>

          <!-- LOWER MIDDLE -->
          <polygon class="brain-facet f-dark" points="130,150 130,115 180,135"/>
          <polygon class="brain-facet f-med" points="130,115 170,110 180,135"/>
          <polygon class="brain-facet f-dark" points="180,135 170,110 230,130"/>
          <polygon class="brain-facet f-med" points="170,110 220,105 230,130"/>
          <polygon class="brain-facet f-dark" points="230,130 220,105 280,125"/>
          <polygon class="brain-facet f-med" points="220,105 280,110 280,125"/>
          <polygon class="brain-facet f-dark" points="280,125 280,110 320,135"/>
          <polygon class="brain-facet f-med" points="280,110 320,120 320,135"/>
          <polygon class="brain-facet f-dark" points="320,135 320,120 330,150"/>
          <polygon class="brain-facet f-med" points="320,120 330,130 330,150"/>

          <!-- BOTTOM (with cut-out on left) -->
          <!-- Cut starts around x=130, goes down-left -->
          <polygon class="brain-facet f-dark" points="180,170 180,135 240,155"/>
          <polygon class="brain-facet f-med" points="180,135 230,130 240,155"/>
          <polygon class="brain-facet f-dark" points="240,155 230,130 290,150"/>
          <polygon class="brain-facet f-med" points="230,130 280,125 290,150"/>
          <polygon class="brain-facet f-dark" points="290,150 280,125 330,150"/>
          <polygon class="brain-facet f-med" points="280,125 320,135 330,150"/>

          <!-- BOTTOM RIGHT - cerebellum area -->
          <polygon class="brain-facet f-dark" points="290,175 290,150 340,170"/>
          <polygon class="brain-facet f-med" points="290,150 330,150 340,170"/>
          <polygon class="brain-facet f-dark" points="340,170 330,150 350,180"/>
          <polygon class="brain-facet f-med" points="330,150 330,180 350,180"/>
          <polygon class="brain-facet f-dark" points="290,200 290,175 330,190"/>
          <polygon class="brain-facet f-med" points="290,175 340,170 330,190"/>
          <polygon class="brain-facet f-dark" points="330,190 340,170 350,200"/>
          <polygon class="brain-facet f-med" points="340,170 350,180 350,200"/>

          <!-- BRAIN STEM (bottom middle) -->
          <polygon class="brain-facet f-dark" points="220,200 240,155 260,190"/>
          <polygon class="brain-facet f-med" points="240,155 290,150 270,185"/>
          <polygon class="brain-facet f-dark" points="260,190 240,155 270,185"/>
          <polygon class="brain-facet f-med" points="270,185 290,150 290,175"/>
          <polygon class="brain-facet f-dark" points="230,220 220,200 260,210"/>
          <polygon class="brain-facet f-med" points="220,200 260,190 260,210"/>
          <polygon class="brain-facet f-dark" points="260,210 260,190 280,205"/>
          <polygon class="brain-facet f-med" points="260,190 270,185 280,205"/>
          <polygon class="brain-facet f-dark" points="240,240 230,220 260,230"/>
          <polygon class="brain-facet f-med" points="230,220 260,210 260,230"/>

          <!-- Brain outline -->
          <path class="brain-edge" d="
            M40,100 C30,60 60,30 120,20
            C180,10 240,10 300,25
            C340,35 360,70 365,120
            C370,170 360,200 340,220
            C320,240 280,250 250,250
            L240,240 L230,220
            C200,200 170,180 150,170
            L130,150 L80,130
            C50,120 40,110 40,100 Z"
            fill="none" stroke="rgba(0,212,255,0.6)" stroke-width="1.5"/>
        </g>

        <!-- Neural activity nodes (glowing dots on brain) -->
        <g class="neural-nodes">
          <circle cx="380" cy="160" r="3" class="neural-node active"/>
          <circle cx="420" cy="200" r="2" class="neural-node"/>
          <circle cx="350" cy="220" r="2.5" class="neural-node active"/>
          <circle cx="300" cy="280" r="2" class="neural-node"/>
          <circle cx="450" cy="320" r="2.5" class="neural-node active"/>
          <circle cx="380" cy="350" r="2" class="neural-node"/>
          <circle cx="320" cy="380" r="2" class="neural-node"/>
        </g>

        <!-- PREFRONTAL CORTEX module (top left HUD panel) - Executive function -->
        <g class="brain-module module-mind ${this.getModuleClass('mind')}" data-module="mind">
          <!-- HUD bracket frame -->
          <path class="hud-bracket" d="M10,40 L10,20 L30,20"/>
          <path class="hud-bracket" d="M170,20 L190,20 L190,40"/>
          <path class="hud-bracket" d="M10,100 L10,120 L30,120"/>
          <path class="hud-bracket" d="M170,120 L190,120 L190,100"/>
          <rect x="15" y="25" width="170" height="90" class="module-shape"/>
          <text x="100" y="50" class="module-label">PREFRONTAL</text>
          <text x="100" y="70" class="module-sublabel">[ EXECUTIVE ]</text>
          <text x="100" y="100" class="module-stat">${this.moduleStats.mind.status.toUpperCase()}</text>
          <!-- Connection line to brain -->
          <line x1="190" y1="70" x2="280" y2="150" class="circuit-connection"/>
          <circle cx="280" cy="150" r="5" class="connection-node"/>
        </g>

        <!-- LIMBIC SYSTEM module (top right HUD panel) - Emotion/motivation -->
        <g class="brain-module module-soul ${this.getModuleClass('soul')}" data-module="soul">
          <path class="hud-bracket" d="M610,40 L610,20 L630,20"/>
          <path class="hud-bracket" d="M770,20 L790,20 L790,40"/>
          <path class="hud-bracket" d="M610,100 L610,120 L630,120"/>
          <path class="hud-bracket" d="M770,120 L790,120 L790,100"/>
          <rect x="615" y="25" width="170" height="90" class="module-shape"/>
          <text x="700" y="50" class="module-label">LIMBIC</text>
          <text x="700" y="70" class="module-sublabel">[ EMOTION ]</text>
          <text x="700" y="100" class="module-stat">${this.moduleStats.soul.mood?.toUpperCase() || 'NEUTRAL'}</text>
          <line x1="610" y1="70" x2="520" y2="180" class="circuit-connection"/>
          <circle cx="520" cy="180" r="5" class="connection-node"/>
        </g>

        <!-- HIPPOCAMPUS module (left side HUD panel) - Memory -->
        <g class="brain-module module-hippocampus ${this.getModuleClass('hippocampus')}" data-module="hippocampus">
          <path class="hud-bracket" d="M5,220 L5,200 L25,200"/>
          <path class="hud-bracket" d="M165,200 L185,200 L185,220"/>
          <path class="hud-bracket" d="M5,330 L5,350 L25,350"/>
          <path class="hud-bracket" d="M165,350 L185,350 L185,330"/>
          <rect x="10" y="205" width="170" height="140" class="module-shape"/>
          <text x="95" y="230" class="module-label">HIPPOCAMPUS</text>
          <text x="95" y="250" class="module-sublabel">[ MEMORY ]</text>
          <!-- Memory bar visualization -->
          <rect x="25" y="268" width="140" height="8" fill="rgba(0,20,30,0.8)" stroke="rgba(0,212,255,0.3)" rx="2"/>
          <rect x="25" y="268" width="105" height="8" fill="rgba(0,212,255,0.6)" rx="2"/>
          <text x="95" y="300" class="module-stat">${this.moduleStats.hippocampus.memoryCount}</text>
          <text x="95" y="325" class="module-sublabel">${this.moduleStats.hippocampus.ltmSize || '0 MB'}</text>
          <line x1="185" y1="275" x2="300" y2="320" class="circuit-connection"/>
          <circle cx="300" cy="320" r="5" class="connection-node"/>
        </g>

        <!-- MOTOR CORTEX module (right side HUD panel) - Actions/tools -->
        <g class="brain-module module-body ${this.getModuleClass('body')}" data-module="body">
          <path class="hud-bracket" d="M615,220 L615,200 L635,200"/>
          <path class="hud-bracket" d="M775,200 L795,200 L795,220"/>
          <path class="hud-bracket" d="M615,330 L615,350 L635,350"/>
          <path class="hud-bracket" d="M775,350 L795,350 L795,330"/>
          <rect x="620" y="205" width="170" height="140" class="module-shape"/>
          <text x="705" y="230" class="module-label">MOTOR CORTEX</text>
          <text x="705" y="250" class="module-sublabel">[ ACTIONS ]</text>
          <!-- Tool slots visualization -->
          <g class="tool-slots">
            ${[0,1,2,3,4,5].map(i => `
              <rect x="${640 + (i % 3) * 35}" y="${268 + Math.floor(i/3) * 28}"
                    width="28" height="20" rx="2"
                    class="tool-slot ${i < this.moduleStats.body.toolsAvailable ? 'active' : ''}"
                    fill="${i < this.moduleStats.body.toolsAvailable ? 'rgba(0,212,255,0.3)' : 'rgba(0,20,30,0.5)'}"
                    stroke="rgba(0,212,255,0.4)"/>
            `).join('')}
          </g>
          <text x="705" y="340" class="module-stat">${this.moduleStats.body.toolsAvailable} ACTIVE</text>
          <line x1="615" y1="275" x2="530" y2="320" class="circuit-connection"/>
          <circle cx="530" cy="320" r="5" class="connection-node"/>
        </g>

        <!-- CNS module (bottom HUD panel) - Integration -->
        <g class="brain-module module-cns ${this.getModuleClass('cns')}" data-module="cns">
          <path class="hud-bracket" d="M300,480 L300,460 L320,460"/>
          <path class="hud-bracket" d="M480,460 L500,460 L500,480"/>
          <path class="hud-bracket" d="M300,570 L300,590 L320,590"/>
          <path class="hud-bracket" d="M480,590 L500,590 L500,570"/>
          <rect x="305" y="465" width="190" height="120" class="module-shape"/>
          <text x="400" y="495" class="module-label">CNS</text>
          <text x="400" y="515" class="module-sublabel">[ INTEGRATION ]</text>
          <!-- Activity waveform -->
          <polyline class="waveform" points="320,550 345,540 370,555 395,530 420,548 445,525 470,545 490,535"/>
          <text x="400" y="575" class="module-stat">${this.moduleStats.cns.connections} CONN</text>
          <line x1="400" y1="460" x2="450" y2="400" class="circuit-connection"/>
          <circle cx="450" cy="400" r="5" class="connection-node"/>
        </g>

        <!-- Status readout (bottom left) -->
        <g class="hud-readout" transform="translate(20, 450)">
          <text x="0" y="20" class="readout-label">SYS.STATUS</text>
          <text x="0" y="40" class="readout-value">${this.persona?.status?.toUpperCase() || 'OFFLINE'}</text>
          <text x="0" y="70" class="readout-label">UPTIME</text>
          <text x="0" y="90" class="readout-value">--:--:--</text>
        </g>

        <!-- Data readout (bottom right) -->
        <g class="hud-readout" transform="translate(680, 450)">
          <text x="0" y="20" class="readout-label">NEURAL.LOAD</text>
          <text x="0" y="40" class="readout-value">42%</text>
          <text x="0" y="70" class="readout-label">PROC.QUEUE</text>
          <text x="0" y="90" class="readout-value">0</text>
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
    gap: 16px;
  }

  .brain-header {
    display: flex;
    justify-content: flex-end;
  }

  .persona-status {
    padding: 4px 12px;
    border-radius: 2px;
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    font-family: 'JetBrains Mono', monospace;
    letter-spacing: 1px;
  }

  .persona-status.status-online {
    background: rgba(0, 255, 100, 0.15);
    color: #00ff64;
    border: 1px solid rgba(0, 255, 100, 0.4);
  }

  .persona-status.status-offline {
    background: rgba(255, 80, 80, 0.15);
    color: #ff5050;
    border: 1px solid rgba(255, 80, 80, 0.4);
  }

  .persona-status.status-idle {
    background: rgba(255, 204, 0, 0.15);
    color: #ffcc00;
    border: 1px solid rgba(255, 204, 0, 0.4);
  }

  /* Brain SVG Visualization */
  .brain-visualization {
    display: flex;
    justify-content: center;
    padding: 10px;
    flex: 1;
  }

  .brain-svg {
    width: 100%;
    max-width: 900px;
    height: auto;
  }

  /* LOW-POLY BRAIN FACETS */
  .brain-facet {
    stroke: rgba(0, 212, 255, 0.3);
    stroke-width: 0.5;
    transition: all 0.3s ease;
  }

  .brain-facet.f-bright {
    fill: rgba(80, 200, 255, 0.5);
  }

  .brain-facet.f-light {
    fill: rgba(40, 160, 220, 0.45);
  }

  .brain-facet.f-med {
    fill: rgba(20, 120, 180, 0.4);
  }

  .brain-facet.f-dark {
    fill: rgba(10, 80, 140, 0.35);
  }

  .brain-edge {
    filter: url(#glow-subtle);
  }

  /* Neural activity nodes */
  .neural-node {
    fill: rgba(0, 212, 255, 0.4);
    stroke: none;
  }

  .neural-node.active {
    fill: #00d4ff;
    filter: url(#glow);
    animation: node-pulse 2s ease-in-out infinite;
  }

  @keyframes node-pulse {
    0%, 100% { opacity: 0.6; r: 3; }
    50% { opacity: 1; r: 4; }
  }

  /* HUD Module Panels */
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

  .hud-bracket {
    fill: none;
    stroke: rgba(0, 212, 255, 0.6);
    stroke-width: 2;
    transition: all 0.3s ease;
  }

  .brain-module:hover .hud-bracket {
    stroke: #00d4ff;
  }

  .module-shape {
    fill: rgba(0, 15, 25, 0.85);
    stroke: rgba(0, 212, 255, 0.3);
    stroke-width: 1;
    transition: all 0.3s ease;
  }

  .brain-module:hover .module-shape,
  .brain-module.selected .module-shape {
    fill: rgba(0, 30, 50, 0.9);
    stroke: rgba(0, 212, 255, 0.6);
  }

  .brain-module.status-active .module-shape {
    stroke: rgba(0, 255, 100, 0.5);
  }

  .brain-module.status-active .hud-bracket {
    stroke: #00ff64;
  }

  .brain-module.status-error .module-shape {
    stroke: rgba(255, 80, 80, 0.5);
    animation: error-flash 1s ease infinite;
  }

  @keyframes error-flash {
    0%, 100% { stroke-opacity: 0.5; }
    50% { stroke-opacity: 1; }
  }

  .module-label {
    fill: rgba(255, 255, 255, 0.95);
    font-size: 14px;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
    letter-spacing: 2px;
  }

  .module-sublabel {
    fill: rgba(0, 212, 255, 0.6);
    font-size: 9px;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
    letter-spacing: 1px;
  }

  .module-stat {
    fill: #00d4ff;
    font-size: 11px;
    font-weight: 500;
    font-family: 'JetBrains Mono', monospace;
    text-anchor: middle;
    pointer-events: none;
  }

  /* Circuit connections */
  .circuit-connection {
    stroke: rgba(0, 212, 255, 0.4);
    stroke-width: 1.5;
    stroke-dasharray: 6 4;
    animation: data-pulse 1.5s linear infinite;
  }

  .connection-node {
    fill: rgba(0, 15, 25, 0.9);
    stroke: #00d4ff;
    stroke-width: 2;
    filter: url(#glow-subtle);
  }

  @keyframes data-pulse {
    0% { stroke-dashoffset: 0; }
    100% { stroke-dashoffset: -20; }
  }

  /* Waveform animation */
  .waveform {
    fill: none;
    stroke: #00d4ff;
    stroke-width: 1.5;
    stroke-linecap: round;
    animation: waveform-anim 2s ease-in-out infinite;
  }

  @keyframes waveform-anim {
    0%, 100% { opacity: 0.6; }
    50% { opacity: 1; }
  }

  /* HUD Readouts */
  .readout-label {
    fill: rgba(0, 212, 255, 0.5);
    font-size: 9px;
    font-family: 'JetBrains Mono', monospace;
    letter-spacing: 1px;
  }

  .readout-value {
    fill: rgba(255, 255, 255, 0.9);
    font-size: 12px;
    font-family: 'JetBrains Mono', monospace;
    font-weight: 500;
  }

  /* Module Detail View */
  .module-details {
    margin-top: 16px;
  }

  .module-detail-view {
    background: rgba(0, 15, 25, 0.85);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    padding: 16px;
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid rgba(0, 212, 255, 0.15);
  }

  .detail-header h3 {
    margin: 0;
    color: #00d4ff;
    font-size: 14px;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    letter-spacing: 2px;
  }

  .btn-small {
    padding: 4px 10px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .stat-row {
    display: flex;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid rgba(0, 212, 255, 0.05);
  }

  .stat-label {
    color: rgba(255, 255, 255, 0.5);
    font-size: 11px;
    font-family: 'JetBrains Mono', monospace;
  }

  .stat-value {
    color: white;
    font-size: 11px;
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
    gap: 32px;
    padding: 12px;
    background: rgba(0, 15, 25, 0.6);
    border: 1px solid rgba(0, 212, 255, 0.15);
    border-radius: 2px;
  }

  .stat-item {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .stat-icon {
    font-size: 12px;
    opacity: 0.7;
  }

  .stat-text {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.7);
    font-family: 'JetBrains Mono', monospace;
    letter-spacing: 0.5px;
  }

  /* Module overview (hidden by default with new design) */
  .module-overview {
    display: none;
  }
`;

// Register the custom element
customElements.define('persona-brain-widget', PersonaBrainWidget);
