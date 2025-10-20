/**
 * ContinuumEmoter - HAL 9000 System Health + Activity Scroller
 * Left: Glowing orb showing global system health
 * Right: Auto-scrolling status feed showing AI activity
 */
class ContinuumEmoter extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.connectionStatus = 'initializing';
        this.health = 'unknown';
        this.lastUpdate = null;
        this.statusMessages = []; // For scrolling feed
        this.maxMessages = 10; // Keep last 10 messages
    }

    connectedCallback() {
        this.render();
        this.startHealthMonitoring();
        this.subscribeToAIEvents();
    }

    disconnectedCallback() {
        if (this.healthInterval) {
            clearInterval(this.healthInterval);
        }
        this.unsubscribeFromAIEvents();
    }

    /**
     * Subscribe to AI decision events for status feed
     */
    subscribeToAIEvents() {
        if (window.jtag && window.jtag.events) {
            console.log('🎭 ContinuumEmoter: Subscribing to AI events...');

            // Subscribe to all AI decision events
            const events = [
                'ai:decision:evaluating',
                'ai:decision:decided-respond',
                'ai:decision:decided-silent',
                'ai:response:generating',
                'ai:decision:checking-redundancy',
                'ai:response:posted',
                'ai:decision:error'
            ];

            events.forEach(eventName => {
                window.jtag.events.subscribe(eventName, (data) => {
                    this.handleAIEvent(eventName, data);
                });
            });
        }
    }

    unsubscribeFromAIEvents() {
        // TODO: Implement unsubscribe if needed
    }

    /**
     * Handle AI events and add to status feed
     */
    handleAIEvent(eventName, data) {
        const personaName = data.personaName || data.personaId || 'AI';
        let statusText = '';

        // Format status message based on event type
        if (eventName.includes('evaluating')) {
            statusText = `${personaName}: thinking...`;
        } else if (eventName.includes('decided-respond')) {
            statusText = `${personaName}: responding`;
        } else if (eventName.includes('decided-silent')) {
            statusText = `${personaName}: passed`;
        } else if (eventName.includes('generating')) {
            statusText = `${personaName}: generating...`;
        } else if (eventName.includes('checking')) {
            statusText = `${personaName}: checking...`;
        } else if (eventName.includes('posted')) {
            statusText = `${personaName}: posted ✓`;
        } else if (eventName.includes('error')) {
            statusText = `${personaName}: error ✗`;
        }

        if (statusText) {
            this.addStatusMessage(statusText);
        }
    }

    /**
     * Add a status message to the scrolling feed
     */
    addStatusMessage(message) {
        const timestamp = new Date().toLocaleTimeString('en-US', {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit'
        });

        this.statusMessages.unshift({ message, timestamp }); // Add to beginning

        // Keep only last N messages
        if (this.statusMessages.length > this.maxMessages) {
            this.statusMessages.pop();
        }

        this.updateStatusScroller();
    }

    /**
     * Update just the status scroller (not full re-render)
     */
    updateStatusScroller() {
        const scroller = this.shadowRoot.querySelector('.status-scroller');
        if (scroller) {
            scroller.innerHTML = this.statusMessages.map(({ message, timestamp }) => `
                <div class="status-message">
                    <span class="status-time">${timestamp}</span>
                    <span class="status-text">${message}</span>
                </div>
            `).join('');
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