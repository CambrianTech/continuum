/**
 * VersionWidget - Dynamic version display
 * Shows current system version with real-time updates
 */

import { BaseWidget } from '../shared/BaseWidget';

export class VersionWidget extends BaseWidget {
  
  private currentVersion: string = 'Loading...';
  private lastUpdate: Date = new Date();

  constructor() {
    super();
    this.widgetName = 'VersionWidget';
    this.widgetIcon = '🏷️';
    this.widgetTitle = 'System Version';
  }

  protected async initializeWidget(): Promise<void> {
    await this.loadCSS();
    await this.fetchCurrentVersion(); // Get real version first
    this.setupVersionMonitoring();
    this.render();
  }

  private setupVersionMonitoring(): void {
    // Listen for version updates
    document.addEventListener('continuum:version-update', (e: Event) => {
      const customEvent = e as CustomEvent;
      this.updateVersion(customEvent.detail.version);
    });

    // Check for version from continuum API
    const continuum = (window as any).continuum;
    if (continuum?.version) {
      this.currentVersion = continuum.version;
    }

    // Monitor for version changes every 30 seconds
    setInterval(() => {
      this.checkForVersionUpdates();
    }, 30000);
  }

  private async fetchCurrentVersion(): Promise<void> {
    try {
      const continuum = (window as any).continuum;
      const result = await continuum.info();
      this.currentVersion = result.version;
      this.lastUpdate = new Date();
    } catch (error) {
      console.warn('Could not get version:', error);
      this.currentVersion = 'Unknown';
    }
  }

  private async checkForVersionUpdates(): Promise<void> {
    const previousVersion = this.currentVersion;
    await this.fetchCurrentVersion();
    
    if (this.currentVersion !== previousVersion) {
      this.render();
      this.showUpdateAnimation();
    }
  }

  private updateVersion(newVersion: string): void {
    if (newVersion !== this.currentVersion) {
      console.log(`🏷️ Version: ${this.currentVersion} → ${newVersion}`);
      this.currentVersion = newVersion;
      this.lastUpdate = new Date();
      this.render();
      this.showUpdateAnimation();
    }
  }

  renderContent(): string {
    const updateTime = this.lastUpdate.toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit'
    });

    return `
      <div class="version-container">
        <div class="version-info">
          <span class="version-label">v</span>
          <span class="version-number">${this.currentVersion}</span>
        </div>
        <div class="version-meta">
          <span class="last-update">Updated ${updateTime}</span>
        </div>
      </div>
    `;
  }

  setupEventListeners(): void {
    // Click to copy version to clipboard
    const container = this.shadowRoot?.querySelector('.version-container');
    if (container) {
      container.addEventListener('click', () => {
        this.copyVersionToClipboard();
      });
    }
  }

  private copyVersionToClipboard(): void {
    const versionText = `Continuum v${this.currentVersion}`;
    
    if (navigator.clipboard) {
      navigator.clipboard.writeText(versionText).then(() => {
        this.showCopyFeedback();
      }).catch(err => {
        console.error('Failed to copy version:', err);
      });
    }
  }

  private showCopyFeedback(): void {
    const container = this.shadowRoot?.querySelector('.version-container') as HTMLElement;
    if (container) {
      container.classList.add('copied');
      setTimeout(() => {
        container.classList.remove('copied');
      }, 1000);
    }
  }

  private showUpdateAnimation(): void {
    const container = this.shadowRoot?.querySelector('.version-container') as HTMLElement;
    if (container) {
      container.classList.add('updated');
      setTimeout(() => {
        container.classList.remove('updated');
      }, 600);
    }
  }
}

// Register the custom element
customElements.define('version-widget', VersionWidget);