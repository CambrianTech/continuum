/**
 * VersionWidget - Dynamic version display
 * Shows current system version with real-time updates
 */

import { BaseWidget } from '../shared/BaseWidget.js';

export class VersionWidget extends BaseWidget {
  private currentVersion: string = '0.2.2177';
  private lastUpdate: Date = new Date();

  constructor() {
    super();
    this.widgetName = 'VersionWidget';
    this.widgetIcon = '🏷️';
    this.widgetTitle = 'System Version';
    this.cssPath = '/src/ui/components/Version/VersionWidget.css';
  }

  protected async initializeWidget(): Promise<void> {
    await this.loadCSS();
    this.setupVersionMonitoring();
    this.render();
  }

  private setupVersionMonitoring(): void {
    // Listen for version updates
    document.addEventListener('continuum:version-update', (e: CustomEvent) => {
      this.updateVersion(e.detail.version);
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

  private checkForVersionUpdates(): void {
    const continuum = (window as any).continuum;
    if (continuum?.version && continuum.version !== this.currentVersion) {
      this.updateVersion(continuum.version);
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