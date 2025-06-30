// StatusWidget - Common parent for widgets that display connection/system status
// Handles all common status widget validation and functionality

abstract class StatusWidget extends BaseWidget {
    protected statusElements: Map<string, HTMLElement> = new Map();

    protected validate(): void {
        super.validate();
        
        // Universal status widget validation
        const requiredStatusElements = this.getStatusElements();
        for (const [id, description] of Object.entries(requiredStatusElements)) {
            const element = this.getElement(id);
            if (element) {
                this.statusElements.set(id, element);
                this.log(`✅ Status element found: ${id} (${description})`);
            } else {
                this.log(`❌ Missing status element: ${id} (${description})`, 'error');
            }
        }
        
        this.log('Status widget validation passed');
    }

    protected initializeWidget(): void {
        this.log('Status widget initializing');
        this.setupContinuumAPI();
        this.initializeStatusUpdates();
    }

    protected onApiReady(): void {
        this.startStatusMonitoring();
    }

    /**
     * Define required status elements - override in each status widget
     * Returns object mapping element ID to description
     */
    protected abstract getStatusElements(): Record<string, string>;

    /**
     * Initialize status-specific updates - override as needed
     */
    protected initializeStatusUpdates(): void {
        // Default implementation - can be overridden
    }

    /**
     * Start monitoring status when API is ready
     */
    protected startStatusMonitoring(): void {
        // Set up connection status monitoring
        this.api!.on('continuum:connected', () => this.updateAllStatus());
        this.api!.on('continuum:disconnected', () => this.updateAllStatus());
        this.api!.on('continuum:error', () => this.updateAllStatus());
        
        this.updateAllStatus();
    }

    /**
     * Update all status elements
     */
    protected updateAllStatus(): void {
        this.updateConnectionStatus();
        this.updateCustomStatus();
    }

    /**
     * Update connection status for any element with 'ws-status' or 'connection' in ID
     */
    protected updateConnectionStatus(): void {
        for (const [id, element] of this.statusElements) {
            if (id.includes('ws-status') || id.includes('connection')) {
                super.updateConnectionStatus(element);
            }
        }
    }

    /**
     * Update custom status elements - override in each widget
     */
    protected updateCustomStatus(): void {
        // Default implementation - can be overridden
    }

    /**
     * Safe status update with error handling
     */
    protected updateStatus(elementId: string, status: string, isError: boolean = false): void {
        const element = this.statusElements.get(elementId);
        if (element) {
            element.textContent = status;
            element.className = `status-item ${isError ? 'error' : 'success'}`;
        }
    }

    /**
     * Test a command and update status element
     */
    protected async testCommand(command: string, statusElementId: string, successMessage: string): Promise<void> {
        try {
            await this.executeCommand(command, {});
            this.updateStatus(statusElementId, `✅ ${successMessage}`, false);
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : String(error);
            this.updateStatus(statusElementId, `❌ ${errorMessage}`, true);
        }
    }
}