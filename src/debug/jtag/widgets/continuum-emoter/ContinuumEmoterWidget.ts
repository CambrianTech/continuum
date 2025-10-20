/**
 * ContinuumEmoter Widget - HAL 9000 System Status
 * Left: Glowing orb showing global system health
 * Right: Auto-scrolling feed of AI activity
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';

interface StatusMessage {
  message: string;
  timestamp: string;
}

export class ContinuumEmoterWidget extends BaseWidget {
  private connectionStatus: 'initializing' | 'connected' | 'disconnected' = 'initializing';
  private health: 'unknown' | 'healthy' | 'warning' | 'error' = 'unknown';
  private statusMessages: StatusMessage[] = [];
  private maxMessages = 10;
  private healthCheckInterval?: NodeJS.Timeout;

  constructor() {
    super({
      widgetId: 'continuum-emoter-widget',
      widgetName: 'ContinuumEmoterWidget',
      styles: 'continuum-emoter.css',
      template: 'continuum-emoter.html',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎭 ContinuumEmoter: Initializing...');

    await this.startHealthMonitoring();
    this.subscribeToAIEvents();

    // Add test message to verify scroller works
    console.log('🎭 ContinuumEmoter: Adding test message...');
    this.addStatusMessage('System: initialized');

    console.log('✅ ContinuumEmoter: Initialized');
  }

  protected override async onWidgetCleanup(): Promise<void> {
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }
  }

  /**
   * Subscribe to AI decision events for status feed
   */
  private subscribeToAIEvents(): void {
    console.log('🎭 ContinuumEmoter: Subscribing to AI events...');
    console.log('🎭 ContinuumEmoter: Event constants:', AI_DECISION_EVENTS);

    Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string; personaName?: string }) => {
      console.log('🎭 ContinuumEmoter: Received EVALUATING event:', data);
      this.addStatusMessage(`${data.personaName || 'AI'}: thinking...`);
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName || 'AI'}: responding`);
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName || 'AI'}: passed`);
    });

    Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName || 'AI'}: generating...`);
    });

    Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName || 'AI'}: checking...`);
    });

    Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName || 'AI'}: posted ✓`);
    });

    Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string; personaName?: string; error: string }) => {
      this.addStatusMessage(`${data.personaName || 'AI'}: error ✗`);
    });
  }

  /**
   * Start monitoring system health
   */
  private async startHealthMonitoring(): Promise<void> {
    // Initial health check
    await this.checkSystemHealth();

    // Check every 5 seconds
    this.healthCheckInterval = setInterval(async () => {
      await this.checkSystemHealth();
    }, 5000);
  }

  /**
   * Check system health using ping command
   */
  private async checkSystemHealth(): Promise<void> {
    try {
      // TODO: Use actual health check command
      // For now, assume connected and healthy
      this.updateStatus('connected', 'healthy');
    } catch (error) {
      this.updateStatus('disconnected', 'error');
    }
  }

  /**
   * Update connection status and health
   */
  private updateStatus(connectionStatus: typeof this.connectionStatus, health: typeof this.health): void {
    if (this.connectionStatus !== connectionStatus || this.health !== health) {
      this.connectionStatus = connectionStatus;
      this.health = health;
      this.updateOrb();
    }
  }

  /**
   * Update just the orb color (not full re-render)
   */
  private updateOrb(): void {
    const orb = this.shadowRoot?.querySelector('.status-orb') as HTMLElement;
    if (orb) {
      // Remove all status classes
      orb.classList.remove('status-healthy', 'status-warning', 'status-error', 'status-initializing');

      // Add current status class
      const statusKey = `${this.connectionStatus}-${this.health}`;
      if (statusKey === 'connected-healthy') {
        orb.classList.add('status-healthy'); // Green (default when online)
      } else if (statusKey.includes('error') || statusKey.includes('disconnected')) {
        orb.classList.add('status-error'); // Red (error state)
      } else {
        orb.classList.add('status-warning'); // Yellow (connecting/initializing)
      }
    }
  }

  /**
   * Add a status message to the scrolling feed
   */
  private addStatusMessage(message: string): void {
    console.log('🎭 ContinuumEmoter: addStatusMessage called:', message);

    const timestamp = new Date().toLocaleTimeString('en-US', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    });

    this.statusMessages.unshift({ message, timestamp });
    console.log('🎭 ContinuumEmoter: Status messages array:', this.statusMessages.length, 'messages');

    // Keep only last N messages
    if (this.statusMessages.length > this.maxMessages) {
      this.statusMessages.pop();
    }

    this.updateStatusScroller();
  }

  /**
   * Update just the status scroller (not full re-render)
   */
  private updateStatusScroller(): void {
    const scroller = this.shadowRoot?.querySelector('.status-scroller');
    if (scroller) {
      scroller.innerHTML = this.statusMessages.map(({ message, timestamp }) => `
        <div class="status-message">
          <span class="status-time">${timestamp}</span>
          <span class="status-text">${message}</span>
        </div>
      `).join('');
    }
  }

  protected async renderWidget(): Promise<void> {
    // Use BaseWidget's template and styles system
    const styles = this.templateCSS ?? '/* No styles loaded */';
    const template = this.templateHTML ?? '<div>Loading...</div>';

    // Ensure template is a string
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${templateString}
    `;
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/continuum-emoter/public/${filename}`;
  }
}
