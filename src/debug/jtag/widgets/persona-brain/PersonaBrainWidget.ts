/**
 * PersonaBrainWidget - Per-persona cognitive visualization HUD
 *
 * Displays a stylized brain diagram with clickable regions for each
 * cognitive module (PrefrontalCortex, LimbicSystem, Hippocampus, MotorCortex, CNS).
 *
 * Features:
 * - Low-poly brain visualization with module status indicators
 * - Activity feed showing real-time cognitive events
 * - Issues panel with click-through to relevant logs
 * - Click any module to view details and access logs
 *
 * Entry points:
 * - Users & Agents sidebar click
 * - Diagnostics panel persona click
 * - @mention click in chat
 * - content/open with contentType='persona'
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { unsafeSVG } from 'lit/directives/unsafe-svg.js';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import { Commands } from '../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { ContentService } from '../../system/state/ContentService';
import { LogToggle, type LogToggleState } from './components/LogToggle';
import { styles as personaBrainStyles } from './styles/persona-brain-widget.styles';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { ALL_PANEL_STYLES } from '../shared/styles';

import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { LogsConfig } from '../../commands/logs/config/shared/LogsConfigTypes';
import { LogsList } from '../../commands/logs/list/shared/LogsListTypes';
import { AIStatus } from '../../commands/ai/status/shared/AIStatusTypes';
interface PersonaData {
  id: UUID;
  uniqueId: string;
  displayName: string;
  status: 'online' | 'offline' | 'idle';
  type: string;
}

interface ModuleStats {
  prefrontal: { status: 'active' | 'idle' | 'error'; lastActivity?: string };
  limbic: { status: 'active' | 'idle' | 'error'; mood?: string };
  hippocampus: { status: 'active' | 'idle' | 'error'; memoryCount: number; ltmSize?: string };
  motorCortex: { status: 'active' | 'idle' | 'error'; toolsAvailable: number };
  cns: { status: 'active' | 'idle' | 'error'; connections: number };
}

interface ActivityEvent {
  timestamp: Date;
  type: 'thought' | 'action' | 'memory' | 'error' | 'tool';
  module: string;
  message: string;
  severity?: 'info' | 'warn' | 'error';
  details?: any;
}

interface Issue {
  id: string;
  module: string;
  type: 'error' | 'warning';
  message: string;
  timestamp: Date;
  details?: any;
}

interface LoggingConfigState {
  enabled: boolean;
  categories: string[];
}

export class PersonaBrainWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(ALL_PANEL_STYLES),
    unsafeCSS(personaBrainStyles)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private personaId: string = '';
  @reactive() private persona: PersonaData | null = null;
  @reactive() private moduleStats: ModuleStats = {
    prefrontal: { status: 'idle' },
    limbic: { status: 'idle' },
    hippocampus: { status: 'idle', memoryCount: 0 },
    motorCortex: { status: 'idle', toolsAvailable: 0 },
    cns: { status: 'idle', connections: 0 }
  };
  @reactive() private isLoading = true;
  @reactive() private selectedModule: string | null = null;
  @reactive() private activityFeed: ActivityEvent[] = [];
  @reactive() private issues: Issue[] = [];
  @reactive() private loggingConfig: LoggingConfigState = { enabled: false, categories: [] };

  // Non-reactive state
  private availableLogs: Set<string> = new Set();
  private panelTitle: string = 'Persona';
  private panelSubtitle: string = 'Cognitive System View';

  constructor() {
    super({
      widgetName: 'PersonaBrainWidget'
    });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Get persona ID from content item (check both attribute formats)
    this.personaId = this.getAttribute('entity-id') ||
                     this.getAttribute('data-entity-id') ||
                     (this as any).entityId ||
                     'helper'; // Default for testing

    await this.loadPersonaData();
    this.emitPositronContext();
  }

  /**
   * Called by MainWidget when this widget is activated with a new entityId.
   * Implements clear/populate/query pattern for instant hydration.
   */
  public async onActivate(entityId?: string, metadata?: Record<string, unknown>): Promise<void> {
    // Check if this is the same persona (handles both UUID and uniqueId formats)
    const isSameEntity = entityId && (
      entityId === this.personaId ||
      entityId === this.persona?.id ||
      entityId === this.persona?.uniqueId
    );

    // SAME ENTITY? Just refresh, don't clear
    if (isSameEntity && this.persona) {
      this.requestUpdate();
      return;
    }

    // Different persona - update tracking
    if (entityId) {
      this.setAttribute('entity-id', entityId);
      this.personaId = entityId;
    }

    // CLEAR old state (prevents stale data flash)
    this.persona = null;
    this.isLoading = true;
    this.selectedModule = null;
    this.activityFeed = [];
    this.issues = [];

    // POPULATE with passed entity (instant hydration)
    const preloaded = metadata?.entity as PersonaData;
    if (preloaded) {
      this.persona = preloaded;
      this.isLoading = false;
      this.requestUpdate(); // Render immediately with what we have
      // Still load additional brain-specific data (modules, activity, issues)
      await this.loadBrainData();
      return;
    }

    // QUERY - only if no metadata (e.g., direct URL navigation)
    await this.loadPersonaData();
    this.emitPositronContext();
  }

  /**
   * Emit Positron context for AI awareness
   * NOTE: Removed emit - MainWidget handles context. Widgets should RECEIVE, not emit.
   */
  private emitPositronContext(): void {
    // No-op - context cascade fix
  }

  /**
   * Load only brain-specific data when persona is already hydrated from metadata.
   * Skips the persona query since we already have it.
   */
  private async loadBrainData(): Promise<void> {
    if (!this.persona) return;

    // Update panel titles from hydrated data
    this.panelTitle = this.persona.displayName;
    this.panelSubtitle = `@${this.persona.uniqueId} - Cognitive System View`;

    // Normalize personaId to uniqueId for log paths
    this.personaId = this.persona.uniqueId;

    // Load brain-specific data in parallel
    await Promise.all([
      this.loadModuleStats(),
      this.loadLoggingConfig(),
      this.loadAvailableLogs()
    ]);

    this.requestUpdate();
  }

  private async loadPersonaData(): Promise<void> {
    this.isLoading = true;
    this.requestUpdate();

    try {
      // Load persona info - handle both uniqueId ("helper") and UUID formats
      // The entityId from MainWidget may be either format (see Phase 1.x technical debt)
      const isUUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(this.personaId);
      const filter = isUUID
        ? { id: this.personaId }
        : { uniqueId: this.personaId };

      const result = await DataList.execute({
        collection: 'users',
        filter,
        limit: 1
      });

      if (result.success && result.items?.length > 0) {
        const user = result.items[0] as BaseEntity & Record<string, any>;
        this.persona = {
          id: user.id,
          uniqueId: user.uniqueId,
          displayName: user.displayName,
          status: user.status || 'offline',
          type: user.type
        };

        // Normalize personaId to uniqueId for log paths and other lookups
        this.personaId = user.uniqueId;

        this.panelTitle = user.displayName;
        this.panelSubtitle = `@${user.uniqueId} - Cognitive System View`;

        // Load module stats, logging config, and available logs
        await Promise.all([
          this.loadModuleStats(),
          this.loadLoggingConfig(),
          this.loadAvailableLogs()
        ]);
      }
    } catch (error) {
      console.error('PersonaBrainWidget: Error loading persona:', error);
    }

    this.isLoading = false;
    this.requestUpdate();
  }

  /**
   * Load logging configuration for this persona
   */
  private async loadLoggingConfig(): Promise<void> {
    try {
      const result = await LogsConfig.execute({
        persona: this.personaId
      } as any) as any;

      if (result.success && result.personaConfig) {
        this.loggingConfig = {
          enabled: result.personaConfig.enabled,
          categories: result.personaConfig.categories || []
        };
      }
    } catch (error) {
      console.warn('PersonaBrainWidget: Error loading logging config:', error);
    }
  }

  /**
   * Load available log files for this persona
   * This determines which log buttons should be enabled
   */
  private async loadAvailableLogs(): Promise<void> {
    try {
      const result = await LogsList.execute({
        personaUniqueId: this.personaId
      } as any) as any;

      this.availableLogs.clear();
      if (result.success && result.logs) {
        for (const log of result.logs) {
          // logType is the log name without path (e.g., 'tools', 'cns', 'hippocampus')
          if (log.logType) {
            this.availableLogs.add(log.logType);
          }
        }
      }
    } catch (error) {
      console.warn('PersonaBrainWidget: Error loading available logs:', error);
    }
  }

  /**
   * Toggle logging for the entire persona
   */
  private async togglePersonaLogging(enabled: boolean): Promise<void> {
    try {
      const result = await LogsConfig.execute({
        persona: this.personaId,
        action: enabled ? 'enable' : 'disable'
      } as any) as any;

      if (result.success && result.personaConfig) {
        this.loggingConfig = {
          enabled: result.personaConfig.enabled,
          categories: result.personaConfig.categories || []
        };
        this.requestUpdate();
      }
    } catch (error) {
      console.error('PersonaBrainWidget: Error toggling logging:', error);
    }
  }

  /**
   * Toggle logging for a specific category
   * Only works when global logging is enabled
   */
  private async toggleCategoryLogging(category: string): Promise<void> {
    // Block if global logging is off
    if (!this.loggingConfig.enabled) {
      this.verbose() && console.log('PersonaBrainWidget: Global logging is off, enable it first');
      return;
    }

    const isEnabled = this.isCategoryEnabled(category);

    try {
      const result = await LogsConfig.execute({
        persona: this.personaId,
        action: isEnabled ? 'disable' : 'enable',
        category
      } as any) as any;

      if (result.success && result.personaConfig) {
        this.loggingConfig = {
          enabled: result.personaConfig.enabled,
          categories: result.personaConfig.categories || []
        };
        this.requestUpdate();
      }
    } catch (error) {
      console.error('PersonaBrainWidget: Error toggling category logging:', error);
    }
  }

  /**
   * Check if a category is enabled for logging
   */
  private isCategoryEnabled(category: string): boolean {
    if (!this.loggingConfig.enabled) return false;
    // Empty categories or '*' means all enabled
    if (this.loggingConfig.categories.length === 0) return true;
    if (this.loggingConfig.categories.includes('*')) return true;
    return this.loggingConfig.categories.includes(category);
  }

  /**
   * Render a log toggle button for a module
   * @param category - The logging category to toggle
   * @param x - X position for the toggle
   * @param y - Y position for the toggle
   */
  private renderLogToggle(category: string, x: number, y: number): string {
    const globalEnabled = this.loggingConfig.enabled;
    const categoryEnabled = this.isCategoryEnabled(category);
    const disabled = !globalEnabled;

    return `
      <g class="module-log-toggle ${categoryEnabled ? 'enabled' : ''} ${disabled ? 'disabled' : ''}" data-category="${category}">
        <rect x="${x}" y="${y}" width="22" height="16" rx="2" class="log-toggle-bg"/>
        <text x="${x + 11}" y="${y + 12}" class="log-toggle-icon">${categoryEnabled ? '📝' : '📋'}</text>
      </g>
    `;
  }

  private async loadModuleStats(): Promise<void> {
    if (!this.persona) return;

    try {
      // Fetch all stats in parallel for better performance
      const [aiStatusResult, memoryCountResult, toolLogsResult] = await Promise.all([
        // 1. Get AI status (model, provider, health)
        AIStatus.execute({} as any) as Promise<any>,
        // 2. Get memory count
        DataList.execute({
          collection: 'memories',
          filter: { personaId: this.persona.id },
          limit: 1  // We just need the count
        }),
        // 3. Get tool execution count
        DataList.execute({
          collection: 'tool_execution_logs',
          filter: { personaId: this.persona.id },
          limit: 1
        })
      ]);

      // Parse AI status
      let aiStatus: any = null;
      if (aiStatusResult?.success && aiStatusResult.personas) {
        aiStatus = aiStatusResult.personas.find(
          (p: any) => p.userId === this.persona?.id || p.uniqueId === this.personaId
        );
      }

      // Calculate memory stats
      const memoryCount = memoryCountResult?.count || 0;
      const ltmSizeKB = memoryCount * 0.5; // Rough estimate: ~0.5KB per memory
      const ltmSize = ltmSizeKB > 1024
        ? `${(ltmSizeKB / 1024).toFixed(1)} MB`
        : `${ltmSizeKB.toFixed(0)} KB`;

      // Tool execution stats
      const toolCount = toolLogsResult?.count || 0;

      // Update module stats with real data
      this.moduleStats = {
        prefrontal: {
          status: aiStatus?.isInitialized ? 'active' : 'idle',
          lastActivity: aiStatus?.status === 'healthy' ? 'Online and ready' : 'Waiting...'
        },
        limbic: {
          status: aiStatus?.hasWorker ? 'active' : 'idle',
          mood: this.inferMood(memoryCount)
        },
        hippocampus: {
          status: memoryCount > 0 ? 'active' : 'idle',
          memoryCount,
          ltmSize
        },
        motorCortex: {
          status: toolCount > 0 ? 'active' : 'idle',
          toolsAvailable: toolCount > 0 ? 12 : 0  // Tool count is hardcoded for now
        },
        cns: {
          status: aiStatus?.isSubscribed ? 'active' : 'idle',
          connections: aiStatus?.isSubscribed ? 5 : 0
        }
      };

      // Store additional persona info
      if (aiStatus) {
        (this.persona as any).provider = aiStatus.provider;
        (this.persona as any).model = aiStatus.model;
        (this.persona as any).health = aiStatus.status;
      }

    } catch (error) {
      console.error('PersonaBrainWidget: Error loading stats:', error);
      // Keep default placeholder stats on error
      this.moduleStats = {
        prefrontal: { status: 'error', lastActivity: 'Failed to load' },
        limbic: { status: 'error', mood: 'unknown' },
        hippocampus: { status: 'error', memoryCount: 0 },
        motorCortex: { status: 'error', toolsAvailable: 0 },
        cns: { status: 'error', connections: 0 }
      };
      // Add issue for error tracking
      this.issues.push({
        id: `load-error-${Date.now()}`,
        module: 'system',
        type: 'error',
        message: 'Failed to load module stats',
        timestamp: new Date(),
        details: error
      });
    }
  }

  /**
   * Infer mood based on memory count and activity
   */
  private inferMood(memoryCount: number): string {
    if (memoryCount > 1000) return 'experienced';
    if (memoryCount > 500) return 'curious';
    if (memoryCount > 100) return 'learning';
    if (memoryCount > 0) return 'exploring';
    return 'fresh';
  }

  /**
   * Truncate model name for display in HUD
   */
  private truncateModel(model: string): string {
    if (model.length <= 12) return model.toUpperCase();
    // Shorten common model names
    return model
      .replace('llama3.2:', 'L3.2:')
      .replace('llama-3.1-', 'L3.1-')
      .replace('claude-', 'C-')
      .replace('-instant', '-I')
      .toUpperCase()
      .substring(0, 12);
  }

  protected override renderContent(): TemplateResult {
    // Panel layout wrapper (from BasePanelWidget pattern)
    return html`
      <div class="panel-layout">
        <div class="panel-main">
          <div class="panel-container">
            <div class="panel-header">
              <h1 class="panel-title">${this.panelTitle}</h1>
              ${this.panelSubtitle ? html`<p class="panel-subtitle">${this.panelSubtitle}</p>` : ''}
            </div>
            ${this.renderBrainContent()}
          </div>
        </div>
      </div>
    `;
  }

  private renderBrainContent(): TemplateResult {
    if (this.isLoading) {
      return html`
        <div class="loading">
          <div class="loading-spinner"></div>
          <p>Loading persona data...</p>
        </div>
      `;
    }

    if (!this.persona) {
      return html`<div class="info-box error">Persona not found</div>`;
    }

    return html`
      <div class="brain-container">
        <div class="brain-header">
          <button class="log-toggle ${this.loggingConfig.enabled ? 'enabled' : ''}"
                  @click=${this.handleToggleLogging}
                  title="${this.loggingConfig.enabled ? 'Logging ON - Click to disable' : 'Logging OFF - Click to enable'}">
            ${this.loggingConfig.enabled ? '📝' : '📋'}
          </button>
          <div class="persona-status status-${this.persona.status}">${this.persona.status}</div>
        </div>

        <div class="brain-main">
          <div class="brain-visualization">
            ${unsafeHTML(this.renderBrainSVG())}
          </div>

          <div class="brain-sidebar">
            ${unsafeHTML(this.renderActivityFeed())}
            ${unsafeHTML(this.renderIssuesPanel())}
          </div>
        </div>

        ${this.selectedModule ? html`<div class="module-details">${unsafeHTML(this.renderModuleDetails())}</div>` : ''}

        <div class="brain-stats">
          ${unsafeHTML(this.renderStats())}
        </div>
      </div>
    `;
  }

  // === EVENT HANDLERS (bound to Lit events) ===

  private handleToggleLogging = async (): Promise<void> => {
    await this.togglePersonaLogging(!this.loggingConfig.enabled);
  };

  private handleModuleClick = (module: string): void => {
    this.selectModule(module);
  };

  private handleBackClick = (): void => {
    this.selectedModule = null;
    this.requestUpdate();
  };

  private handleViewLog = async (logFile: string): Promise<void> => {
    if (this.persona) {
      await this.openLogViewer(logFile);
    }
  };

  private handleOpenLog = async (logType: string): Promise<void> => {
    await this.openLogViewer(logType);
  };

  private handleCategoryToggle = async (category: string): Promise<void> => {
    await this.toggleCategoryLogging(category);
  };

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
        <g class="brain-module module-prefrontal ${this.getModuleClass('prefrontal')}" data-module="prefrontal">
          <!-- HUD bracket frame -->
          <path class="hud-bracket" d="M10,40 L10,20 L30,20"/>
          <path class="hud-bracket" d="M170,20 L190,20 L190,40"/>
          <path class="hud-bracket" d="M10,100 L10,120 L30,120"/>
          <path class="hud-bracket" d="M170,120 L190,120 L190,100"/>
          <rect x="15" y="25" width="170" height="90" class="module-shape"/>
          <text x="100" y="50" class="module-label">PREFRONTAL</text>
          <text x="100" y="70" class="module-sublabel">[ EXECUTIVE ]</text>
          <text x="100" y="100" class="module-stat">${this.moduleStats.prefrontal.status.toUpperCase()}</text>
          ${this.renderLogToggle('cognition', 160, 30)}
          <!-- Connection line to brain -->
          <line x1="190" y1="70" x2="280" y2="150" class="circuit-connection"/>
          <circle cx="280" cy="150" r="5" class="connection-node"/>
        </g>

        <!-- LIMBIC SYSTEM module (top right HUD panel) - Emotion/motivation -->
        <g class="brain-module module-limbic ${this.getModuleClass('limbic')}" data-module="limbic">
          <path class="hud-bracket" d="M610,40 L610,20 L630,20"/>
          <path class="hud-bracket" d="M770,20 L790,20 L790,40"/>
          <path class="hud-bracket" d="M610,100 L610,120 L630,120"/>
          <path class="hud-bracket" d="M770,120 L790,120 L790,100"/>
          <rect x="615" y="25" width="170" height="90" class="module-shape"/>
          <text x="700" y="50" class="module-label">LIMBIC</text>
          <text x="700" y="70" class="module-sublabel">[ EMOTION ]</text>
          <text x="700" y="100" class="module-stat">${this.moduleStats.limbic.mood?.toUpperCase() || 'NEUTRAL'}</text>
          ${this.renderLogToggle('user', 760, 30)}
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
          ${this.renderLogToggle('hippocampus', 155, 210)}
          <!-- Memory bar visualization -->
          <rect x="25" y="268" width="140" height="8" fill="rgba(0,20,30,0.8)" stroke="rgba(0,212,255,0.3)" rx="2"/>
          <rect x="25" y="268" width="105" height="8" fill="rgba(0,212,255,0.6)" rx="2"/>
          <text x="95" y="300" class="module-stat">${this.moduleStats.hippocampus.memoryCount}</text>
          <text x="95" y="325" class="module-sublabel">${this.moduleStats.hippocampus.ltmSize || '0 MB'}</text>
          <line x1="185" y1="275" x2="300" y2="320" class="circuit-connection"/>
          <circle cx="300" cy="320" r="5" class="connection-node"/>
        </g>

        <!-- MOTOR CORTEX module (right side HUD panel) - Actions/tools -->
        <g class="brain-module module-motorCortex ${this.getModuleClass('motorCortex')}" data-module="motorCortex">
          <path class="hud-bracket" d="M615,220 L615,200 L635,200"/>
          <path class="hud-bracket" d="M775,200 L795,200 L795,220"/>
          <path class="hud-bracket" d="M615,330 L615,350 L635,350"/>
          <path class="hud-bracket" d="M775,350 L795,350 L795,330"/>
          <rect x="620" y="205" width="170" height="140" class="module-shape"/>
          <text x="705" y="230" class="module-label">MOTOR CORTEX</text>
          <text x="705" y="250" class="module-sublabel">[ ACTIONS ]</text>
          ${this.renderLogToggle('adapters', 765, 210)}
          <!-- Tool slots visualization -->
          <g class="tool-slots">
            ${[0,1,2,3,4,5].map(i => `
              <rect x="${640 + (i % 3) * 35}" y="${268 + Math.floor(i/3) * 28}"
                    width="28" height="20" rx="2"
                    class="tool-slot ${i < this.moduleStats.motorCortex.toolsAvailable ? 'active' : ''}"
                    fill="${i < this.moduleStats.motorCortex.toolsAvailable ? 'rgba(0,212,255,0.3)' : 'rgba(0,20,30,0.5)'}"
                    stroke="rgba(0,212,255,0.4)"/>
            `).join('')}
          </g>
          <text x="705" y="340" class="module-stat">${this.moduleStats.motorCortex.toolsAvailable} ACTIVE</text>
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
          ${this.renderLogToggle('cognition', 470, 470)}
          <!-- Activity waveform -->
          <polyline class="waveform" points="320,550 345,540 370,555 395,530 420,548 445,525 470,545 490,535"/>
          <text x="400" y="575" class="module-stat">${this.moduleStats.cns.connections} CONN</text>
          <line x1="400" y1="460" x2="450" y2="400" class="circuit-connection"/>
          <circle cx="450" cy="400" r="5" class="connection-node"/>
        </g>

        <!-- Status readout (bottom left) -->
        <g class="hud-readout" transform="translate(20, 450)">
          <text x="0" y="20" class="readout-label">SYS.STATUS</text>
          <text x="0" y="40" class="readout-value">${((this.persona as any)?.health || this.persona?.status || 'OFFLINE').toUpperCase()}</text>
          <text x="0" y="70" class="readout-label">PROVIDER</text>
          <text x="0" y="90" class="readout-value">${((this.persona as any)?.provider || 'N/A').toUpperCase()}</text>
        </g>

        <!-- Data readout (bottom right) -->
        <g class="hud-readout" transform="translate(680, 450)">
          <text x="0" y="20" class="readout-label">MODEL</text>
          <text x="0" y="40" class="readout-value">${this.truncateModel((this.persona as any)?.model || 'N/A')}</text>
          <text x="0" y="70" class="readout-label">MEMORIES</text>
          <text x="0" y="90" class="readout-value">${this.moduleStats.hippocampus.memoryCount.toLocaleString()}</text>
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
      { id: 'prefrontal', name: 'Prefrontal', desc: 'Executive function', icon: '🧠' },
      { id: 'limbic', name: 'Limbic', desc: 'Emotion & motivation', icon: '💜' },
      { id: 'hippocampus', name: 'Hippocampus', desc: 'Memory & learning', icon: '💾' },
      { id: 'motorCortex', name: 'Motor Cortex', desc: 'Actions & tools', icon: '⚡' },
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
    // Map module name to log type (motorCortex -> motor-cortex)
    const logTypeMap: Record<string, string> = {
      prefrontal: 'prefrontal',
      limbic: 'limbic',
      hippocampus: 'hippocampus',
      motorCortex: 'motor-cortex',
      cns: 'cns'
    };
    const logType = logTypeMap[module] || module;

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
          <button class="btn btn-primary" data-action="view-log" data-log="${logType}">
            View ${logType}.log
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
      case 'prefrontal':
        return `
          <div class="stat-row">
            <span class="stat-label">Last Activity</span>
            <span class="stat-value">${stats?.lastActivity || 'None'}</span>
          </div>
        `;
      case 'limbic':
        return `
          <div class="stat-row">
            <span class="stat-label">Current Mood</span>
            <span class="stat-value">${stats?.mood || 'neutral'}</span>
          </div>
        `;
      case 'motorCortex':
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
    // Helper to generate stat item - only clickable if log exists
    const statItem = (logType: string, icon: string, text: string, title: string) => {
      const hasLog = this.availableLogs.has(logType);
      const classes = hasLog ? 'stat-item clickable' : 'stat-item disabled';
      const attrs = hasLog ? `data-action="open-log" data-log="${logType}"` : '';
      const tooltip = hasLog ? title : `${title} (no log yet)`;
      return `
        <div class="${classes}" ${attrs} title="${tooltip}">
          <span class="stat-icon">${icon}</span>
          <span class="stat-text">${text}</span>
        </div>
      `;
    };

    return `
      <div class="stats-bar">
        ${statItem('hippocampus', '💾', this.moduleStats.hippocampus.ltmSize || '0 MB', 'View memory log')}
        ${statItem('tools', '⚡', `${this.moduleStats.motorCortex.toolsAvailable} tools`, 'View tools log')}
        ${statItem('cns', '🔗', `${this.moduleStats.cns.connections} conn`, 'View CNS log')}
        ${this.issues.length > 0 ? `
        <div class="stat-item issue-indicator" data-action="show-issues">
          <span class="stat-icon">⚠️</span>
          <span class="stat-text issue-count">${this.issues.length} issues</span>
        </div>
        ` : ''}
      </div>
    `;
  }

  /**
   * Render activity feed panel showing recent cognitive events
   */
  private renderActivityFeed(): string {
    if (this.activityFeed.length === 0) {
      return `
        <div class="activity-feed">
          <div class="feed-header">
            <span class="feed-title">ACTIVITY STREAM</span>
            <span class="feed-status">NO RECENT ACTIVITY</span>
          </div>
        </div>
      `;
    }

    const recentEvents = this.activityFeed.slice(-10).reverse();
    return `
      <div class="activity-feed">
        <div class="feed-header">
          <span class="feed-title">ACTIVITY STREAM</span>
          <span class="feed-count">${this.activityFeed.length} events</span>
        </div>
        <div class="feed-items">
          ${recentEvents.map(event => `
            <div class="feed-item severity-${event.severity || 'info'}" data-event-type="${event.type}">
              <span class="feed-icon">${this.getEventIcon(event.type)}</span>
              <span class="feed-time">${this.formatTime(event.timestamp)}</span>
              <span class="feed-module">${event.module}</span>
              <span class="feed-message">${event.message}</span>
            </div>
          `).join('')}
        </div>
      </div>
    `;
  }

  /**
   * Render issues panel showing errors and warnings
   */
  private renderIssuesPanel(): string {
    if (this.issues.length === 0) return '';

    return `
      <div class="issues-panel">
        <div class="issues-header">
          <span class="issues-title">ISSUES</span>
          <span class="issues-count">${this.issues.length}</span>
        </div>
        <div class="issues-list">
          ${this.issues.map(issue => `
            <div class="issue-item type-${issue.type}" data-issue-id="${issue.id}" data-module="${issue.module}">
              <span class="issue-icon">${issue.type === 'error' ? '❌' : '⚠️'}</span>
              <span class="issue-module">${issue.module}</span>
              <span class="issue-message">${issue.message}</span>
              <span class="issue-time">${this.formatTime(issue.timestamp)}</span>
            </div>
          `).join('')}
        </div>
      </div>
    `;
  }

  /**
   * Get icon for activity event type
   */
  private getEventIcon(type: string): string {
    switch (type) {
      case 'thought': return '💭';
      case 'action': return '⚡';
      case 'memory': return '💾';
      case 'error': return '❌';
      case 'tool': return '🔧';
      default: return '📡';
    }
  }

  /**
   * Format timestamp for display
   */
  private formatTime(date: Date): string {
    return date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  /**
   * Add activity event (can be called from external sources)
   */
  public addActivity(event: Omit<ActivityEvent, 'timestamp'>): void {
    this.activityFeed.push({
      ...event,
      timestamp: new Date()
    });
    // Keep only last 100 events
    if (this.activityFeed.length > 100) {
      this.activityFeed = this.activityFeed.slice(-100);
    }
    this.requestUpdate();
  }

  /**
   * Add issue (can be called from external sources)
   */
  public addIssue(issue: Omit<Issue, 'id' | 'timestamp'>): void {
    this.issues.push({
      ...issue,
      id: `issue-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      timestamp: new Date()
    });
    this.requestUpdate();
  }

  /**
   * Clear issue by ID
   */
  public clearIssue(issueId: string): void {
    this.issues = this.issues.filter(i => i.id !== issueId);
    this.requestUpdate();
  }

  /**
   * Attach event listeners to dynamically rendered content (unsafeHTML)
   * Called after each update since unsafeHTML doesn't preserve listeners
   */
  protected override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);
    this.attachDynamicEventListeners();
  }

  private attachDynamicEventListeners(): void {
    if (!this.shadowRoot) return;

    // Module click handlers (SVG regions)
    this.shadowRoot.querySelectorAll('.brain-module').forEach(el => {
      // Remove old listener to prevent duplicates
      const handler = () => {
        const module = (el as HTMLElement).dataset.module;
        if (module) this.selectModule(module);
      };
      el.removeEventListener('click', handler);
      el.addEventListener('click', handler);
    });

    // Module card click handlers
    this.shadowRoot.querySelectorAll('.module-card').forEach(el => {
      const handler = () => {
        const module = (el as HTMLElement).dataset.module;
        if (module) this.selectModule(module);
      };
      el.removeEventListener('click', handler);
      el.addEventListener('click', handler);
    });

    // Back button
    const backBtn = this.shadowRoot.querySelector('[data-action="back"]');
    if (backBtn) {
      backBtn.addEventListener('click', () => {
        this.selectedModule = null;
        this.requestUpdate();
      });
    }

    // View log button
    const viewLogBtn = this.shadowRoot.querySelector('[data-action="view-log"]');
    if (viewLogBtn) {
      viewLogBtn.addEventListener('click', async (e) => {
        const logFile = (e.currentTarget as HTMLElement).dataset.log;
        if (logFile && this.persona) {
          await this.openLogViewer(logFile);
        }
      });
    }

    // Issue click handlers - click to view module logs
    this.shadowRoot.querySelectorAll('.issue-item').forEach(el => {
      el.addEventListener('click', async () => {
        const module = (el as HTMLElement).dataset.module;
        if (module && module !== 'system') {
          const logTypeMap: Record<string, string> = {
            prefrontal: 'prefrontal',
            limbic: 'limbic',
            hippocampus: 'hippocampus',
            motorCortex: 'motor-cortex',
            cns: 'cns'
          };
          const logType = logTypeMap[module];
          if (logType) {
            await this.openLogViewer(logType);
          }
        }
      });
    });

    // Issue indicator in stats bar - scroll to issues panel
    this.shadowRoot.querySelector('[data-action="show-issues"]')?.addEventListener('click', () => {
      const issuesPanel = this.shadowRoot?.querySelector('.issues-panel');
      issuesPanel?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    });

    // Stats bar quick-access log buttons (one-click to open logs)
    this.shadowRoot.querySelectorAll('[data-action="open-log"]').forEach(el => {
      el.addEventListener('click', async () => {
        const logType = (el as HTMLElement).dataset.log;
        if (logType) {
          await this.openLogViewer(logType);
        }
      });
    });

    // Category-specific log toggles on each module
    this.shadowRoot.querySelectorAll('.module-log-toggle').forEach(el => {
      el.addEventListener('click', async (e) => {
        e.stopPropagation(); // Don't trigger module selection
        const category = el.getAttribute('data-category');
        if (category) {
          await this.toggleCategoryLogging(category);
        }
      });
    });
  }

  private selectModule(module: string): void {
    this.selectedModule = module;
    this.requestUpdate();
  }

  private openLogViewer(logType: string): void {
    // Guard: Don't try to open logs that don't exist
    if (!this.availableLogs.has(logType)) {
      console.warn(`PersonaBrainWidget: Log '${logType}' not available for ${this.personaId}. Available: ${[...this.availableLogs].join(', ')}`);
      return;
    }

    // Log paths are in format: {uniqueId}/{logType} e.g., "helper/cns", "local/prefrontal"
    const logPath = `${this.personaId}/${logType}`;

    // OPTIMISTIC: Use ContentService for instant tab creation
    ContentService.open('diagnostics-log', logPath, {
      title: `${this.persona?.displayName} - ${logType}`,
      uniqueId: logPath,
      metadata: { logPath, autoFollow: true }
    });
  }
}

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry
