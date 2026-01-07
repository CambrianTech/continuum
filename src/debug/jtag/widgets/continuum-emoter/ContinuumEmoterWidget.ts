/**
 * ContinuumEmoter Widget - HAL 9000 System Status
 * Left: Glowing orb showing global system health
 * Right: Auto-scrolling feed of AI activity
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';
import { COGNITION_EVENTS, type StageCompleteEvent } from '../../system/conversation/shared/CognitionEventTypes';
import { OrbStateManager, type ConnectionStatus, type HealthState } from './OrbStateManager';
import { TRANSPORT_EVENTS } from '../../system/transports/shared/TransportEvents';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export class ContinuumEmoterWidget extends BaseWidget {
  private connectionStatus: ConnectionStatus = 'initializing';
  private health: HealthState = 'unknown';
  private maxMessages: number;
  private healthCheckInterval?: NodeJS.Timeout;
  private orbManager: OrbStateManager | null = null;

  constructor(maxMessages: number = 100) {
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
    this.maxMessages = maxMessages;
  }

  protected async onWidgetInitialize(): Promise<void> {
    verbose() && console.log('🎭 ContinuumEmoter: Initializing...');

    this.subscribeToTransportEvents();  // Listen for instant connection state changes
    this.subscribeToAIEvents();
    this.subscribeToCognitionEvents();
    this.subscribeToEmotionEvents();

    // Add test message to verify scroller works
    verbose() && console.log('🎭 ContinuumEmoter: Adding test message...');
    this.addStatusMessage('System: initialized');

    verbose() && console.log('✅ ContinuumEmoter: Initialized');
  }

  protected override async onWidgetCleanup(): Promise<void> {
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }
    this.orbManager?.destroy();
  }

  /**
   * Subscribe to AI decision events for status feed
   */
  private subscribeToAIEvents(): void {
    //console.log('🎭 ContinuumEmoter: Subscribing to AI events...');
    //console.log('🎭 ContinuumEmoter: Event constants:', AI_DECISION_EVENTS);

    Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: thinking...`);
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: responding`);
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: passed`);
    });

    Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: generating...`);
    });

    Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: checking...`);
    });

    Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string; personaName?: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: posted ✓`);
    });

    Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string; personaName?: string; error: string }) => {
      this.addStatusMessage(`${data.personaName ?? 'AI'}: error ✗`);
    });
  }

  /**
   * Subscribe to cognition pipeline events
   */
  private subscribeToCognitionEvents(): void {
    verbose() && console.log('🎭 ContinuumEmoter: Subscribing to cognition events...');
    verbose() && console.log('🎭 ContinuumEmoter: COGNITION_EVENTS constant:', COGNITION_EVENTS);

    Events.subscribe(COGNITION_EVENTS.STAGE_COMPLETE, (data: StageCompleteEvent) => {
      // Show stage completion in status feed
      const statusIcon = data.metrics.status === 'fast' ? '⚡' :
                        data.metrics.status === 'normal' ? '✓' :
                        data.metrics.status === 'slow' ? '⏱️' : '🐌';

      this.addStatusMessage(`${statusIcon} ${data.stage}: ${data.metrics.durationMs}ms`);
    });
  }

  /**
   * Subscribe to emotion events (emoji + color overlay)
   */
  private subscribeToEmotionEvents(): void {
    verbose() && console.log('🎭 ContinuumEmoter: Subscribing to emotion events...');

    Events.subscribe('continuum:emotion', (data: { emoji: string; color: string; duration: number }) => {
      this.showEmotion(data.emoji, data.color, data.duration);
    });

    // Subscribe to continuum:status events for persistent orb color changes
    Events.subscribe('continuum:status', (data: {
      emoji?: string;
      color?: string;
      message?: string;
      clear?: boolean;
      priority?: string;
      source?: string;
      timestamp?: number;
      autoRevertAt?: number;
    }) => {
      this.handleStatusUpdate(data);
    });
  }

  /**
   * Handle continuum:status events - update orb color persistently
   */
  private handleStatusUpdate(status: {
    emoji?: string;
    color?: string;
    message?: string;
    clear?: boolean;
  }): void {
    const orb = this.shadowRoot?.querySelector('.status-orb') as HTMLElement;
    if (!orb) return;

    // Handle clear request - revert to health-based color
    if (status.clear) {
      // Clear any active emotion and revert to health state
      this.orbManager?.clearEmotion();
      this.orbManager?.updateHealth(this.connectionStatus, this.health);
      return;
    }

    // Apply custom color if provided
    if (status.color) {
      // Remove all status classes
      orb.classList.remove('status-healthy', 'status-warning', 'status-error', 'status-initializing');

      // Apply custom color via inline style
      orb.style.setProperty('--orb-color', status.color);
      orb.classList.add('status-custom');
    }

    // Show emoji if provided (floating overlay)
    if (status.emoji) {
      const duration = 3000; // Default 3 second display
      this.showEmotion(status.emoji, status.color || '#00ccff', duration);
    }
  }

  /**
   * Display emotion temporarily (emoji overlay + color glow)
   */
  private showEmotion(emoji: string, color: string, duration: number): void {
    const orb = this.shadowRoot?.querySelector('.status-orb') as HTMLElement;
    if (!orb || !this.orbManager) return;

    // Apply emotion color via manager
    this.orbManager.setEmotion(color, duration);

    // Emit continuum:status event so favicon updates with emoji and color
    Events.emit('continuum:status', {
      emoji,
      color,
      message: 'Emotion active',
      priority: 'high',
      source: 'emoter',
      autoRevertAt: Date.now() + duration
    });

    // Create emoji overlay - floats OUT of the ring (above border)
    const emojiOverlay = document.createElement('div');
    emojiOverlay.className = 'emotion-emoji';
    emojiOverlay.textContent = emoji;
    emojiOverlay.style.cssText = `
      position: absolute;
      top: 50%;
      left: 50%;
      transform: translate(-50%, -50%);
      font-size: 24px;
      z-index: 10;
      pointer-events: none;
      opacity: 1;
      transition: opacity 0.6s ease-out, transform 0.6s cubic-bezier(0.34, 1.56, 0.64, 1);
    `;

    orb.appendChild(emojiOverlay);

    // Float up slowly like an angel going to heaven - brief but nice touch
    setTimeout(() => {
      emojiOverlay.style.opacity = '0';
      emojiOverlay.style.transform = 'translate(-50%, -150%)'; // Slow float up
    }, duration - 600);

    // Remove emoji overlay after animation
    setTimeout(() => {
      emojiOverlay.remove();

      // Clear continuum:status after emotion ends
      Events.emit('continuum:status', { clear: true });
    }, duration);
  }

  /**
   * Subscribe to transport events for instant connection state updates
   */
  private subscribeToTransportEvents(): void {
    verbose() && console.log('🎭 ContinuumEmoter: Subscribing to transport events...');
    verbose() && console.log(`🎭 ContinuumEmoter: DISCONNECTED event name = "${TRANSPORT_EVENTS.DISCONNECTED}"`);
    verbose() && console.log(`🎭 ContinuumEmoter: CONNECTED event name = "${TRANSPORT_EVENTS.CONNECTED}"`);

    // Listen for instant disconnection
    Events.subscribe(TRANSPORT_EVENTS.DISCONNECTED, () => {
      verbose() && console.log('🔴 ContinuumEmoter: Received DISCONNECTED event');
      this.updateStatus('disconnected', 'error');
    });

    // Listen for instant reconnection
    Events.subscribe(TRANSPORT_EVENTS.CONNECTED, () => {
      verbose() && console.log('🟢 ContinuumEmoter: Received CONNECTED event');
      this.updateStatus('connected', 'healthy');
    });

    // Start with assumption of connected (will be corrected by first DISCONNECTED if needed)
    this.updateStatus('connected', 'healthy');
  }

  /**
   * Update connection status and health
   */
  private updateStatus(connectionStatus: ConnectionStatus, health: HealthState): void {
    if (this.connectionStatus !== connectionStatus || this.health !== health) {
      this.connectionStatus = connectionStatus;
      this.health = health;

      // Update orb via manager (respects active emotions automatically)
      this.orbManager?.updateHealth(connectionStatus, health);

      // Emit health status event so ContinuumWidget (favicon) can update too
      const statusKey = `${connectionStatus}-${health}`;
      if (statusKey.includes('error') || statusKey.includes('disconnected')) {
        // Disconnected - emit red error status
        Events.emit('continuum:status', {
          color: '#ff0060',
          message: 'Server disconnected',
          priority: 'critical',
          source: 'system'
        });
      } else if (statusKey === 'connected-healthy') {
        // Connected and healthy - clear custom status (revert to ground state)
        Events.emit('continuum:status', {
          clear: true
        });
      }
    }
  }


  /**
   * Add a status message to the scrolling feed
   */
  private addStatusMessage(message: string): void {
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

    // Initialize orb state manager after DOM is ready
    const orbElement = this.shadowRoot.querySelector('.status-orb') as HTMLElement;
    if (orbElement) {
      this.orbManager = new OrbStateManager(orbElement);
      // Apply initial health state
      this.orbManager.updateHealth(this.connectionStatus, this.health);
    }
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/continuum-emoter/public/${filename}`;
  }
}
