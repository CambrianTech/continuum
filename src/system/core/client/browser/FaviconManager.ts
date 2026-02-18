/**
 * FaviconManager - Dynamic Browser Favicon Management
 *
 * Manages browser favicon updates from main document context (NOT shadow DOM).
 * Listens to both 'continuum:status' and 'connection:status' events.
 *
 * **CRITICAL ARCHITECTURAL REQUIREMENT**: Must run in MAIN document context,
 * NOT inside Web Component shadow DOM. Shadow DOM cannot reliably manipulate
 * document.head.
 *
 * **Priority System**:
 * - continuum:status (custom status) takes priority over connection:status
 * - connection:status (system health) only shows when no custom status active
 * - Favicon updates with emojis or colored dots (HAL 9000 style)
 */

import { Events } from '../../shared/Events';
import type { ContinuumStatus } from '../../../../commands/continuum/set/shared/ContinuumSetTypes';
import type { ConnectionStatus } from './ConnectionMonitor';

export class FaviconManager {
  private currentStatus: ContinuumStatus | null = null;
  private connectionStatus: ConnectionStatus | null = null;

  constructor() {
    console.log('🎨 FaviconManager: Initializing favicon management (main document context)...');

    // Listen to continuum:status for custom status (higher priority)
    Events.subscribe('continuum:status', (status: ContinuumStatus) => {
      this.handleContinuumStatus(status);
    });

    // Listen to connection:status for system health (lower priority)
    Events.subscribe('connection:status', (status: ConnectionStatus) => {
      this.handleConnectionStatus(status);
    });

    // Set initial favicon (connection ground state - green)
    this.updateFavicon('#00cc00'); // Green dot

    console.log('✅ FaviconManager: Initialization complete');
  }

  /**
   * Handle continuum:status events (custom status with emoji/color/message)
   */
  private handleContinuumStatus(status: ContinuumStatus): void {
    if (status.clear) {
      // Clear custom status, revert to connection status
      this.currentStatus = null;
      this.updateFaviconFromConnectionStatus();
      console.log('🔄 FaviconManager: Custom status cleared, reverting to connection status');
      return;
    }

    // Store new custom status
    this.currentStatus = status;

    // Update favicon with emoji or color
    if (status.emoji) {
      this.updateFavicon(undefined, status.emoji);
      console.log(`✨ FaviconManager: Updated favicon with emoji ${status.emoji}`);
    } else if (status.color) {
      this.updateFavicon(status.color);
      console.log(`✨ FaviconManager: Updated favicon with color ${status.color}`);
    }
  }

  /**
   * Handle connection:status events (system health monitoring)
   */
  private handleConnectionStatus(status: ConnectionStatus): void {
    // Store connection status
    this.connectionStatus = status;

    // Only update favicon if no custom status is active
    if (!this.currentStatus) {
      this.updateFaviconFromConnectionStatus();
    }
  }

  /**
   * Update favicon based on current connection status
   */
  private updateFaviconFromConnectionStatus(): void {
    if (this.connectionStatus) {
      this.updateFavicon(this.connectionStatus.color);
      console.log(`🔌 FaviconManager: Updated favicon for connection ${this.connectionStatus.state} (${this.connectionStatus.color})`);
    }
  }

  /**
   * Update browser favicon with color or emoji
   * @param color - CSS color for dot (e.g., '#00cc00', '#ffaa00', '#ff0000')
   * @param emoji - Optional emoji to display instead of dot
   */
  private updateFavicon(color?: string, emoji?: string): void {
    try {
      // Create canvas for favicon (32x32)
      const canvas = document.createElement('canvas');
      canvas.width = 32;
      canvas.height = 32;
      const ctx = canvas.getContext('2d');

      if (!ctx) {
        console.warn('⚠️ FaviconManager: Cannot create canvas context');
        return;
      }

      // Clear canvas
      ctx.clearRect(0, 0, 32, 32);

      if (emoji) {
        // Draw emoji as favicon
        ctx.font = '28px Arial';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(emoji, 16, 16);
      } else {
        // Draw colored dot as favicon (HAL 9000 style)
        const faviconColor = color || '#00cc00'; // Default to green

        // Draw main dot
        ctx.fillStyle = faviconColor;
        ctx.beginPath();
        ctx.arc(16, 16, 12, 0, Math.PI * 2);
        ctx.fill();

        // Add glow effect for visual appeal
        ctx.shadowBlur = 8;
        ctx.shadowColor = faviconColor;
        ctx.fill();
      }

      // Convert canvas to data URL
      const faviconUrl = canvas.toDataURL('image/png');

      // Update favicon link in document head
      let faviconLink = document.querySelector('link[rel="icon"]') as HTMLLinkElement;
      if (!faviconLink) {
        faviconLink = document.createElement('link');
        faviconLink.rel = 'icon';
        document.head.appendChild(faviconLink);
      }
      faviconLink.href = faviconUrl;

    } catch (error) {
      console.error('❌ FaviconManager: Failed to update favicon:', error);
    }
  }

  /**
   * Force update favicon (useful for testing)
   */
  forceUpdate(): void {
    if (this.currentStatus) {
      this.handleContinuumStatus(this.currentStatus);
    } else if (this.connectionStatus) {
      this.updateFaviconFromConnectionStatus();
    }
  }
}

// Create singleton instance
let managerInstance: FaviconManager | null = null;

/**
 * Get singleton FaviconManager instance
 */
export function getFaviconManager(): FaviconManager {
  if (!managerInstance) {
    managerInstance = new FaviconManager();
  }
  return managerInstance;
}

/**
 * Initialize favicon management (called from JTAGClientBrowser initialization)
 */
export function initializeFaviconManager(): void {
  getFaviconManager();
}
