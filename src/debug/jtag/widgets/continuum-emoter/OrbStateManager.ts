/**
 * OrbStateManager - Encapsulates all orb state logic
 *
 * Manages orb color transitions between:
 * - Health-based states (healthy, warning, error, initializing)
 * - Custom emotions (temporary color overlays)
 * - Future: Cursor mode (detached orb moving around screen)
 */

export type HealthState = 'unknown' | 'healthy' | 'warning' | 'error';
export type ConnectionStatus = 'initializing' | 'connected' | 'disconnected';

export interface EmotionState {
  color: string;
  duration: number;
  startTime: number;
}

/**
 * OrbStateManager manages orb state priority and transitions
 */
export class OrbStateManager {
  private health: HealthState = 'unknown';
  private connectionStatus: ConnectionStatus = 'initializing';
  private currentEmotion: EmotionState | null = null;
  private emotionTimeout: NodeJS.Timeout | null = null;

  constructor(
    private orbElement: HTMLElement,
    private onEmotionEnd?: () => void
  ) {}

  /**
   * Update health state
   */
  updateHealth(connectionStatus: ConnectionStatus, health: HealthState): void {
    this.connectionStatus = connectionStatus;
    this.health = health;

    // Only update orb if no emotion is active
    if (!this.currentEmotion) {
      this.applyHealthState();
    }
  }

  /**
   * Apply temporary emotion color
   */
  setEmotion(color: string, duration: number): void {
    // Clear any existing emotion
    this.clearEmotion();

    // Set new emotion state
    this.currentEmotion = {
      color,
      duration,
      startTime: Date.now()
    };

    // Apply emotion color
    this.applyEmotionState();

    // Schedule emotion end
    this.emotionTimeout = setTimeout(() => {
      this.clearEmotion();
      this.applyHealthState();
      this.onEmotionEnd?.();
    }, duration);
  }

  /**
   * Clear current emotion
   */
  clearEmotion(): void {
    if (this.emotionTimeout) {
      clearTimeout(this.emotionTimeout);
      this.emotionTimeout = null;
    }
    this.currentEmotion = null;
  }

  /**
   * Check if emotion is currently active
   */
  get isEmotionActive(): boolean {
    return this.currentEmotion !== null;
  }

  /**
   * Apply health-based color to orb
   */
  private applyHealthState(): void {
    // Remove all status classes
    this.orbElement.classList.remove(
      'status-healthy',
      'status-warning',
      'status-error',
      'status-initializing',
      'status-custom'
    );

    // Remove custom color
    this.orbElement.style.removeProperty('--orb-color');

    // Add current status class
    const statusKey = `${this.connectionStatus}-${this.health}`;
    if (statusKey === 'connected-healthy') {
      this.orbElement.classList.add('status-healthy'); // Green
    } else if (statusKey.includes('error') || statusKey.includes('disconnected')) {
      this.orbElement.classList.add('status-error'); // Red
    } else {
      this.orbElement.classList.add('status-warning'); // Yellow
    }
  }

  /**
   * Apply emotion color to orb
   */
  private applyEmotionState(): void {
    if (!this.currentEmotion) return;

    // Remove all status classes
    this.orbElement.classList.remove(
      'status-healthy',
      'status-warning',
      'status-error',
      'status-initializing'
    );

    // Apply custom color
    this.orbElement.style.setProperty('--orb-color', this.currentEmotion.color);
    this.orbElement.classList.add('status-custom');
  }

  /**
   * Cleanup on destroy
   */
  destroy(): void {
    this.clearEmotion();
  }
}
