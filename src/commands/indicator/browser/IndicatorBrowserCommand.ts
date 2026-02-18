/**
 * Browser Indicator Command - Creates absolutely positioned visual indicators
 */

import { IndicatorCommand, type IndicatorParams, type IndicatorResult } from '../shared/IndicatorCommand';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';

export class IndicatorBrowserCommand extends IndicatorCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async showIndicator(params: IndicatorParams): Promise<IndicatorResult> {
    const timestamp = new Date().toISOString();
    const alertId = `event-test-indicator-${Date.now()}`;

    // Create fixed position indicator div like git hook validation
    const alertDiv = document.createElement('div');
    alertDiv.id = alertId;
    alertDiv.style.cssText = `
      position: fixed;
      top: 10px;
      right: 10px;
      background: ${this.getBackgroundColor(params.type || 'info')};
      color: white;
      padding: 10px;
      border-radius: 5px;
      font-family: monospace;
      font-size: 12px;
      z-index: 9999;
      box-shadow: rgba(0, 0, 0, 0.3) 0px 2px 10px;
      white-space: pre-line;
    `;

    // Set content like git hook indicator
    const title = params.title || 'REAL-TIME EVENT TEST';
    const eventId = `event-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const currentTime = new Date().toLocaleTimeString();

    alertDiv.innerHTML = `🌉 ${title}<br>ID: ${eventId}<br>Message: ${params.message}<br>Time: ${currentTime}<br>Status: ACTIVE`;

    // Add to DOM
    document.body.appendChild(alertDiv);

    console.log(`🔔 EVENT-INDICATOR-${Date.now()}: Created visual indicator for "${params.message}"`);

    // Keep it visible for 5 seconds for screenshots
    setTimeout(() => {
      if (alertDiv.parentNode) {
        alertDiv.parentNode.removeChild(alertDiv);
      }
    }, 5000);

    return {
      context: params.context,
      sessionId: params.sessionId,
      alerted: true,
      message: params.message,
      timestamp
    };
  }

  private getBackgroundColor(type: string): string {
    const colors = {
      'info': '#3b82f6',    // Blue
      'success': '#10b981', // Green
      'warning': '#f59e0b', // Orange
      'error': '#ef4444'    // Red
    };
    return colors[type as keyof typeof colors] || colors.info;
  }
}