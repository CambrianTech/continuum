/**
 * SidebarWidget - Left sidebar panel with navigation and status
 *
 * Shows:
 * - System emoter (HAL 9000 status indicator)
 * - Cognition histogram (AI pipeline visualization)
 * - Metrics (AI performance dashboard)
 * - Room list and user list
 *
 * Extends BaseSidePanelWidget for consistent panel behavior.
 */

import { BaseSidePanelWidget, type SidePanelSide } from '../shared/BaseSidePanelWidget';

export class SidebarWidget extends BaseSidePanelWidget {

  constructor() {
    super({
      widgetName: 'SidebarWidget'
    });
  }

  // === Panel Configuration ===

  protected get panelTitle(): string {
    return '';  // Not used - no header
  }

  protected get panelIcon(): string {
    return '';  // Not used - no header
  }

  protected get panelSide(): SidePanelSide {
    return 'left';
  }

  protected get showHeader(): boolean {
    return false;  // Just floating « button
  }

  // === Lifecycle ===

  protected async onPanelInitialize(): Promise<void> {
    console.log('🎯 SidebarWidget: Initializing...');
  }

  protected async onPanelCleanup(): Promise<void> {
    console.log('🧹 SidebarWidget: Cleanup complete');
  }

  // === Content Rendering ===

  protected async renderPanelContent(): Promise<string> {
    return `
      <div class="sidebar-widgets">
        <!-- ContinuumEmoter Widget - HAL 9000 System Status -->
        <continuum-emoter-widget></continuum-emoter-widget>

        <!-- Cognition Histogram Widget - AI Pipeline Visualization -->
        <cognition-histogram-widget></cognition-histogram-widget>

        <!-- Metrics Widget - AI Performance Dashboard -->
        <continuum-metrics-widget></continuum-metrics-widget>

        <!-- Room List -->
        <div class="widget-container rooms">
          <room-list-widget></room-list-widget>
        </div>

        <!-- User List (flex: 2 for more space) -->
        <div class="widget-container users">
          <user-list-widget></user-list-widget>
        </div>
      </div>
    `;
  }

  protected getAdditionalStyles(): string {
    return `
      .sidebar-widgets {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md, 12px);
        padding: var(--spacing-md, 12px);
        overflow-y: auto;
        overflow-x: hidden;
      }

      /* Emoter styling */
      continuum-emoter-widget {
        margin-bottom: 8px;
      }

      /* Widget containers split remaining vertical space */
      .widget-container {
        flex: 1;
        min-height: 120px;
        display: flex;
        flex-direction: column;
        overflow: hidden;
      }

      /* Users section gets more space (more items typically) */
      .widget-container.users {
        flex: 2;
        min-height: 200px;
      }

      /* Status View (if needed) */
      .status-view {
        margin-bottom: 16px;
        padding: 10px;
        background: var(--widget-surface, rgba(0, 212, 255, 0.1));
        border-radius: 6px;
        border: 1px solid var(--border-subtle, rgba(0, 212, 255, 0.2));
      }

      .connection-status {
        font-size: 0.8em;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 1px;
        margin-bottom: 5px;
      }

      .connection-status.connected {
        color: var(--content-success, #4CAF50);
      }

      .user-status {
        font-size: 0.7em;
        color: var(--content-secondary, rgba(255, 255, 255, 0.7));
      }
    `;
  }

  protected async onPanelRendered(): Promise<void> {
    console.log('✅ SidebarWidget: Rendered');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
