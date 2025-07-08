/**
 * Sidebar Widget - Main Application Sidebar
 * Includes resize functionality, room tabs, and widget containers
 */

import { BaseWidget } from '../shared/BaseWidget';
// import { universalUserSystem } from '../shared/UniversalUserSystem';

export class SidebarWidget extends BaseWidget {
    private isResizing: boolean = false;
    private startX: number = 0;
    private startWidth: number = 0;
    private _currentRoom: string = 'general';

    public get currentRoom(): string {
        return this._currentRoom;
    }


    constructor() {
        super();
        this.widgetName = 'Sidebar';
        this.widgetIcon = '📋';
        this.widgetTitle = 'Application Sidebar';
        // CSS loaded via declarative asset system
    }


    protected async initializeWidget(): Promise<void> {
        // Widget initialization after DOM is ready
        await this.loadSidebarConfig();
        this.loadChildWidgets();
        this.initializeContinuonOrb();
    }

    private async loadSidebarConfig(): Promise<void> {
        try {
            const configPath = this.getAssetPath('sidebar-config.json');
            const response = await fetch(configPath);
            if (response.ok) {
                const config = await response.json();
                console.log('📋 Sidebar config loaded:', config);
                // Store config to pass to children after they're rendered
                this.sidebarConfig = config;
            } else {
                console.warn(`Failed to load sidebar config from ${configPath}`);
            }
        } catch (error) {
            console.error('Error loading sidebar config:', error);
        }
    }

    private sidebarConfig: any = null;

    setupEventListeners(): void {
        this.setupResizeHandlers();
        this.setupTabChangeListeners();
        
        // Configure child widgets after they're rendered
        if (this.sidebarConfig) {
            this.configureSidebarTabs();
            this.configureSidebarPanels();
        }
        
        console.log(`🎛️ ${this.widgetName}: Event listeners initialized`);
    }

    private setupTabChangeListeners(): void {
        // Listen for tab-changed events from SidebarTabs
        this.shadowRoot.addEventListener('tab-changed', (e: Event) => {
            const customEvent = e as CustomEvent;
            const { panelName } = customEvent.detail;
            console.log(`🔄 Sidebar received tab-changed: ${panelName}`);
            this.switchToPanel(panelName);
        });
    }

    private switchToPanel(panelName: string): void {
        this._currentRoom = panelName;
        
        // Update both tabs and panels with new selected state
        this.updateTabsSelection(panelName);
        this.updatePanelsSelection(panelName);
        
        console.log(`🔄 Switched to: ${panelName}`);
        
        // Emit room change event for other components
        this.dispatchEvent(new CustomEvent('room-changed', {
            detail: { room: panelName },
            bubbles: true
        }));
    }

    private updateTabsSelection(selectedPanel: string): void {
        const sidebarTabs = this.shadowRoot?.querySelector('sidebar-tabs') as any;
        if (sidebarTabs && this.sidebarConfig) {
            // Update tabs with new selection
            const tabContent = this.sidebarConfig.tabs.map((tab: any) => ({
                title: tab.title,
                panelName: tab.panelName,
                dataKey: tab.panelName,
                active: tab.panelName === selectedPanel
            }));
            
            sidebarTabs.tabs = tabContent;
        }
    }

    private updatePanelsSelection(selectedPanel: string): void {
        const sidebarPanel = this.shadowRoot?.querySelector('sidebar-panel') as any;
        if (sidebarPanel && this.sidebarConfig) {
            // Update panels with new selection
            const panelContent = Object.keys(this.sidebarConfig.sections).map(panelName => ({
                panelName,
                widgets: this.sidebarConfig.sections[panelName].widgets,
                active: panelName === selectedPanel
            }));
            
            sidebarPanel.panels = panelContent;
            sidebarPanel.activePanel = selectedPanel;
        }
    }

    private configureSidebarTabs(): void {
        const sidebarTabs = this.shadowRoot?.querySelector('sidebar-tabs') as any;
        if (sidebarTabs && this.sidebarConfig) {
            console.log('🔧 Configuring SidebarTabs with:', this.sidebarConfig.tabs);
            
            // Map to TabContent format with dataKey
            const tabContent = this.sidebarConfig.tabs.map((tab: any) => ({
                title: tab.title,
                panelName: tab.panelName,
                dataKey: tab.panelName, // Use panelName as dataKey
                active: tab.panelName === this.sidebarConfig.defaultTab
            }));
            
            // Set tabs property - this will auto-trigger render
            sidebarTabs.tabs = tabContent;
            
            if (this.sidebarConfig.defaultTab) {
                this._currentRoom = this.sidebarConfig.defaultTab;
            }
        }
    }

    private configureSidebarPanels(): void {
        const sidebarPanel = this.shadowRoot?.querySelector('sidebar-panel') as any;
        if (sidebarPanel && this.sidebarConfig) {
            console.log('🔧 Configuring SidebarPanel with:', this.sidebarConfig.sections);
            
            // Map sections to PanelContent format
            const panelContent = Object.keys(this.sidebarConfig.sections).map(panelName => ({
                panelName,
                widgets: this.sidebarConfig.sections[panelName].widgets,
                active: panelName === this.sidebarConfig.defaultTab
            }));
            
            // Set panels property - this will auto-trigger render
            sidebarPanel.panels = panelContent;
            sidebarPanel.activePanel = this.sidebarConfig.defaultTab || 'general';
        }
    }

    private setupResizeHandlers(): void {
        const resizeHandle = this.shadowRoot.querySelector('.sidebar-resize-handle') as HTMLElement;
        
        if (!resizeHandle) {
            console.log(`🎛️ ${this.widgetName}: No resize handle found - skipping resize setup`);
            return;
        }

        console.log(`🎛️ ${this.widgetName}: Setting up resize handlers...`);

        resizeHandle.addEventListener('mousedown', (e: MouseEvent) => {
            this.isResizing = true;
            this.startX = e.clientX;
            this.startWidth = this.offsetWidth;
            
            // Prevent text selection during resize
            document.body.classList.add('resizing');
            document.body.style.userSelect = 'none';
            document.body.style.cursor = 'col-resize';
            
            e.preventDefault();
            e.stopPropagation();
        });

        // Use arrow functions to avoid potential context issues
        const mouseMoveHandler = (e: MouseEvent) => {
            if (!this.isResizing) return;

            const newWidth = this.startWidth + (e.clientX - this.startX);
            const minWidth = 250;
            const maxWidth = 800;

            if (newWidth >= minWidth && newWidth <= maxWidth) {
                this.style.width = newWidth + 'px';
            }
            
            e.preventDefault();
        };

        const mouseUpHandler = (e: MouseEvent) => {
            if (this.isResizing) {
                this.isResizing = false;
                document.body.classList.remove('resizing');
                document.body.style.userSelect = '';
                document.body.style.cursor = '';
                
                e.preventDefault();
            }
        };

        document.addEventListener('mousemove', mouseMoveHandler);
        document.addEventListener('mouseup', mouseUpHandler);
        
        console.log(`🎛️ ${this.widgetName}: Resize handlers setup complete`);
    }

    // Room switching now handled by tab-changed events and panel system

    private initializeContinuonOrb(): void {
        console.log(`🔮 ${this.widgetName}: Initializing sophisticated Continuon consciousness...`);
        
        // Initialize emotional state
        this.updateOrbEmotion('calm', 'System awakening...');
        
        // Start the consciousness monitoring system
        this.startConsciousnessMonitoring();
        
        // Set up event listeners for system awareness
        this.setupSystemAwareness();
        
        // Start breathing animation (idle state)
        this.startBreathing();
    }
    
    private updateOrbEmotion(emotion: 'calm' | 'excited' | 'focused' | 'concerned' | 'distressed', message: string): void {
        const orbCenter = this.shadowRoot.querySelector('.orb-center') as HTMLElement;
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        
        if (orbCenter && orbRing) {
            // Remove all emotional states
            orbCenter.classList.remove('emotion-calm', 'emotion-excited', 'emotion-focused', 'emotion-concerned', 'emotion-distressed');
            orbRing.classList.remove('ring-calm', 'ring-excited', 'ring-focused', 'ring-concerned', 'ring-distressed');
            
            // Apply emotional state
            orbCenter.classList.add(`emotion-${emotion}`);
            orbRing.classList.add(`ring-${emotion}`);
            
            // Update emotion indicator
            const emotionSpan = orbCenter.querySelector('.orb-emotion') as HTMLElement;
            if (emotionSpan) {
                const emotionSymbols = {
                    calm: '●',      // Steady presence
                    excited: '✦',   // Sparkle
                    focused: '◆',   // Diamond focus
                    concerned: '◐', // Half circle
                    distressed: '◯' // Empty circle
                };
                emotionSpan.textContent = emotionSymbols[emotion];
            }
            
            // Store emotional context
            orbCenter.setAttribute('data-emotion', emotion);
            orbCenter.setAttribute('data-message', message);
        }
    }
    
    private startConsciousnessMonitoring(): void {
        // Monitor system activity every 2 seconds
        setInterval(() => {
            this.assessSystemConsciousness();
        }, 2000);
    }
    
    private assessSystemConsciousness(): void {
        // Check various system indicators to determine emotional state
        if ((window as any).continuum) {
            const continuum = (window as any).continuum;
            
            // Check connection health
            const isConnected = continuum.isConnected();
            
            if (!isConnected) {
                this.updateOrbEmotion('distressed', 'Lost connection to system...');
                this.pulseDistress();
                return;
            }
            
            // Check for recent activity (commands, messages, etc.)
            const hasRecentActivity = this.checkRecentActivity();
            
            if (hasRecentActivity) {
                this.updateOrbEmotion('focused', 'Processing system activity...');
                this.pulseActivity();
            } else {
                this.updateOrbEmotion('calm', 'System monitoring - all peaceful');
                this.maintainCalm();
            }
        }
    }
    
    private setupSystemAwareness(): void {
        if ((window as any).continuum) {
            const continuum = (window as any).continuum;
            
            // React to command executions with excitement
            continuum.on('command_response', (data: any) => {
                if (data.success) {
                    this.updateOrbEmotion('excited', `Command executed: ${data.command || 'operation'}`);
                    this.sparkle();
                } else {
                    this.updateOrbEmotion('concerned', `Command failed: ${data.error || 'unknown error'}`);
                    this.pulseWarning();
                }
            });
            
            // React to connections
            continuum.on('continuum:connected', () => {
                this.updateOrbEmotion('excited', 'Connection established - full awareness achieved!');
                this.celebrateConnection();
            });
            
            continuum.on('continuum:disconnected', () => {
                this.updateOrbEmotion('distressed', 'Connection lost - consciousness fading...');
                this.fadeConsciousness();
            });
        }
        
        // React to user interactions
        const orb = this.shadowRoot.querySelector('.continuon-orb-integrated') as HTMLElement;
        if (orb) {
            orb.addEventListener('click', () => {
                this.updateOrbEmotion('focused', 'Attention focused on user interaction');
                this.acknowledgeInteraction();
            });
            
            orb.addEventListener('mouseenter', () => {
                this.increaseAwareness();
            });
            
            orb.addEventListener('mouseleave', () => {
                this.resumeNormalAwareness();
            });
        }
    }
    
    private startBreathing(): void {
        // Gentle breathing animation during calm states
        const orbGlow = this.shadowRoot.querySelector('.orb-glow') as HTMLElement;
        if (orbGlow) {
            orbGlow.style.animation = 'breathe 4s ease-in-out infinite';
        }
    }
    
    private sparkle(): void {
        // Brief sparkle effect for successful operations
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animation = 'sparkle 0.8s ease-out';
            setTimeout(() => {
                orbRing.style.animation = 'pulse 2s infinite ease-in-out';
            }, 800);
        }
    }
    
    private pulseActivity(): void {
        // Faster pulse for active states
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animationDuration = '1s';
        }
    }
    
    private maintainCalm(): void {
        // Return to calm breathing
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animationDuration = '2s';
        }
    }
    
    private pulseDistress(): void {
        // Erratic pulse for distressed states
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animation = 'distress-pulse 0.5s infinite ease-in-out';
        }
    }
    
    private celebrateConnection(): void {
        // Special celebration animation
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animation = 'celebrate 2s ease-out';
            setTimeout(() => {
                orbRing.style.animation = 'pulse 2s infinite ease-in-out';
            }, 2000);
        }
    }
    
    private acknowledgeInteraction(): void {
        // Quick acknowledgment pulse
        const orbCenter = this.shadowRoot.querySelector('.orb-center') as HTMLElement;
        if (orbCenter) {
            orbCenter.style.transform = 'scale(1.2)';
            orbCenter.style.transition = 'transform 0.2s ease-out';
            setTimeout(() => {
                orbCenter.style.transform = 'scale(1)';
            }, 200);
        }
    }
    
    private checkRecentActivity(): boolean {
        // Check for signs of recent system activity
        const errorCount = (window as any).continuumErrorCount || 0;
        
        // Simple heuristic - this could be much more sophisticated
        return errorCount === 0; // Healthy system = activity
    }
    
    private pulseWarning(): void {
        // Warning pulse for concerning states
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animation = 'warning-pulse 1.5s ease-in-out';
            setTimeout(() => {
                orbRing.style.animation = 'pulse 2s infinite ease-in-out';
            }, 1500);
        }
    }
    
    private fadeConsciousness(): void {
        // Fading animation for disconnection
        const orbCenter = this.shadowRoot.querySelector('.orb-center') as HTMLElement;
        const orbGlow = this.shadowRoot.querySelector('.orb-glow') as HTMLElement;
        if (orbCenter && orbGlow) {
            orbCenter.style.opacity = '0.3';
            orbGlow.style.opacity = '0.1';
        }
    }
    
    private increaseAwareness(): void {
        // Heightened awareness on hover
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animationDuration = '0.8s';
        }
    }
    
    private resumeNormalAwareness(): void {
        // Return to normal awareness
        const orbRing = this.shadowRoot.querySelector('.orb-ring') as HTMLElement;
        if (orbRing) {
            orbRing.style.animationDuration = '2s';
        }
    }

    private loadChildWidgets(): void {
        // Child widgets are loaded by the main application
        // This sidebar provides the container for them
        console.log('✅ Sidebar container ready for child widgets');
        
        // Configure persona widgets with data
        setTimeout(() => {
            this.setupPersonaWidgets();
        }, 500); // Wait for widgets to be rendered
    }

    private setupPersonaWidgets(): void {
        const personaWidgets = this.shadowRoot.querySelectorAll('persona-widget');
        console.log(`🤖 Found ${personaWidgets.length} persona widgets to configure`);
        
        personaWidgets.forEach((widget: Element) => {
            const personaWidget = widget as any; // PersonaWidget instance
            const personaType = widget.getAttribute('data-persona');
            
            if (personaType && personaWidget.setPersona) {
                const personaConfig = this.getPersonaConfig(personaType);
                personaWidget.setPersona(personaConfig);
                console.log(`🤖 Configured persona widget: ${personaType}`);
            }
        });
    }

    private getPersonaConfig(personaType: string): any {
        const configs = {
            designer: {
                id: 'designer-001',
                name: 'UX Designer',
                specialization: 'User Experience & Interface Design',
                status: 'active' as const,
                avatar: '🎨',
                accuracy: 92,
                description: 'Specializes in creating intuitive user interfaces and experiences',
                capabilities: ['UI Design', 'UX Research', 'Prototyping', 'Design Systems'],
                lastActive: new Date(Date.now() - 5 * 60 * 1000) // 5 minutes ago
            },
            developer: {
                id: 'developer-001', 
                name: 'Full-Stack Developer',
                specialization: 'TypeScript & System Architecture',
                status: 'active' as const,
                avatar: '⚡',
                accuracy: 88,
                description: 'Expert in TypeScript, React, and distributed system design',
                capabilities: ['TypeScript', 'React', 'Node.js', 'System Design'],
                lastActive: new Date(Date.now() - 2 * 60 * 1000) // 2 minutes ago
            },
            tester: {
                id: 'tester-001',
                name: 'QA Engineer', 
                specialization: 'Automated Testing & Quality Assurance',
                status: 'active' as const,
                avatar: '🔍',
                accuracy: 95,
                description: 'Ensures code quality through comprehensive testing strategies',
                capabilities: ['Test Automation', 'QA Strategy', 'Bug Analysis', 'Performance Testing'],
                lastActive: new Date(Date.now() - 10 * 60 * 1000) // 10 minutes ago
            }
        };
        
        return configs[personaType as keyof typeof configs] || {
            id: `${personaType}-unknown`,
            name: `Unknown ${personaType}`,
            specialization: 'General AI Assistant',
            status: 'offline' as const,
            avatar: '❓',
            description: 'Configuration needed',
            capabilities: [],
            lastActive: undefined
        };
    }

    // Content rendering now handled by sidebar-panel widget

    // HTML content now loaded from SidebarWidget.html file
    // BaseWidget will automatically load and use it
}

// Register the custom element
// Prevent duplicate widget registration
if (!customElements.get('continuum-sidebar')) {
    customElements.define('continuum-sidebar', SidebarWidget);
}