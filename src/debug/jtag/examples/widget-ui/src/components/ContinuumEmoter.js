/**
 * ContinuumEmoter - Connection status indicator with JTAG integration
 * Shows real-time system health and connection status
 */
class ContinuumEmoter extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.connectionStatus = 'initializing';
        this.health = 'unknown';
        this.lastUpdate = null;
    }

    connectedCallback() {
        this.render();
        this.startHealthMonitoring();
    }

    disconnectedCallback() {
        if (this.healthInterval) {
            clearInterval(this.healthInterval);
        }
    }

    startHealthMonitoring() {
        // Check health every 5 seconds using JTAG system
        this.healthInterval = setInterval(async () => {
            try {
                await this.checkSystemHealth();
            } catch (error) {
                console.warn('ContinuumEmoter: Health check failed:', error.message);
                this.updateStatus('disconnected', 'error');
            }
        }, 5000);

        // Initial health check
        setTimeout(() => this.checkSystemHealth(), 1000);
    }

    async checkSystemHealth() {
        try {
            // Use JTAG system health check if available
            if (window.jtag && window.jtag.commands) {
                const healthResult = await window.jtag.commands.health();
                if (healthResult && healthResult.success) {
                    this.updateStatus('connected', 'healthy');
                    this.lastUpdate = new Date().toLocaleTimeString();
                } else {
                    this.updateStatus('connected', 'warning');
                }
            } else {
                // Fallback to basic connectivity check
                const response = await fetch('/health', { 
                    method: 'GET',
                    timeout: 3000 
                });
                if (response.ok) {
                    this.updateStatus('connected', 'healthy');
                } else {
                    this.updateStatus('connected', 'warning');
                }
            }
        } catch (error) {
            this.updateStatus('disconnected', 'error');
        }
    }

    updateStatus(connectionStatus, health) {
        if (this.connectionStatus !== connectionStatus || this.health !== health) {
            this.connectionStatus = connectionStatus;
            this.health = health;
            this.render();
        }
    }

    getStatusConfig() {
        const configs = {
            'connected-healthy': {
                emoji: '🟢',
                text: 'continuum',
                class: 'status-connected-healthy'
            },
            'connected-warning': {
                emoji: '🟡',
                text: 'continuum',
                class: 'status-connected-warning'
            },
            'disconnected-error': {
                emoji: '🔴',
                text: 'continuum',
                class: 'status-disconnected-error'
            },
            'initializing-unknown': {
                emoji: '⚪',
                text: 'continuum',
                class: 'status-initializing'
            }
        };

        const key = `${this.connectionStatus}-${this.health}`;
        return configs[key] || configs['initializing-unknown'];
    }

    render() {
        const config = this.getStatusConfig();
        
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    display: block;
                    font-size: 1.1rem;
                    font-weight: 600;
                    text-align: center;
                    padding: 12px;
                    border-radius: 8px;
                    transition: all 0.3s ease;
                    cursor: pointer;
                    user-select: none;
                }

                .status-connected-healthy {
                    color: #00ff64;
                    background: rgba(0, 255, 100, 0.1);
                    border: 1px solid rgba(0, 255, 100, 0.3);
                    box-shadow: 0 0 15px rgba(0, 255, 100, 0.2);
                }

                .status-connected-warning {
                    color: #ffaa00;
                    background: rgba(255, 170, 0, 0.1);
                    border: 1px solid rgba(255, 170, 0, 0.3);
                    box-shadow: 0 0 15px rgba(255, 170, 0, 0.2);
                }

                .status-disconnected-error {
                    color: #ff0096;
                    background: rgba(255, 0, 150, 0.1);
                    border: 1px solid rgba(255, 0, 150, 0.3);
                    box-shadow: 0 0 15px rgba(255, 0, 150, 0.2);
                }

                .status-initializing {
                    color: #8a92a5;
                    background: rgba(255, 255, 255, 0.05);
                    border: 1px solid rgba(255, 255, 255, 0.1);
                    animation: pulse 2s infinite;
                }

                @keyframes pulse {
                    0%, 100% { opacity: 1; }
                    50% { opacity: 0.6; }
                }

                :host(:hover) {
                    transform: translateY(-2px);
                    filter: brightness(1.1);
                }

                .emoter-content {
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    gap: 8px;
                }

                .status-text {
                    font-family: inherit;
                }

                .last-update {
                    font-size: 0.7rem;
                    opacity: 0.6;
                    margin-top: 4px;
                }
            </style>
            <div class="emoter-content ${config.class}">
                <span class="status-emoji">${config.emoji}</span>
                <span class="status-text">${config.text}</span>
            </div>
            ${this.lastUpdate ? `<div class="last-update">Updated: ${this.lastUpdate}</div>` : ''}
        `;

        // Add click handler for JTAG debug panel
        this.onclick = () => this.openDebugPanel();
    }

    async openDebugPanel() {
        try {
            if (window.jtag && window.jtag.commands) {
                // Get system status for debug panel
                const healthData = await window.jtag.commands.health();
                const debugInfo = {
                    connection: this.connectionStatus,
                    health: this.health,
                    lastUpdate: this.lastUpdate,
                    systemHealth: healthData
                };
                
                console.log('🔧 ContinuumEmoter Debug Panel:', debugInfo);
                
                // Could open a modal or drawer with debug info
                alert(`Continuum Status Debug:
Connection: ${this.connectionStatus}
Health: ${this.health}
Last Update: ${this.lastUpdate || 'Never'}
System Health: ${JSON.stringify(healthData, null, 2)}`);
            } else {
                console.log('🔧 ContinuumEmoter: JTAG system not available for debug panel');
            }
        } catch (error) {
            console.error('🔧 ContinuumEmoter Debug Panel Error:', error);
        }
    }
}

// Register the custom element
customElements.define('continuum-emoter', ContinuumEmoter);

export { ContinuumEmoter };