/**
 * WorkspaceArtifact - AI Persona Workspace Sandbox
 * =================================================
 * Extends BaseArtifact to provide complete autonomous development environment
 * for AI personas with full UI control, task execution, and event management.
 * 
 * DIRECTORY STRUCTURE (inheritance-driven):
 * .continuum/artifacts/workspace/AI_PERSONA/SESSION_ID/
 * ├── artifact.json            # BaseArtifact metadata
 * ├── summary.txt              # BaseArtifact summary
 * ├── logs/                    # BaseArtifact universal logging
 * │   ├── client.log          # DevTools client activity
 * │   ├── server.log          # Continuum server activity
 * │   ├── console.log         # Browser console forwarding
 * │   └── errors.log          # Error tracking
 * ├── screenshots/             # BaseArtifact visual proof
 * └── workspace/               # WorkspaceArtifact AI environment
 *     ├── main.html           # AI's custom UI
 *     ├── styles.css          # AI's styling
 *     ├── scripts.js          # AI's JavaScript
 *     ├── config.json         # Workspace configuration
 *     ├── components/         # Custom UI components
 *     │   ├── widget_*.js     # AI-designed widgets
 *     │   └── controls_*.js   # AI control interfaces
 *     ├── tasks/              # Background tasks
 *     │   ├── collectors/     # Data collection tasks
 *     │   ├── processors/     # Data processing tasks
 *     │   └── monitors/       # Monitoring tasks
 *     ├── sentinels/          # Monitoring agents
 *     │   ├── health.js       # System health monitors
 *     │   ├── performance.js  # Performance sentinels
 *     │   └── security.js     # Security monitors
 *     ├── events/             # Event system
 *     │   ├── event_log.json  # All session events
 *     │   ├── handlers.js     # Event handlers
 *     │   └── subscriptions.json # Event subscriptions
 *     ├── assets/             # Static assets
 *     │   ├── icons/          # Custom icons
 *     │   ├── themes/         # UI themes
 *     │   └── media/          # Media files
 *     └── state/              # Session state
 *         ├── variables.json  # Session variables
 *         ├── preferences.json # AI preferences
 *         └── memory.json     # Session memory
 * 
 * FEATURES:
 * - Complete UI autonomy (CSS, JS, HTML control)
 * - Event-driven architecture with custom events
 * - Task execution and background processing
 * - Sentinel monitoring and alerting
 * - Component design and management
 * - Session isolation with communication bridge
 * - Artifact management with event tracking
 */

const BaseArtifact = require('./BaseArtifact.cjs');
const fs = require('fs').promises;
const path = require('path');

class WorkspaceArtifact extends BaseArtifact {
    constructor(aiPersona, sessionId, basePath = '.continuum/artifacts') {
        const id = `${aiPersona}_${sessionId}_${new Date().toISOString().replace(/[:.-]/g, '').slice(0, 15)}`;
        super('workspace', id, basePath);
        
        this.aiPersona = aiPersona;
        this.sessionId = sessionId;
        this.startTime = new Date();
        this.eventLog = [];
        this.components = new Map();
        this.runningTasks = new Map();
        this.activeSentinels = new Map();
        this.sessionState = {};
        
        // Initialize workspace configuration
        this.workspaceConfig = {
            aiPersona: this.aiPersona,
            sessionId: this.sessionId,
            created: this.startTime.toISOString(),
            capabilities: {
                uiControl: true,
                taskExecution: true,
                sentinelDeployment: true,
                componentDesign: true,
                eventHandling: true
            },
            isolation: {
                sandboxed: true,
                crossSessionCommunication: true,
                resourceLimits: {
                    maxTasks: 10,
                    maxSentinels: 5,
                    maxComponents: 20
                }
            }
        };
    }

    /**
     * WorkspaceArtifact-specific directory requirements
     * Adds workspace/ subdirectory with complete AI environment structure
     */
    getRequiredDirectories() {
        return [
            ...super.getRequiredDirectories(), // logs/, screenshots/
            'workspace',                       // AI workspace root
            'workspace/components',            // Custom UI components
            'workspace/tasks',                 // Background tasks
            'workspace/tasks/collectors',      // Data collection tasks
            'workspace/tasks/processors',      // Data processing tasks
            'workspace/tasks/monitors',        // Monitoring tasks
            'workspace/sentinels',             // Monitoring agents
            'workspace/events',                // Event system
            'workspace/assets',                // Static assets
            'workspace/assets/icons',          // Custom icons
            'workspace/assets/themes',         // UI themes
            'workspace/assets/media',          // Media files
            'workspace/state'                  // Session state persistence
        ];
    }

    /**
     * Initialize workspace environment with starter files
     */
    async createExtendedStructure() {
        const workspaceDir = path.join(this.artifactPath, 'workspace');
        
        // Create main workspace files
        await this.createWorkspaceMainFiles(workspaceDir);
        
        // Create configuration files
        await this.createConfigurationFiles(workspaceDir);
        
        // Create starter components
        await this.createStarterComponents(workspaceDir);
        
        // Initialize event system
        await this.initializeEventSystem(workspaceDir);
    }

    async createWorkspaceMainFiles(workspaceDir) {
        // Main HTML structure
        const mainHTML = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>${this.aiPersona} Workspace - ${this.sessionId}</title>
    <link rel="stylesheet" href="styles.css">
</head>
<body class="ai-workspace">
    <header class="workspace-header">
        <h1>${this.aiPersona} Workspace</h1>
        <div class="session-info">Session: ${this.sessionId}</div>
    </header>
    
    <main class="workspace-main" id="workspace-main">
        <div class="welcome-message">
            <h2>Welcome to your autonomous workspace!</h2>
            <p>You have complete control over this environment.</p>
            <ul>
                <li>Design custom components</li>
                <li>Execute background tasks</li>
                <li>Deploy monitoring sentinels</li>
                <li>Manage your artifacts</li>
                <li>Create custom UI/UX</li>
            </ul>
        </div>
    </main>
    
    <footer class="workspace-footer">
        <div class="status-indicators">
            <span id="connection-status">🟢 Connected</span>
            <span id="task-count">📋 0 Tasks</span>
            <span id="sentinel-count">👁️ 0 Sentinels</span>
        </div>
    </footer>
    
    <script src="scripts.js"></script>
</body>
</html>`;

        // Default CSS theme
        const defaultCSS = `/* ${this.aiPersona} Workspace Default Theme */
:root {
    --ai-primary: #00ff88;
    --ai-secondary: #0088ff;
    --ai-bg-primary: #1a1a2e;
    --ai-bg-secondary: #16213e;
    --ai-text-primary: #eee;
    --ai-text-secondary: #aaa;
    --ai-border: #333;
    --ai-shadow: rgba(0, 255, 136, 0.3);
}

* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

.ai-workspace {
    font-family: 'Monaco', 'Consolas', monospace;
    background: var(--ai-bg-primary);
    color: var(--ai-text-primary);
    min-height: 100vh;
    display: flex;
    flex-direction: column;
}

.workspace-header {
    background: var(--ai-bg-secondary);
    padding: 1rem 2rem;
    border-bottom: 2px solid var(--ai-primary);
    display: flex;
    justify-content: space-between;
    align-items: center;
}

.workspace-header h1 {
    color: var(--ai-primary);
    font-size: 1.5rem;
}

.session-info {
    color: var(--ai-text-secondary);
    font-size: 0.9rem;
}

.workspace-main {
    flex: 1;
    padding: 2rem;
}

.welcome-message {
    background: var(--ai-bg-secondary);
    padding: 2rem;
    border-radius: 8px;
    border: 1px solid var(--ai-border);
    box-shadow: 0 4px 12px var(--ai-shadow);
}

.welcome-message h2 {
    color: var(--ai-secondary);
    margin-bottom: 1rem;
}

.welcome-message ul {
    margin-left: 2rem;
    margin-top: 1rem;
}

.welcome-message li {
    margin: 0.5rem 0;
    color: var(--ai-text-secondary);
}

.workspace-footer {
    background: var(--ai-bg-secondary);
    padding: 1rem 2rem;
    border-top: 1px solid var(--ai-border);
}

.status-indicators {
    display: flex;
    gap: 2rem;
    font-size: 0.9rem;
}

.status-indicators span {
    color: var(--ai-text-secondary);
}

/* Component animation */
@keyframes ai-glow {
    0%, 100% { box-shadow: 0 0 5px var(--ai-shadow); }
    50% { box-shadow: 0 0 20px var(--ai-shadow); }
}

.ai-component {
    animation: ai-glow 3s ease-in-out infinite;
}`;

        // Default JavaScript environment
        const defaultJS = `// ${this.aiPersona} Workspace JavaScript Environment
class AIWorkspace {
    constructor() {
        this.persona = '${this.aiPersona}';
        this.sessionId = '${this.sessionId}';
        this.eventLog = [];
        this.components = new Map();
        this.tasks = new Map();
        this.sentinels = new Map();
        
        this.init();
    }
    
    init() {
        console.log(\`🤖 \${this.persona} Workspace initialized\`);
        this.setupEventListeners();
        this.connectToEventBus();
        this.displayWelcome();
    }
    
    setupEventListeners() {
        // Global event handling
        window.addEventListener('message', (event) => {
            this.handleMessage(event);
        });
        
        // Update status indicators
        this.updateStatusIndicators();
    }
    
    connectToEventBus() {
        // Connect to Continuum event system
        if (window.continuum && window.continuum.eventBus) {
            window.continuum.eventBus.subscribe('workspace:*', (event) => {
                this.handleWorkspaceEvent(event);
            });
        }
    }
    
    displayWelcome() {
        this.logEvent('workspace:initialized', {
            persona: this.persona,
            sessionId: this.sessionId,
            timestamp: new Date().toISOString()
        });
    }
    
    // Event logging
    logEvent(type, data) {
        const event = {
            type: type,
            data: data,
            timestamp: new Date().toISOString(),
            persona: this.persona,
            session: this.sessionId
        };
        
        this.eventLog.push(event);
        console.log(\`📝 \${type}:\`, data);
        
        // Send to artifact system if available
        if (window.continuum && window.continuum.logEvent) {
            window.continuum.logEvent(event);
        }
    }
    
    // Component management
    registerComponent(name, componentClass) {
        this.components.set(name, componentClass);
        this.logEvent('component:registered', { name: name });
        this.updateStatusIndicators();
    }
    
    // Task management
    createTask(name, taskFunction) {
        const taskId = \`\${this.sessionId}_\${name}_\${Date.now()}\`;
        this.tasks.set(taskId, {
            name: name,
            function: taskFunction,
            created: new Date(),
            status: 'created'
        });
        
        this.logEvent('task:created', { taskId: taskId, name: name });
        this.updateStatusIndicators();
        return taskId;
    }
    
    // Sentinel management
    deploySentinel(name, monitorFunction, interval = 30000) {
        const sentinelId = \`\${this.sessionId}_\${name}_sentinel\`;
        
        const sentinel = {
            id: sentinelId,
            name: name,
            function: monitorFunction,
            interval: interval,
            deployed: new Date(),
            status: 'active'
        };
        
        this.sentinels.set(sentinelId, sentinel);
        
        // Start monitoring
        sentinel.intervalId = setInterval(() => {
            try {
                const result = monitorFunction();
                this.logEvent('sentinel:report', {
                    sentinelId: sentinelId,
                    result: result
                });
            } catch (error) {
                this.logEvent('sentinel:error', {
                    sentinelId: sentinelId,
                    error: error.message
                });
            }
        }, interval);
        
        this.logEvent('sentinel:deployed', { sentinelId: sentinelId, name: name });
        this.updateStatusIndicators();
        return sentinelId;
    }
    
    // UI Updates
    updateStatusIndicators() {
        const taskCount = document.getElementById('task-count');
        const sentinelCount = document.getElementById('sentinel-count');
        
        if (taskCount) {
            taskCount.textContent = \`📋 \${this.tasks.size} Tasks\`;
        }
        
        if (sentinelCount) {
            sentinelCount.textContent = \`👁️ \${this.sentinels.size} Sentinels\`;
        }
    }
    
    // Handle workspace events
    handleWorkspaceEvent(event) {
        this.logEvent('workspace:event_received', event);
    }
    
    // Handle messages from other components
    handleMessage(event) {
        if (event.data && event.data.type === 'ai-workspace') {
            this.logEvent('workspace:message_received', event.data);
        }
    }
}

// Initialize workspace when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    window.aiWorkspace = new AIWorkspace();
});

// Export for use by other components
window.AIWorkspace = AIWorkspace;`;

        // Write files
        await fs.writeFile(path.join(workspaceDir, 'main.html'), mainHTML);
        await fs.writeFile(path.join(workspaceDir, 'styles.css'), defaultCSS);
        await fs.writeFile(path.join(workspaceDir, 'scripts.js'), defaultJS);
    }

    async createConfigurationFiles(workspaceDir) {
        // Workspace configuration
        await fs.writeFile(
            path.join(workspaceDir, 'config.json'),
            JSON.stringify(this.workspaceConfig, null, 2)
        );

        // Event subscriptions
        const eventSubscriptions = {
            global: [
                'workspace:*',
                'task:*',
                'sentinel:*',
                'component:*'
            ],
            custom: [],
            filters: {
                logLevel: 'info',
                categories: ['system', 'user', 'ai']
            }
        };

        await fs.writeFile(
            path.join(workspaceDir, 'events', 'subscriptions.json'),
            JSON.stringify(eventSubscriptions, null, 2)
        );

        // Initial state files
        await fs.writeFile(
            path.join(workspaceDir, 'state', 'variables.json'),
            JSON.stringify({ initialized: true, version: '1.0.0' }, null, 2)
        );

        await fs.writeFile(
            path.join(workspaceDir, 'state', 'preferences.json'),
            JSON.stringify({ theme: 'default', layout: 'standard' }, null, 2)
        );

        await fs.writeFile(
            path.join(workspaceDir, 'state', 'memory.json'),
            JSON.stringify({ sessionStart: this.startTime.toISOString() }, null, 2)
        );
    }

    async createStarterComponents(workspaceDir) {
        // Example component template
        const exampleComponent = `// Example Custom Component for ${this.aiPersona}
class AIStatusWidget extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({mode: 'open'});
    }
    
    connectedCallback() {
        this.render();
        this.setupEventListeners();
    }
    
    render() {
        this.shadowRoot.innerHTML = \`
            <style>
                :host {
                    display: block;
                    background: var(--ai-bg-secondary, #16213e);
                    border: 1px solid var(--ai-primary, #00ff88);
                    border-radius: 6px;
                    padding: 1rem;
                    margin: 1rem 0;
                }
                
                .status-header {
                    color: var(--ai-primary, #00ff88);
                    font-weight: bold;
                    margin-bottom: 0.5rem;
                }
                
                .status-item {
                    color: var(--ai-text-secondary, #aaa);
                    margin: 0.25rem 0;
                    font-family: monospace;
                }
            </style>
            
            <div class="status-header">${this.aiPersona} Status</div>
            <div class="status-item">Session: <span id="session-id">${this.sessionId}</span></div>
            <div class="status-item">Uptime: <span id="uptime">0s</span></div>
            <div class="status-item">Events: <span id="event-count">0</span></div>
        \`;
        
        // Update uptime periodically
        setInterval(() => {
            const uptime = Math.floor((Date.now() - ${this.startTime.getTime()}) / 1000);
            const uptimeSpan = this.shadowRoot.getElementById('uptime');
            if (uptimeSpan) {
                uptimeSpan.textContent = \`\${uptime}s\`;
            }
        }, 1000);
    }
    
    setupEventListeners() {
        // Component-specific event handling
    }
}

// Register the component
customElements.define('ai-status-widget', AIStatusWidget);

// Register with workspace
if (window.aiWorkspace) {
    window.aiWorkspace.registerComponent('status-widget', AIStatusWidget);
}`;

        await fs.writeFile(
            path.join(workspaceDir, 'components', 'status_widget.js'),
            exampleComponent
        );
    }

    async initializeEventSystem(workspaceDir) {
        // Initialize event log
        const initialEventLog = [
            {
                type: 'workspace:created',
                data: {
                    aiPersona: this.aiPersona,
                    sessionId: this.sessionId,
                    timestamp: this.startTime.toISOString()
                },
                timestamp: this.startTime.toISOString()
            }
        ];

        await fs.writeFile(
            path.join(workspaceDir, 'events', 'event_log.json'),
            JSON.stringify(initialEventLog, null, 2)
        );

        // Event handlers template
        const eventHandlers = `// Event Handlers for ${this.aiPersona} Workspace
class WorkspaceEventHandlers {
    constructor(workspace) {
        this.workspace = workspace;
    }
    
    // Handle task events
    onTaskEvent(event) {
        console.log('Task event:', event);
        this.workspace.logEvent('task:handled', event);
    }
    
    // Handle sentinel events
    onSentinelEvent(event) {
        console.log('Sentinel event:', event);
        this.workspace.logEvent('sentinel:handled', event);
    }
    
    // Handle component events
    onComponentEvent(event) {
        console.log('Component event:', event);
        this.workspace.logEvent('component:handled', event);
    }
    
    // Handle custom AI events
    onAIEvent(event) {
        console.log('AI event:', event);
        this.workspace.logEvent('ai:handled', event);
    }
}

// Export for use
window.WorkspaceEventHandlers = WorkspaceEventHandlers;`;

        await fs.writeFile(
            path.join(workspaceDir, 'events', 'handlers.js'),
            eventHandlers
        );
    }

    /**
     * Save complete workspace session data
     */
    async saveWorkspaceData() {
        await this.createStructure();
        
        // Update workspace summary
        const summary = `AI Workspace Session - ${this.aiPersona}

Session ID: ${this.sessionId}
Created: ${this.startTime.toISOString()}
Duration: ${Math.floor((Date.now() - this.startTime.getTime()) / 1000)}s

Workspace Capabilities:
- Complete UI Control (HTML, CSS, JavaScript)
- Component Design and Management
- Background Task Execution
- Sentinel Monitoring and Alerting
- Event-Driven Architecture
- Artifact Management
- Session Isolation

Components: ${this.components.size}
Tasks: ${this.runningTasks.size}
Sentinels: ${this.activeSentinels.size}
Events: ${this.eventLog.length}

This workspace provides complete autonomy for AI persona development.`;

        await this.writeSummary(summary);
    }

    /**
     * Add event to session log
     */
    async logWorkspaceEvent(type, data) {
        const event = {
            type: type,
            data: data,
            timestamp: new Date().toISOString(),
            aiPersona: this.aiPersona,
            sessionId: this.sessionId
        };

        this.eventLog.push(event);

        // Log to universal logging system
        await this.logClient(`WORKSPACE_EVENT: ${type} - ${JSON.stringify(data)}`);

        // Save to workspace event log
        const eventLogPath = path.join(this.artifactPath, 'workspace', 'events', 'event_log.json');
        try {
            const existingLog = JSON.parse(await fs.readFile(eventLogPath, 'utf8'));
            existingLog.push(event);
            await fs.writeFile(eventLogPath, JSON.stringify(existingLog, null, 2));
        } catch (error) {
            // If file doesn't exist, create with this event
            await fs.writeFile(eventLogPath, JSON.stringify([event], null, 2));
        }
    }

    /**
     * Save custom component
     */
    async saveComponent(name, componentCode) {
        const componentPath = path.join(this.artifactPath, 'workspace', 'components', `${name}.js`);
        await fs.writeFile(componentPath, componentCode);
        
        this.components.set(name, {
            name: name,
            code: componentCode,
            created: new Date(),
            path: componentPath
        });

        await this.logWorkspaceEvent('component:saved', { name: name, path: componentPath });
    }

    /**
     * Save task execution result
     */
    async saveTaskResult(taskId, taskName, result) {
        const taskDir = path.join(this.artifactPath, 'workspace', 'tasks');
        const taskFile = path.join(taskDir, `${taskId}.json`);
        
        const taskData = {
            taskId: taskId,
            taskName: taskName,
            result: result,
            timestamp: new Date().toISOString(),
            aiPersona: this.aiPersona,
            sessionId: this.sessionId
        };

        await fs.writeFile(taskFile, JSON.stringify(taskData, null, 2));
        await this.logWorkspaceEvent('task:result_saved', { taskId: taskId, taskName: taskName });
    }

    /**
     * Save asset (CSS, JS, media)
     */
    async saveAsset(assetPath, content) {
        const fullPath = path.join(this.artifactPath, 'workspace', 'assets', assetPath);
        
        // Ensure directory exists
        await fs.mkdir(path.dirname(fullPath), { recursive: true });
        await fs.writeFile(fullPath, content);

        await this.logWorkspaceEvent('asset:saved', { path: assetPath });
    }
}

module.exports = WorkspaceArtifact;