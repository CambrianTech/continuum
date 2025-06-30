// Sidebar Widget Component - Extends StatusWidget
// Automatic validation and status handling via parent class

class ContinuumSidebar extends StatusWidget {
    private static templateHTML: string = `{{SIDEBAR_WIDGET_HTML}}`;

    protected getStatusElements(): Record<string, string> {
        return {
            'version': 'Version display element',
            'ws-status': 'WebSocket connection status',
            'cmd-status': 'Command system status'
        };
    }

    protected initializeStatusUpdates(): void {
        // Set up version display
        const versionEl = this.statusElements.get('version');
        if (versionEl && window.__CONTINUUM_VERSION__) {
            versionEl.textContent = window.__CONTINUUM_VERSION__;
        }
    }

    protected updateCustomStatus(): void {
        // Test command system and update status
        this.testCommand('ping', 'cmd-status', 'Command system ready');
    }
}