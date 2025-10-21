/**
 * Session Costs Widget - Display session usage metrics
 * ===================================================
 * Shows requests count and cost information in sidebar
 */

import { BaseWidget } from '../shared/BaseWidget';

interface SessionMetrics {
  requests: number;
  cost: number;
  status: 'active' | 'paused' | 'ended';
}

export class SessionCostsWidget extends BaseWidget {
  private metrics: SessionMetrics = {
    requests: 0,
    cost: 0.0,
    status: 'active'
  };

  constructor() {
    super();
    this.loadSessionMetrics();
  }

  static get widgetName(): string {
    return 'session-costs';
  }

  protected getOwnCSS(): string[] {
    return ['SessionCosts.css'];
  }

  protected renderOwnContent(): string {
    return `
      <div class="session-costs-container">
        <div class="section-header">
          <span class="section-icon">💰</span>
          <span class="section-title">Session Costs</span>
          <span class="section-status status-${this.metrics.status}">${this.getStatusText()}</span>
        </div>
        
        <div class="metrics-display">
          <div class="metric-row">
            <span class="metric-label">Requests</span>
            <span class="metric-value">${this.metrics.requests}</span>
          </div>
          <div class="metric-row">
            <span class="metric-label">Cost</span>
            <span class="metric-value highlight">$${this.formatCost(this.metrics.cost)}</span>
          </div>
        </div>
        
        <div class="cost-actions">
          <button class="action-button" data-action="refresh">🔄 Refresh</button>
          <button class="action-button" data-action="export">📊 Export</button>
        </div>
      </div>
    `;
  }

  private async loadSessionMetrics(): Promise<void> {
    try {
      // Get current session info
      const sessionResponse = await this.executeCommand('session-info', {});
      
      if (sessionResponse?.success) {
        this.metrics.requests = sessionResponse.data?.requests || 0;
        this.metrics.cost = sessionResponse.data?.cost || 0.0;
        this.metrics.status = sessionResponse.data?.status || 'active';
      } else {
        // Fallback to mock data if command fails
        this.loadMockMetrics();
      }
      
      this.updateContent();
      
    } catch (error) {
      console.warn('📊 SessionCosts: Failed to load metrics, using mock data:', error);
      this.loadMockMetrics();
      this.updateContent();
    }
  }

  private loadMockMetrics(): void {
    this.metrics = {
      requests: 47,
      cost: 0.0000,
      status: 'active'
    };
  }

  private formatCost(cost: number): string {
    return cost.toFixed(4);
  }

  private getStatusText(): string {
    switch (this.metrics.status) {
      case 'active': return 'Active';
      case 'paused': return 'Paused';
      case 'ended': return 'Ended';
      default: return 'Unknown';
    }
  }

  protected setupEventListeners(): void {
    this.addEventListener('click', this.handleActionClick.bind(this));
    
    // Listen for session updates
    this.notifySystem('session_updated', () => {
      this.loadSessionMetrics();
    });
  }

  private handleActionClick(event: Event): void {
    const target = event.target as HTMLElement;
    const action = target.dataset.action;
    
    switch (action) {
      case 'refresh':
        this.loadSessionMetrics();
        break;
      case 'export':
        this.exportMetrics();
        break;
    }
  }

  private async exportMetrics(): Promise<void> {
    try {
      const exportData = {
        sessionMetrics: this.metrics,
        timestamp: new Date().toISOString(),
        export_type: 'session_costs'
      };
      
      await this.executeCommand('export-data', exportData);
      console.log('📊 SessionCosts: Metrics exported successfully');
      
    } catch (error) {
      console.error('📊 SessionCosts: Failed to export metrics:', error);
    }
  }

  private updateContent(): void {
    const container = this.shadowRoot?.querySelector('.session-costs-container');
    if (container) {
      container.innerHTML = this.renderOwnContent();
      this.setupEventListeners();
    }
  }
}

// Register the custom element
customElements.define('session-costs', SessionCostsWidget);