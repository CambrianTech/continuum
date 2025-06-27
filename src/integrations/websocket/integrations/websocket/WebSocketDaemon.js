/**
 * WebSocket Daemon - TypeScript Implementation
 * Extends BaseDaemon with WebSocket server functionality
 */
import { BaseDaemon } from '../../daemons/base/BaseDaemon.js';
import { WebSocketServer } from 'ws';
import { ConnectionManager } from './core/ConnectionManager';
import { DynamicMessageRouter } from './core/DynamicMessageRouter';
import { DaemonConnector } from './core/DaemonConnector';
import { BrowserManager } from '../../core/BrowserManager.js';
import { createServer } from 'http';
export class WebSocketDaemon extends BaseDaemon {
    constructor(config = {}) {
        super();
        this.name = 'websocket-server';
        this.version = '1.0.0';
        this.server = null;
        this.httpServer = null;
        this.registeredDaemons = new Map();
        this.config = {
            port: config.port ?? 9000,
            host: config.host ?? 'localhost',
            maxClients: config.maxClients ?? 100,
            enableHeartbeat: config.enableHeartbeat ?? true,
            enableAuth: config.enableAuth ?? false,
            daemonConfig: {
                autoConnect: false, // Disable until module paths fixed
                enableFallback: false,
                retryAttempts: 3,
                retryInterval: 5000,
                ...config.daemonConfig
            }
        };
        // Initialize modular components
        this.connectionManager = new ConnectionManager({
            maxClients: this.config.maxClients,
            enableHeartbeat: this.config.enableHeartbeat,
            enableAuth: this.config.enableAuth
        });
        this.messageRouter = new DynamicMessageRouter();
        this.daemonConnector = new DaemonConnector(this.config.daemonConfig);
        this.browserManager = new BrowserManager(this.config.port);
        this.setupEventHandlers();
    }
    async onStart() {
        this.log(`Starting WebSocket server on ${this.config.host}:${this.config.port}`);
        // Register self as a daemon in the router
        await this.messageRouter.registerDaemon(this.name, this);
        this.log(`✅ Registered self (${this.name}) with dynamic router`);
        // Start daemon connector first
        if (this.config.daemonConfig.autoConnect) {
            await this.daemonConnector.connect();
        }
        // Start HTTP server first, then attach WebSocket
        try {
            // Create HTTP server
            this.httpServer = createServer((req, res) => {
                this.handleHttpRequest(req, res);
            });
            // Create WebSocket server attached to HTTP server
            this.server = new WebSocketServer({
                server: this.httpServer
            });
            this.server.on('connection', (socket, request) => {
                this.handleConnection(socket, request);
            });
            this.server.on('error', (error) => {
                this.log(`❌ WebSocket server error: ${error.message}`, 'error');
                this.emit('error', error);
                throw error;
            });
            // Start HTTP server listening
            this.httpServer.listen(this.config.port, this.config.host, () => {
                this.log(`✅ HTTP server listening on http://${this.config.host}:${this.config.port}`);
                this.log(`✅ WebSocket server ACTUALLY listening on ws://${this.config.host}:${this.config.port}`);
            });
            // Wait for server to actually start listening
            await new Promise((resolve, reject) => {
                this.httpServer.on('listening', resolve);
                this.httpServer.on('error', reject);
                // Timeout after 5 seconds
                setTimeout(() => reject(new Error('HTTP/WebSocket server failed to start listening')), 5000);
            });
            this.log(`✅ WebSocket server started and verified listening on ws://${this.config.host}:${this.config.port}`);
        }
        catch (error) {
            this.log(`❌ Failed to start WebSocket server: ${error.message}`, 'error');
            throw error;
        }
    }
    async onStop() {
        this.log('Stopping WebSocket server...');
        // Stop daemon connector
        await this.daemonConnector.disconnect();
        // Cleanup browser manager
        await this.browserManager.cleanup();
        // Shutdown connection manager
        this.connectionManager.shutdown();
        // Close WebSocket server
        if (this.server) {
            await new Promise((resolve) => {
                this.server.close(() => {
                    this.log('✅ WebSocket server stopped');
                    resolve();
                });
            });
            this.server = null;
        }
        // Close HTTP server
        if (this.httpServer) {
            await new Promise((resolve) => {
                this.httpServer.close(() => {
                    this.log('✅ HTTP server stopped');
                    resolve();
                });
            });
            this.httpServer = null;
        }
    }
    async handleMessage(message) {
        switch (message.type) {
            case 'get_stats':
                return {
                    success: true,
                    data: this.getStats()
                };
            case 'get_clients':
                return {
                    success: true,
                    data: this.connectionManager.getAllClients().map(client => ({
                        id: client.id,
                        connected: client.connected,
                        connectTime: client.connectTime,
                        lastActivity: client.lastActivity,
                        metadata: client.metadata
                    }))
                };
            case 'send_message':
                return this.handleSendMessage(message.data);
            case 'broadcast_message':
                return this.handleBroadcastMessage(message.data);
            case 'tabRegister':
                return this.handleTabRegister(message.data);
            case 'get_component_css':
                return this.handleGetComponentCSS(message.data);
            case 'get_capabilities':
                return {
                    success: true,
                    data: {
                        capabilities: [
                            'websocket-server',
                            'client-management',
                            'api-endpoints',
                            'css-service'
                        ]
                    }
                };
            case 'ping':
                return {
                    success: true,
                    data: {
                        type: 'pong',
                        timestamp: new Date().toISOString(),
                        server: 'websocket-daemon'
                    }
                };
            case 'pong':
                return {
                    success: true,
                    data: {
                        received: true,
                        timestamp: new Date().toISOString()
                    }
                };
            default:
                return {
                    success: false,
                    error: `Unknown message type: ${message.type}`
                };
        }
    }
    handleConnection(socket, request) {
        try {
            const clientMetadata = {
                userAgent: request.headers['user-agent'],
                origin: request.headers.origin,
                remoteAddress: request.socket.remoteAddress,
                url: request.url
            };
            const clientId = this.connectionManager.addClient(socket, clientMetadata);
            // Register with browser manager
            this.browserManager.registerClient({
                clientId,
                ...clientMetadata,
                capabilities: []
            });
            socket.on('message', (data) => {
                this.handleClientMessage(clientId, data);
                // Update activity in browser manager
                this.browserManager.updateClientActivity(clientId);
            });
            socket.on('close', () => {
                this.browserManager.removeClient(clientId);
            });
            // Send connection confirmation
            this.sendToClient(clientId, {
                type: 'connection_confirmed',
                data: {
                    clientId,
                    server: this.name,
                    version: this.version,
                    daemon: this.daemonConnector.isConnected(),
                    browserState: this.browserManager.getBrowserState()
                },
                timestamp: new Date().toISOString()
            });
        }
        catch (error) {
            this.log(`Failed to handle connection: ${error.message}`, 'error');
            socket.close(1011, 'Server error');
        }
    }
    async handleHttpRequest(req, res) {
        const url = new URL(req.url, `http://${this.config.host}:${this.config.port}`);
        // Serve HTML interface via RendererDaemon
        if (req.method === 'GET' && url.pathname === '/') {
            try {
                // Try to get UI from RendererDaemon
                const rendererResponse = await this.requestFromDaemon('renderer', {
                    type: 'render_request',
                    data: {
                        type: 'render_ui',
                        page: 'main',
                        clientId: 'http-browser'
                    }
                });
                this.log(`Renderer response: ${JSON.stringify(rendererResponse)}`);
                // Serve test HTML to verify basic serving works
                const { readFile } = await import('fs/promises');
                const { join } = await import('path');
                const htmlPath = join(process.cwd(), 'test-ui.html');
                const htmlContent = await readFile(htmlPath, 'utf-8');
                res.writeHead(200, { 'Content-Type': 'text/html' });
                res.end(htmlContent);
            }
            catch (error) {
                this.log(`Failed to serve TypeScript UI: ${error.message}`, 'error');
                res.writeHead(500, { 'Content-Type': 'text/plain' });
                res.end('Error loading TypeScript UI');
            }
        }
        else if (req.method === 'GET' && url.pathname.startsWith('/src/')) {
            // Serve static files from src directory (continuum-api.js, components, etc.)
            await this.serveStaticFile(url.pathname, res);
        }
        else if (req.method === 'GET' && url.pathname === '/health') {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ status: 'healthy', server: this.name, version: this.version }));
        }
        else if (req.method === 'GET' && url.pathname === '/status') {
            // Serve status page with simple UI and system info
            const statusPage = await this.generateStatusPage();
            res.writeHead(200, { 'Content-Type': 'text/html' });
            res.end(statusPage);
        }
        else if (req.method === 'GET' && url.pathname.startsWith('/api/')) {
            // Handle API endpoints
            await this.handleApiRequest(url.pathname, req, res);
        }
        else {
            res.writeHead(404, { 'Content-Type': 'text/plain' });
            res.end('Not Found');
        }
    }
    async serveStaticFile(pathname, res) {
        const { readFile } = await import('fs/promises');
        const { join } = await import('path');
        try {
            // Remove leading slash and construct file path
            const filePath = join(process.cwd(), pathname.substring(1));
            // Determine content type
            const ext = pathname.split('.').pop();
            const contentType = this.getContentType(ext);
            const content = await readFile(filePath);
            res.writeHead(200, { 'Content-Type': contentType });
            res.end(content);
        }
        catch (error) {
            this.log(`Failed to serve static file ${pathname}: ${error.message}`, 'error');
            res.writeHead(404, { 'Content-Type': 'text/plain' });
            res.end('Not Found');
        }
    }
    getContentType(ext) {
        const types = {
            'js': 'application/javascript',
            'ts': 'application/javascript', // Serve TypeScript as JavaScript for ES modules
            'css': 'text/css',
            'html': 'text/html',
            'json': 'application/json',
            'png': 'image/png',
            'jpg': 'image/jpeg',
            'jpeg': 'image/jpeg',
            'gif': 'image/gif',
            'svg': 'image/svg+xml'
        };
        return types[ext || ''] || 'text/plain';
    }
    async handleApiRequest(pathname, req, res) {
        this.log(`🔌 API request: ${pathname}`);
        try {
            let responseData;
            switch (pathname) {
                case '/api/agents':
                    responseData = await this.getAgentsData();
                    break;
                case '/api/personas':
                    responseData = await this.getPersonasData();
                    break;
                case '/api/system':
                    responseData = this.getSystemStatus();
                    break;
                case '/api/daemons':
                    responseData = {
                        daemons: Array.from(this.registeredDaemons.keys()),
                        router: this.messageRouter.getSystemStatus()
                    };
                    break;
                case '/api/version':
                    responseData = await this.getCurrentVersion();
                    break;
                case '/api/register':
                    responseData = await this.handleClientRegistration(req);
                    break;
                case '/api/commands':
                    responseData = await this.getAvailableCommands();
                    break;
                case '/api/message':
                    responseData = await this.handleApiMessage(req);
                    break;
                case '/api/disconnect':
                    responseData = await this.handleClientDisconnect(req);
                    break;
                case '/api/browser/state':
                    responseData = this.browserManager.getBrowserState();
                    break;
                case '/api/browser/ensure':
                    responseData = await this.handleEnsureBrowser(req);
                    break;
                case '/api/browser/devtools':
                    responseData = await this.handleLaunchDevTools(req);
                    break;
                default:
                    res.writeHead(404, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify({ error: 'API endpoint not found' }));
                    return;
            }
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify(responseData));
        }
        catch (error) {
            this.log(`❌ API error for ${pathname}: ${error.message}`, 'error');
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Internal server error' }));
        }
    }
    async getAgentsData() {
        // Return array directly as UserSelector widget expects iterable agents
        return [
            {
                id: 'claude',
                name: 'Claude',
                role: 'AI Assistant',
                avatar: '🧠',
                status: 'online',
                type: 'ai'
            },
            {
                id: 'developer',
                name: 'Developer',
                role: 'Human Developer',
                avatar: '👨‍💻',
                status: 'online',
                type: 'human'
            }
        ];
    }
    async getPersonasData() {
        // Return array directly as SavedPersonas widget expects iterable apiPersonas
        return [
            {
                id: 'default',
                name: 'Default Assistant',
                description: 'General purpose AI assistant',
                avatar: '🤖',
                category: 'general',
                active: true
            },
            {
                id: 'developer',
                name: 'Code Expert',
                description: 'Specialized in software development',
                avatar: '👨‍💻',
                category: 'development',
                active: false
            },
            {
                id: 'researcher',
                name: 'Research Assistant',
                description: 'Focused on research and analysis',
                avatar: '🔬',
                category: 'research',
                active: false
            }
        ];
    }
    /**
     * Register a daemon instance for direct communication
     */
    registerDaemon(daemonName, daemon) {
        this.registeredDaemons.set(daemonName, daemon);
        this.log(`🔌 Registered daemon: ${daemonName}`);
    }
    async requestFromDaemon(daemonName, message) {
        const daemon = this.registeredDaemons.get(daemonName);
        if (daemon && daemon.handleMessage) {
            return await daemon.handleMessage(message);
        }
        throw new Error(`Daemon ${daemonName} not found or no handleMessage method`);
    }
    async generateStatusPage() {
        const { readFile } = await import('fs/promises');
        const { join } = await import('path');
        try {
            const templatePath = join(process.cwd(), 'src/ui/templates/status.html');
            let html = await readFile(templatePath, 'utf8');
            const systemStatus = {
                status: 'operational',
                server: this.name,
                version: this.version,
                daemons: Array.from(this.registeredDaemons.keys()),
                connections: this.connectionManager.getStats(),
                uptime: process.uptime(),
                timestamp: new Date().toISOString()
            };
            // Replace template variables
            html = html.replace('{{PORT}}', this.config.port.toString());
            html = html.replace('{{VERSION}}', this.version);
            html = html.replace('{{SYSTEM_STATUS}}', JSON.stringify(systemStatus, null, 2));
            return html;
        }
        catch (error) {
            this.log(`Failed to load status template: ${error.message}`, 'error');
            return '<h1>Status page error</h1><p>Could not load status template</p>';
        }
    }
    generateSimpleUI() {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum - Browser-Centric AI Collaboration</title>
    <style>
        body { 
            font-family: -apple-system, BlinkMacSystemFont, sans-serif; 
            margin: 0; padding: 20px; 
            background: #0a0a0a; color: #ffffff; 
        }
        .container { max-width: 800px; margin: 0 auto; }
        .status { 
            background: #1a1a1a; padding: 20px; border-radius: 8px; 
            border-left: 4px solid #00ff88; margin-bottom: 20px;
        }
        .emoji { font-size: 1.2em; }
        .ws-status { color: #00ff88; }
        pre { background: #1a1a1a; padding: 15px; border-radius: 8px; overflow-x: auto; }
    </style>
</head>
<body>
    <div class="container">
        <h1><span class="emoji">🌐</span> Continuum Service</h1>
        <div class="status">
            <h2><span class="emoji">✅</span> System Ready</h2>
            <p><strong>Service:</strong> Like Docker Desktop for AI collaboration</p>
            <p><strong>WebSocket:</strong> <span class="ws-status">Connected</span> on ws://localhost:${this.config.port}</p>
            <p><strong>Version:</strong> ${this.version}</p>
        </div>
        
        <h3><span class="emoji">🔌</span> WebSocket Test</h3>
        <div id="connection-status">Connecting...</div>
        <div id="messages"></div>
        
        <script>
            const ws = new WebSocket('ws://localhost:${this.config.port}');
            const status = document.getElementById('connection-status');
            const messages = document.getElementById('messages');
            
            ws.onopen = () => {
                status.innerHTML = '<span style="color: #00ff88">✅ WebSocket Connected</span>';
                ws.send(JSON.stringify({ type: 'ping', data: {}, timestamp: new Date().toISOString() }));
            };
            
            ws.onmessage = (event) => {
                const msg = JSON.parse(event.data);
                const div = document.createElement('div');
                div.innerHTML = '<pre>' + JSON.stringify(msg, null, 2) + '</pre>';
                messages.appendChild(div);
            };
            
            ws.onerror = () => {
                status.innerHTML = '<span style="color: #ff4444">❌ WebSocket Error</span>';
            };
            
            ws.onclose = () => {
                status.innerHTML = '<span style="color: #ffaa00">⚠️ WebSocket Disconnected</span>';
            };
        </script>
    </div>
</body>
</html>`;
    }
    // ============================================================================
    // BROWSER MANAGEMENT API HANDLERS
    // ============================================================================
    /**
     * Handle browser management requests
     */
    async handleEnsureBrowser(req) {
        try {
            const body = await this.getRequestBody(req);
            const options = body ? JSON.parse(body) : { mode: 'default' };
            this.log(`🌐 Ensuring browser connection (mode: ${options.mode})`);
            const browserState = await this.browserManager.ensureBrowserConnection(options);
            return {
                success: true,
                browserState,
                action: 'browser_ensured',
                connections: browserState.connectedClients.length
            };
        }
        catch (error) {
            this.log(`❌ Browser ensure failed: ${error.message}`, 'error');
            return {
                success: false,
                error: error.message
            };
        }
    }
    async handleLaunchDevTools(req) {
        try {
            const body = await this.getRequestBody(req);
            const options = body ? JSON.parse(body) : {};
            this.log(`🛠️ Launching DevTools browser`);
            const browserState = await this.browserManager.ensureBrowserConnection({
                mode: 'devtools',
                debugPort: options.debugPort || 9222,
                url: options.url || `http://localhost:${this.config.port}`
            });
            return {
                success: true,
                browserState,
                action: 'devtools_launched',
                debugPort: browserState.devToolsPort,
                devToolsApi: `http://localhost:${browserState.devToolsPort}/json`
            };
        }
        catch (error) {
            this.log(`❌ DevTools launch failed: ${error.message}`, 'error');
            return {
                success: false,
                error: error.message
            };
        }
    }
    // ============================================================================
    // THIN CLIENT API HANDLERS
    // ============================================================================
    /**
     * Handle thin client registration
     */
    async handleClientRegistration(req) {
        try {
            const body = await this.getRequestBody(req);
            const registrationData = JSON.parse(body);
            this.log(`📡 Registering thin client: ${registrationData.data.clientId}`);
            // Store client info (could be expanded to persist in database)
            const clientInfo = {
                ...registrationData.data,
                registeredAt: new Date().toISOString(),
                lastSeen: new Date().toISOString()
            };
            // Register with browser manager
            this.browserManager.registerClient(clientInfo);
            return {
                success: true,
                clientId: registrationData.data.clientId,
                serverVersion: this.version,
                registeredAt: clientInfo.registeredAt,
                availableCommands: await this.getCommandCount()
            };
        }
        catch (error) {
            this.log(`❌ Client registration failed: ${error.message}`, 'error');
            return {
                success: false,
                error: error.message
            };
        }
    }
    /**
     * Get available commands from daemon system
     */
    async getAvailableCommands() {
        try {
            // Get commands from registered daemons
            const commands = [];
            for (const [daemonName, daemonInfo] of this.registeredDaemons) {
                const daemonCommands = daemonInfo.messageTypes.map(type => ({
                    name: type,
                    daemon: daemonName,
                    type: 'daemon-command',
                    capabilities: daemonInfo.capabilities,
                    clientExecution: this.getClientExecutionInfo(type)
                }));
                commands.push(...daemonCommands);
            }
            return {
                success: true,
                commands,
                totalCommands: commands.length,
                daemons: Array.from(this.registeredDaemons.keys())
            };
        }
        catch (error) {
            this.log(`❌ Command discovery failed: ${error.message}`, 'error');
            return {
                success: false,
                error: error.message,
                commands: []
            };
        }
    }
    /**
     * Handle API messages from thin clients
     */
    async handleApiMessage(req) {
        try {
            const body = await this.getRequestBody(req);
            const message = JSON.parse(body);
            this.log(`📨 API message: ${message.type}`);
            // Route through message router like WebSocket messages
            const response = await this.messageRouter.routeMessage(message, 'api-client', this.daemonConnector);
            return response || { success: true, processed: true };
        }
        catch (error) {
            this.log(`❌ API message failed: ${error.message}`, 'error');
            return {
                success: false,
                error: error.message
            };
        }
    }
    /**
     * Handle client disconnect
     */
    async handleClientDisconnect(req) {
        try {
            const body = await this.getRequestBody(req);
            const disconnectData = JSON.parse(body);
            this.log(`📱 Client disconnected: ${disconnectData.data.clientId}`);
            return {
                success: true,
                message: 'Client disconnect recorded'
            };
        }
        catch (error) {
            this.log(`❌ Client disconnect handling failed: ${error.message}`, 'error');
            return {
                success: false,
                error: error.message
            };
        }
    }
    /**
     * Utility: Get request body
     */
    async getRequestBody(req) {
        return new Promise((resolve, reject) => {
            let body = '';
            req.on('data', (chunk) => {
                body += chunk.toString();
            });
            req.on('end', () => {
                resolve(body);
            });
            req.on('error', reject);
        });
    }
    /**
     * Utility: Get client execution info for a command
     */
    getClientExecutionInfo(commandType) {
        // Define which commands need client-side execution
        const clientSideCommands = [
            'execute_js', 'reload', 'navigate', 'inject_css',
            'screenshot', 'ping', 'update_ui'
        ];
        if (clientSideCommands.includes(commandType)) {
            return {
                required: true,
                reason: 'Browser context required'
            };
        }
        return {
            required: false,
            reason: 'Server-side execution'
        };
    }
    /**
     * Utility: Get command count
     */
    async getCommandCount() {
        let count = 0;
        for (const [, daemonInfo] of this.registeredDaemons) {
            count += daemonInfo.messageTypes.length;
        }
        return count;
    }
    async handleClientMessage(clientId, data) {
        try {
            const message = JSON.parse(data.toString());
            // Handle client console logs directly (JTAG methodology)
            if (message.type === 'client_console_log') {
                this.log(`📱 CLIENT ${clientId} [${message.level.toUpperCase()}]: ${message.message}`, message.level === 'error' ? 'error' : 'info');
                return; // Don't route these, just log them
            }
            this.log(`📨 Message from ${clientId}: ${message.type}`);
            const response = await this.messageRouter.routeMessage(message, clientId, this.daemonConnector);
            if (response) {
                this.sendToClient(clientId, response);
            }
        }
        catch (error) {
            this.log(`Error handling message from ${clientId}: ${error.message}`, 'error');
            // Get current version for error response
            const versionInfo = await this.getCurrentVersion();
            this.sendToClient(clientId, {
                type: 'error',
                data: {
                    error: 'Invalid message format',
                    serverVersion: versionInfo.version,
                    serverBuild: versionInfo.build,
                    daemon: this.name
                },
                timestamp: new Date().toISOString()
            });
        }
    }
    async handleSendMessage(data) {
        const { clientId, message } = data;
        const success = this.sendToClient(clientId, message);
        return {
            success,
            data: { sent: success }
        };
    }
    async handleBroadcastMessage(data) {
        const { message, excludeClientId } = data;
        const sentCount = this.connectionManager.broadcast(message, excludeClientId);
        return {
            success: true,
            data: { sentCount }
        };
    }
    async handleTabRegister(data) {
        const tabId = data?.tabId || `tab_${Date.now()}`;
        this.log(`📱 Tab registered: ${tabId}`);
        return {
            success: true,
            data: {
                registered: true,
                tabId: tabId,
                serverVersion: this.version,
                timestamp: new Date().toISOString()
            }
        };
    }
    async handleGetComponentCSS(data) {
        // Log the full data object to debug
        this.log(`🔍 CSS request data: ${JSON.stringify(data)}`);
        const component = data?.component || 'unknown';
        const cssPath = data?.path;
        this.log(`🎨 Component CSS requested: ${component} (path: ${cssPath})`);
        try {
            // Try to read the actual CSS file first
            let componentCSS;
            if (cssPath) {
                componentCSS = await this.readComponentCSSFile(cssPath);
            }
            else {
                // Fallback to hardcoded CSS
                componentCSS = this.getComponentCSS(component);
            }
            return {
                success: true,
                data: {
                    component: component,
                    css: componentCSS,
                    timestamp: new Date().toISOString()
                }
            };
        }
        catch (error) {
            this.log(`❌ Failed to load CSS for ${component}: ${error.message}`, 'error');
            // Return fallback CSS
            return {
                success: true,
                data: {
                    component: component,
                    css: this.getComponentCSS(component),
                    timestamp: new Date().toISOString()
                }
            };
        }
    }
    async readComponentCSSFile(cssPath) {
        const { readFile } = await import('fs/promises');
        const { join } = await import('path');
        // Remove leading slash and construct full path
        const fullPath = join(process.cwd(), 'src', cssPath.replace(/^\//, ''));
        this.log(`📁 Reading CSS file: ${fullPath}`);
        const cssContent = await readFile(fullPath, 'utf8');
        this.log(`✅ Loaded CSS file (${cssContent.length} bytes)`);
        return cssContent;
    }
    getComponentCSS(componentName) {
        // Basic fallback CSS for components
        const cssMap = {
            'SavedPersonas': `
        .saved-personas {
          background: #1a1a1a;
          border-radius: 8px;
          padding: 16px;
        }
        .persona-item {
          padding: 8px;
          margin: 4px 0;
          border-radius: 4px;
          background: #2a2a2a;
        }
      `,
            'UserSelector': `
        .user-selector {
          background: #1a1a1a;
          border-radius: 8px;
          padding: 16px;
        }
        .agent-item {
          padding: 8px;
          margin: 4px 0;
          border-radius: 4px;
          background: #2a2a2a;
        }
      `,
            'ActiveProjects': `
        .active-projects {
          background: #1a1a1a;
          border-radius: 8px;
          padding: 16px;
        }
        .project-item {
          padding: 8px;
          margin: 4px 0;
          border-radius: 4px;
          background: #2a2a2a;
        }
      `
        };
        return cssMap[componentName] || `/* No CSS found for ${componentName} */`;
    }
    sendToClient(clientId, message) {
        return this.connectionManager.sendToClient(clientId, message);
    }
    setupEventHandlers() {
        this.connectionManager.on('client:connected', (client) => {
            this.log(`Client connected: ${client.id}`);
            this.emit('client:connected', client);
        });
        this.connectionManager.on('client:disconnected', (client) => {
            this.log(`Client disconnected: ${client.id}`);
            this.emit('client:disconnected', client);
        });
        this.connectionManager.on('heartbeat:cleanup', (data) => {
            this.log(`Heartbeat cleanup: ${data.removedCount} stale clients removed`);
        });
        this.daemonConnector.on('connected', () => {
            this.log('✅ Connected to TypeScript command daemon');
        });
        this.daemonConnector.on('disconnected', () => {
            this.log('❌ Disconnected from TypeScript command daemon');
        });
        this.daemonConnector.on('error', (error) => {
            this.log(`Daemon connector error: ${error.message}`, 'error');
        });
    }
    /**
     * Register an external daemon with the dynamic router
     */
    async registerExternalDaemon(name, daemon) {
        await this.messageRouter.registerDaemon(name, daemon);
        this.registeredDaemons.set(name, daemon);
        this.log(`✅ Registered external daemon: ${name}`);
    }
    /**
     * Get comprehensive system status including all registered daemons
     */
    getSystemStatus() {
        const browserState = this.browserManager.getBrowserState();
        return {
            server: {
                name: this.name,
                version: this.version,
                status: this.getStatus(),
                uptime: this.getUptime(),
                port: this.config.port,
                host: this.config.host
            },
            connections: this.connectionManager.getStats(),
            browserConnections: {
                hasActiveConnections: browserState.hasActiveConnections,
                connectedClients: browserState.connectedClients.length,
                debugMode: browserState.debugMode,
                devToolsPort: browserState.devToolsPort,
                clients: browserState.connectedClients.map(client => ({
                    type: client.type,
                    lastSeen: client.lastSeen,
                    capabilities: client.capabilities.length
                }))
            },
            daemonConnector: {
                connected: this.daemonConnector.isConnected(),
                commandsAvailable: this.daemonConnector.getAvailableCommands().length
            },
            dynamicRouter: this.messageRouter.getSystemStatus(),
            registeredDaemons: Array.from(this.registeredDaemons.keys())
        };
    }
    getStats() {
        return this.getSystemStatus();
    }
    async getCurrentVersion() {
        try {
            const { readFileSync } = await import('fs');
            const packageData = JSON.parse(readFileSync('./package.json', 'utf8'));
            return {
                version: packageData.version,
                build: 'TypeScript Daemon System',
                timestamp: new Date().toISOString()
            };
        }
        catch (error) {
            this.log(`Failed to read version: ${error.message}`, 'error');
            return {
                version: '0.2.UNKNOWN',
                build: 'TypeScript Daemon System',
                timestamp: new Date().toISOString()
            };
        }
    }
}
// Main execution when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
    const daemon = new WebSocketDaemon();
    process.on('SIGINT', async () => {
        console.log('\n🛑 Received shutdown signal...');
        await daemon.stop();
        process.exit(0);
    });
    process.on('SIGTERM', async () => {
        console.log('\n🛑 Received termination signal...');
        await daemon.stop();
        process.exit(0);
    });
    daemon.start().catch(error => {
        console.error('❌ WebSocket daemon failed:', error);
        process.exit(1);
    });
}
