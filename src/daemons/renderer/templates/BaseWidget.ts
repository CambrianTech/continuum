// Base Widget Class - Handles common widget functionality
// Self-validating widgets with built-in template management

interface ContinuumAPI {
    connect(): Promise<void>;
    disconnect(): void;
    isConnected(): boolean;
    getConnectionState(): string;
    validateClientHealth(): Promise<any>;
    execute(command: string, params?: any): Promise<any>;
    on(event: string, handler: (data?: any) => void): void;
    off(event: string, handler?: (data?: any) => void): void;
    emit(event: string, data?: any): void;
}

declare global {
    interface Window {
        continuum: ContinuumAPI;
        __CONTINUUM_VERSION__: string;
    }
}

abstract class BaseWidget extends HTMLElement {
    public shadowRoot: ShadowRoot;
    protected api?: ContinuumAPI;
    protected static templateHTML: string = '';

    constructor() {
        super();
        this.shadowRoot = this.attachShadow({ mode: 'open' });
    }

    connectedCallback(): void {
        this.render();
        this.validate();
        this.initializeWidget();
    }

    /**
     * Validate widget after rendering
     * Override this method in each widget to add custom validation
     */
    protected validate(): void {
        const constructor = this.constructor as typeof BaseWidget;
        const templateHTML = constructor.templateHTML;
        
        if (!templateHTML || templateHTML.length === 0) {
            this.log('No template HTML provided', 'error');
            return;
        }

        this.log('Base validation passed');
    }

    /**
     * Render the widget using its static template
     */
    protected render(): void {
        const constructor = this.constructor as typeof BaseWidget;
        this.shadowRoot.innerHTML = constructor.templateHTML;
    }

    /**
     * Initialize widget-specific functionality
     * Must be implemented by each widget
     */
    protected abstract initializeWidget(): void;


    /**
     * Safe element retrieval with error handling
     */
    protected getElement(id: string): HTMLElement | null {
        const element = this.shadowRoot.getElementById(id);
        if (!element) {
            console.error(`❌ ${this.constructor.name}: Element not found: ${id}`);
        }
        return element;
    }

    /**
     * Safe typed element retrieval
     */
    protected getTypedElement<T extends HTMLElement>(id: string): T | null {
        const element = this.getElement(id);
        return element as T;
    }

    /**
     * Set up Continuum API connection
     */
    protected setupContinuumAPI(): void {
        if (window.continuum) {
            this.api = window.continuum;
            this.onApiReady();
        } else {
            document.addEventListener('continuum:ready', () => {
                this.api = window.continuum;
                this.onApiReady();
            });
        }
    }

    /**
     * Called when Continuum API is ready
     * Override in each widget
     */
    protected onApiReady(): void {
        // Default implementation - can be overridden
    }

    /**
     * Execute command with error handling
     */
    protected async executeCommand(command: string, params: any = {}): Promise<any> {
        if (!this.api) {
            throw new Error(`${this.constructor.name}: Continuum API not available`);
        }

        try {
            return await this.api.execute(command, params);
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : String(error);
            console.error(`❌ ${this.constructor.name}: Command failed: ${command} - ${errorMessage}`);
            throw error;
        }
    }

    /**
     * Update connection status display
     */
    protected updateConnectionStatus(statusElement: HTMLElement): void {
        if (!this.api) return;

        const state = this.api.getConnectionState();
        const isConnected = state === 'connected';
        
        statusElement.textContent = isConnected 
            ? '🟢 Connected to localhost:9000' 
            : `🔴 ${state}`;
        
        statusElement.className = `connection-status ${state}`;
    }

    /**
     * Log widget messages with consistent formatting
     */
    protected log(message: string, level: 'info' | 'error' | 'warn' = 'info'): void {
        const prefix = `🎨 ${this.constructor.name}:`;
        
        switch (level) {
            case 'error':
                console.error(`${prefix} ${message}`);
                break;
            case 'warn':
                console.warn(`${prefix} ${message}`);
                break;
            default:
                console.log(`${prefix} ${message}`);
        }
    }
}

// Export to make this a module for global augmentation
export {};