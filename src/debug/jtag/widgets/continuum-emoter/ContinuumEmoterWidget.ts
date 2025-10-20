/**
 * ContinuumEmoter Widget - HAL 9000 System Status
 * Left: Glowing orb showing global system health
 * Right: Auto-scrolling feed of AI activity
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';

export class ContinuumEmoterWidget extends BaseWidget {
  private connectionStatus: 'initializing' | 'connected' | 'disconnected' = 'initializing';
  private health: 'unknown' | 'healthy' | 'warning' | 'error' = 'unknown';
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
    this.subscribeToEmotionEvents();

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
   * Subscribe to emotion events (emoji + color overlay)
   */
  private subscribeToEmotionEvents(): void {
    console.log('🎭 ContinuumEmoter: Subscribing to emotion events...');

    Events.subscribe('continuum:emotion', (data: { emoji: string; color: string; duration: number }) => {
      console.log('🎭 ContinuumEmoter: Received emotion event:', data);
      this.showEmotion(data.emoji, data.color, data.duration);
    });
  }

  /**
   * Display emotion temporarily (emoji overlay + color glow)
   */
  private showEmotion(emoji: string, color: string, duration: number): void {
    const orb = this.shadowRoot?.querySelector('.status-orb') as HTMLElement;
    if (!orb) return;

    // Create emoji overlay - positioned INSIDE the ring, ABOVE the orb, BELOW the ring border
    const emojiOverlay = document.createElement('div');
    emojiOverlay.className = 'emotion-emoji';
    emojiOverlay.textContent = emoji;
    emojiOverlay.style.cssText = `
      position: absolute;
      top: 50%;
      left: 50%;
      transform: translate(-50%, -50%);
      font-size: 24px;
      z-index: 2;
      pointer-events: none;
      opacity: 1;
      transition: opacity 0.5s ease-out, transform 0.5s ease-out;
    `;

    // Save current color and apply emotion color temporarily
    const originalColor = orb.style.color;
    orb.style.color = color;
    orb.appendChild(emojiOverlay);

    // Fade out and float upward before removing
    setTimeout(() => {
      emojiOverlay.style.opacity = '0';
      emojiOverlay.style.transform = 'translate(-50%, -70%)'; // Move up 20% while fading
    }, duration - 500);

    // Reset after duration
    setTimeout(() => {
      orb.style.color = originalColor;
      emojiOverlay.remove();
    }, duration);
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

    const scroller = this.shadowRoot?.querySelector('.status-scroller');
    if (!scroller) return;

    // Create DOM element
    const messageItem = document.createElement('div');
    messageItem.className = 'status-message-item';
    messageItem.innerHTML = `<span class="status-text">${message}</span>`;

    // Append to bottom (will flow up due to flex-direction: column + justify-content: flex-end)
    scroller.appendChild(messageItem);

    // Remove from top if over limit
    const items = scroller.querySelectorAll('.status-message-item');
    if (items.length > this.maxMessages) {
      items[0].remove(); // Remove oldest (at top)
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
