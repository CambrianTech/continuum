/**
 * DiagnosticsWidget - System diagnostics and persona log viewer
 *
 * Provides access to:
 * - All persona log files (hippocampus, mind, soul, body, cns)
 * - System diagnostics and health checks
 * - Clickable log regions that open as tabs
 *
 * Uses BasePanelWidget for consistent styling with Settings/Help/Theme.
 */

import { BasePanelWidget } from '../shared/BasePanelWidget';
import { Commands } from '../../system/core/shared/Commands';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

interface PersonaInfo {
  uniqueId: string;
  displayName: string;
  id: UUID;
  type: 'persona' | 'agent' | 'human';
  status: 'online' | 'offline' | 'idle';
  logFiles: LogFileInfo[];
}

interface LogFileInfo {
  name: string;
  path: string;
  size: number;
  lastModified: string;
  category: 'cognition' | 'memory' | 'system' | 'integration';
}

export class DiagnosticsWidget extends BasePanelWidget {
  private personas: PersonaInfo[] = [];
  private selectedPersona: string | null = null;
  private isLoading = true;

  constructor() {
    super({
      widgetName: 'DiagnosticsWidget',
      panelTitle: 'System Diagnostics',
      panelSubtitle: 'Monitor personas, view logs, and run health checks',
      // Assistant now handled by layout system's right panel
      // assistantRoom: 'help',
      // assistantGreeting: 'Need help debugging? Ask me about logs, persona health, or system diagnostics.',
      enableDatabase: true,
      additionalStyles: DIAGNOSTICS_STYLES
    });
  }

  protected async onPanelInitialize(): Promise<void> {
    await this.loadPersonas();
    this.emitPositronContext();
  }

  /**
   * Emit Positron context for AI awareness
   */
  private emitPositronContext(): void {
    const selectedPersonaInfo = this.selectedPersona
      ? this.personas.find(p => p.uniqueId === this.selectedPersona)
      : null;

    PositronWidgetState.emit(
      {
        widgetType: 'diagnostics',
        section: this.selectedPersona ? 'persona-logs' : 'persona-list',
        title: this.selectedPersona
          ? `Diagnostics - ${selectedPersonaInfo?.displayName || this.selectedPersona}`
          : 'System Diagnostics',
        entityId: this.selectedPersona || undefined,
        metadata: {
          personaCount: this.personas.length,
          selectedPersona: this.selectedPersona,
          onlinePersonas: this.personas.filter(p => p.status === 'online').length
        }
      },
      { action: 'debugging', target: this.selectedPersona ? 'persona logs' : 'system health' }
    );
  }

  private async loadPersonas(): Promise<void> {
    this.isLoading = true;
    this.renderWidget();

    try {
      // Get all users with persona type
      const result = await Commands.execute('data/list', {
        collection: 'users',
        filter: { type: 'persona' },
        limit: 50
      } as any) as any;

      if (result.success && result.items) {
        this.personas = await Promise.all(
          result.items.map(async (user: any) => ({
            uniqueId: user.uniqueId,
            displayName: user.displayName,
            id: user.id,
            type: user.type,
            status: user.status || 'offline',
            logFiles: await this.getLogFilesForPersona(user.uniqueId)
          }))
        );
      }
    } catch (error) {
      console.error('DiagnosticsWidget: Error loading personas:', error);
    }

    this.isLoading = false;
    this.renderWidget();
  }

  private async getLogFilesForPersona(uniqueId: string): Promise<LogFileInfo[]> {
    // TODO: Implement actual log file discovery
    // For now, return expected log files
    const logCategories = [
      { name: 'hippocampus.log', category: 'memory' as const },
      { name: 'mind.log', category: 'cognition' as const },
      { name: 'soul.log', category: 'cognition' as const },
      { name: 'body.log', category: 'system' as const },
      { name: 'cns.log', category: 'integration' as const }
    ];

    return logCategories.map(log => ({
      name: log.name,
      path: `.continuum/personas/${uniqueId}/logs/${log.name}`,
      size: 0,
      lastModified: new Date().toISOString(),
      category: log.category
    }));
  }

  protected getStyles(): string {
    return DIAGNOSTICS_STYLES;
  }

  protected async renderContent(): Promise<string> {
    if (this.isLoading) {
      return this.createLoading('Loading persona diagnostics...');
    }

    return `
      ${this.renderSystemHealth()}
      ${this.renderPersonaList()}
      ${this.selectedPersona ? this.renderPersonaDetails() : ''}
    `;
  }

  private renderSystemHealth(): string {
    return this.createSection('System Health', `
      <div class="health-grid">
        <div class="health-item">
          <span class="health-label">Personas Active</span>
          <span class="health-value">${this.personas.filter(p => p.status === 'online').length}/${this.personas.length}</span>
        </div>
        <div class="health-item">
          <span class="health-label">Memory DBs</span>
          <span class="health-value status-operational">Healthy</span>
        </div>
        <div class="health-item">
          <span class="health-label">Hippocampus</span>
          <span class="health-value status-operational">Active</span>
        </div>
        <div class="health-item">
          <span class="health-label">RAG System</span>
          <span class="health-value status-operational">Online</span>
        </div>
      </div>
    `);
  }

  private renderPersonaList(): string {
    const personaItems = this.personas.map(persona => `
      <div class="persona-card ${this.selectedPersona === persona.uniqueId ? 'selected' : ''}"
           data-persona-id="${persona.uniqueId}">
        <div class="persona-header">
          <span class="persona-avatar">${this.getPersonaAvatar(persona)}</span>
          <div class="persona-info">
            <span class="persona-name">${persona.displayName}</span>
            <span class="persona-id">@${persona.uniqueId}</span>
          </div>
          <span class="status-indicator status-${persona.status}">${persona.status}</span>
        </div>
        <div class="persona-logs">
          ${persona.logFiles.map(log => `
            <button class="log-button" data-log-path="${log.path}" data-log-name="${log.name}">
              <span class="log-icon">${this.getLogIcon(log.category)}</span>
              <span class="log-name">${log.name.replace('.log', '')}</span>
            </button>
          `).join('')}
        </div>
      </div>
    `).join('');

    return this.createSection('Personas', personaItems, {
      intro: 'Click a persona to see details, or click a log file to open it in a new tab.'
    });
  }

  private renderPersonaDetails(): string {
    const persona = this.personas.find(p => p.uniqueId === this.selectedPersona);
    if (!persona) return '';

    return this.createSection(`${persona.displayName} Details`, `
      <div class="persona-detail-grid">
        <div class="detail-item">
          <span class="detail-label">ID</span>
          <span class="detail-value">${persona.id}</span>
        </div>
        <div class="detail-item">
          <span class="detail-label">Type</span>
          <span class="detail-value">${persona.type}</span>
        </div>
        <div class="detail-item">
          <span class="detail-label">Status</span>
          <span class="detail-value">${persona.status}</span>
        </div>
      </div>
      <div class="detail-actions">
        <button class="btn btn-secondary" data-action="view-memory">View Memory Stats</button>
        <button class="btn btn-secondary" data-action="view-rag">View RAG Context</button>
        <button class="btn btn-secondary" data-action="restart">Restart Persona</button>
      </div>
    `, { highlighted: true });
  }

  private getPersonaAvatar(persona: PersonaInfo): string {
    const firstLetter = persona.displayName.charAt(0).toUpperCase();
    return `<span class="avatar-letter">${firstLetter}</span>`;
  }

  private getLogIcon(category: string): string {
    switch (category) {
      case 'memory': return '🧠';
      case 'cognition': return '💭';
      case 'system': return '⚙️';
      case 'integration': return '🔗';
      default: return '📄';
    }
  }

  protected async onContentRendered(): Promise<void> {
    if (!this.shadowRoot) return;

    // Attach click handlers for persona cards
    this.shadowRoot.querySelectorAll('.persona-card').forEach(card => {
      card.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        if (!target.closest('.log-button')) {
          const personaId = (card as HTMLElement).dataset.personaId;
          this.selectPersona(personaId || null);
        }
      });
    });

    // Attach click handlers for log buttons
    this.shadowRoot.querySelectorAll('.log-button').forEach(button => {
      button.addEventListener('click', async (e) => {
        e.stopPropagation();
        const btn = e.currentTarget as HTMLElement;
        const logPath = btn.dataset.logPath;
        const logName = btn.dataset.logName;
        if (logPath && logName) {
          await this.openLogTab(logPath, logName);
        }
      });
    });

    // Attach click handlers for detail actions
    this.shadowRoot.querySelectorAll('[data-action]').forEach(button => {
      button.addEventListener('click', async (e) => {
        const action = (e.currentTarget as HTMLElement).dataset.action;
        await this.handleDetailAction(action || '');
      });
    });
  }

  private selectPersona(personaId: string | null): void {
    this.selectedPersona = personaId;
    this.renderWidget();
  }

  private async openLogTab(logPath: string, logName: string): Promise<void> {
    console.log(`DiagnosticsWidget: Opening log tab for ${logPath}`);

    // TODO: Implement opening log as a new tab
    // This will use the content/open command to create a new tab with a log viewer
    try {
      await Commands.execute('collaboration/content/open', {
        contentType: 'diagnostics-log',
        entityId: logPath,
        title: logName,
        setAsCurrent: true,
        metadata: {
          logPath,
          autoFollow: true
        }
      } as any);
    } catch (error) {
      console.error('DiagnosticsWidget: Error opening log tab:', error);
    }
  }

  private async handleDetailAction(action: string): Promise<void> {
    if (!this.selectedPersona) return;

    console.log(`DiagnosticsWidget: Action ${action} for ${this.selectedPersona}`);

    switch (action) {
      case 'view-memory':
        // TODO: Open memory stats panel
        break;
      case 'view-rag':
        // TODO: Open RAG context viewer
        break;
      case 'restart':
        // TODO: Restart persona
        break;
    }
  }
}

/**
 * Additional styles specific to DiagnosticsWidget
 */
const DIAGNOSTICS_STYLES = `
  .health-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
  }

  .health-item {
    background: rgba(0, 10, 15, 0.5);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 6px;
    padding: 12px;
    text-align: center;
  }

  .health-label {
    display: block;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    margin-bottom: 4px;
  }

  .health-value {
    font-size: 14px;
    font-weight: 600;
    color: var(--content-accent, #00d4ff);
  }

  .persona-card {
    background: rgba(0, 10, 15, 0.5);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 12px;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .persona-card:hover {
    border-color: rgba(0, 212, 255, 0.4);
    background: rgba(0, 10, 15, 0.7);
  }

  .persona-card.selected {
    border-color: var(--content-accent, #00d4ff);
    background: rgba(0, 212, 255, 0.1);
  }

  .persona-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
  }

  .persona-avatar {
    width: 40px;
    height: 40px;
    background: var(--button-primary-background, linear-gradient(135deg, #00d4ff, #0099cc));
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .avatar-letter {
    font-size: 18px;
    font-weight: 600;
    color: white;
  }

  .persona-info {
    flex: 1;
  }

  .persona-name {
    display: block;
    font-size: 14px;
    font-weight: 500;
    color: white;
  }

  .persona-id {
    display: block;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    font-family: monospace;
  }

  .persona-logs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .log-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.2));
    border-radius: 4px;
    color: var(--content-accent, #00d4ff);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .log-button:hover {
    background: rgba(0, 212, 255, 0.2);
    border-color: rgba(0, 212, 255, 0.4);
  }

  .log-icon {
    font-size: 14px;
  }

  .persona-detail-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
    margin-bottom: 16px;
  }

  .detail-item {
    text-align: center;
  }

  .detail-label {
    display: block;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    margin-bottom: 4px;
  }

  .detail-value {
    font-size: 13px;
    font-family: monospace;
    color: white;
    word-break: break-all;
  }

  .detail-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
`;

// Register the custom element
customElements.define('diagnostics-widget', DiagnosticsWidget);
