/**
 * Modular UI Generator
 * Generates web interface using web components instead of HTML strings
 */

class UIGeneratorModular {
  constructor(continuum) {
    this.continuum = continuum;
    
    // Initialize Academy interface
    const AcademyWebInterface = require('./AcademyWebInterface.cjs');
    this.academyInterface = new AcademyWebInterface(continuum);
  }

  generateHTML() {
    // Clear require cache to get fresh version info
    delete require.cache[require.resolve('../../package.json')];
    const packageInfo = require('../../package.json');
    
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum Academy - AI Workforce Construction</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🎓</text></svg>">
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif;
            background: linear-gradient(135deg, #0f1419 0%, #1a1f2e 100%);
            color: #e0e6ed;
            height: 100vh;
            overflow: hidden;
        }
        
        .app-container {
            display: flex;
            height: 100vh;
        }
        
        /* Sidebar */
        .sidebar {
            width: 300px;
            background: rgba(20, 25, 35, 0.95);
            backdrop-filter: blur(10px);
            border-right: 1px solid rgba(255, 255, 255, 0.1);
            display: flex;
            flex-direction: column;
        }
        
        .sidebar-header {
            padding: 20px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .logo {
            display: flex;
            align-items: center;
            margin-bottom: 10px;
        }
        
        .logo-icon {
            width: 32px;
            height: 32px;
            background: linear-gradient(135deg, #4FC3F7, #29B6F6);
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 12px;
            font-size: 18px;
        }
        
        .logo-text {
            font-size: 24px;
            font-weight: 700;
            color: #4FC3F7;
        }
        
        .subtitle {
            font-size: 14px;
            color: #8a92a5;
            margin-bottom: 15px;
        }
        
        /* Main Chat Area */
        .main-content {
            flex: 1;
            display: flex;
            flex-direction: column;
        }

        /* Component containers */
        .sidebar-content {
            flex: 1;
            display: flex;
            flex-direction: column;
            overflow: hidden;
        }

        /* Version info */
        .version-info {
            padding: 15px 20px;
            border-top: 1px solid rgba(255, 255, 255, 0.1);
            font-size: 11px;
            color: #666;
            background: rgba(0, 0, 0, 0.3);
        }
    </style>
</head>
<body>
    <div class="app-container">
        <!-- Sidebar -->
        <div class="sidebar">
            <div class="sidebar-header">
                <div class="logo">
                    <div class="logo-icon">🎓</div>
                    <div class="logo-text">Continuum</div>
                </div>
                <div class="subtitle">AI Workforce Construction</div>
                <status-pill status="connected" text="Connected"></status-pill>
            </div>
            
            <div class="sidebar-content">
                <!-- Room Tabs Component -->
                <room-tabs></room-tabs>
                
                <!-- Agent Selector Component -->
                <agent-selector></agent-selector>
                
                <!-- Academy Section Component (conditionally rendered) -->
                <div id="academy-container" style="display: none;">
                    <academy-section></academy-section>
                </div>
            </div>
            
            <div class="version-info">
                v${packageInfo.version} • Node ${process.version}
            </div>
        </div>
        
        <!-- Main Content -->
        <div class="main-content">
            <!-- Chat Header Component -->
            <chat-header title="Agent Coordination" subtitle="Multi-agent AI workspace"></chat-header>
            
            <!-- Chat Area Component -->
            <chat-area></chat-area>
        </div>
    </div>

    <!-- Load Component Scripts -->
    <script src="/src/ui/components/AgentSelector.js"></script>
    <script src="/src/ui/components/ChatHeader.js"></script>
    <script src="/src/ui/components/ChatArea.js"></script>
    <script src="/src/ui/components/RoomTabs.js"></script>
    <script src="/src/ui/components/StatusPill.js"></script>
    <script src="/src/ui/components/AcademySection.js"></script>
    
    <!-- Continuum API -->
    <script src="/src/ui/continuum-api.js"></script>
    
    <!-- Application Logic -->
    <script>
        // WebSocket connection
        let ws = null;
        let isConnected = false;
        let currentRoom = 'general';
        
        // Component references
        let agentSelector = null;
        let chatArea = null;
        let chatHeader = null;
        let roomTabs = null;
        let statusPill = null;
        let academySection = null;
        
        // Update continuum version after script loads
        if (window.continuum) {
            window.continuum.version = '${packageInfo.version}';
        }
        
        // Initialize application
        document.addEventListener('DOMContentLoaded', function() {
            initializeComponents();
            connectWebSocket();
        });
        
        function initializeComponents() {
            // Get component references
            agentSelector = document.querySelector('agent-selector');
            chatArea = document.querySelector('chat-area');
            chatHeader = document.querySelector('chat-header');
            roomTabs = document.querySelector('room-tabs');
            statusPill = document.querySelector('status-pill');
            academySection = document.querySelector('academy-section');
            
            // Setup event listeners
            setupEventListeners();
            
            console.log('✅ Components initialized');
        }
        
        function setupEventListeners() {
            // Agent selector events
            if (agentSelector) {
                agentSelector.addEventListener('agent-selected', (e) => {
                    console.log('Agent selected:', e.detail.agentId);
                });
                
                agentSelector.addEventListener('agent-info-requested', (e) => {
                    console.log('Agent info requested:', e.detail.agentId);
                });
            }
            
            // Chat area events
            if (chatArea) {
                chatArea.addEventListener('send-message', (e) => {
                    sendMessage(e.detail.message);
                });
            }
            
            // Chat header events
            if (chatHeader) {
                chatHeader.addEventListener('clear-chat', () => {
                    clearChat();
                });
            }
            
            // Room tabs events
            if (roomTabs) {
                roomTabs.addEventListener('room-changed', (e) => {
                    switchRoom(e.detail.roomId);
                });
            }
            
            // Academy events
            if (academySection) {
                academySection.addEventListener('send-sheriff', () => {
                    sendSheriffToAcademy();
                });
                
                academySection.addEventListener('train-custom', () => {
                    trainCustomPersona();
                });
            }
        }
        
        function connectWebSocket() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = protocol + '//' + window.location.host;
            
            statusPill?.setStatus('connecting', 'Connecting...');
            
            ws = new WebSocket(wsUrl);
            
            // Make WebSocket globally accessible for validation scripts
            window.ws = ws;
            
            ws.onopen = function() {
                isConnected = true;
                statusPill?.setStatus('connected', 'Connected');
                chatArea?.setConnected(true);
                console.log('✅ WebSocket connected');
                
                // Don't run validation immediately - wait for server handshake completion
            };
            
            ws.onmessage = function(event) {
                try {
                    const data = JSON.parse(event.data);
                    handleWebSocketMessage(data);
                } catch (error) {
                    console.error('Error parsing WebSocket message:', error);
                }
            };
            
            ws.onclose = function() {
                isConnected = false;
                statusPill?.setStatus('disconnected', 'Disconnected');
                chatArea?.setConnected(false);
                console.log('❌ WebSocket disconnected');
                
                // Reconnect after delay
                setTimeout(connectWebSocket, 3000);
            };
            
            ws.onerror = function(error) {
                console.error('WebSocket error:', error);
                statusPill?.setStatus('error', 'Connection Error');
            };
        }
        
        function handleWebSocketMessage(data) {
            console.log('🔧 DEBUG: handleWebSocketMessage called with type:', data.type);
            switch (data.type) {
                case 'message':
                    addMessage(data);
                    break;
                    
                case 'agent_list':
                    agentSelector?.updateRemoteAgents(data.agents || []);
                    break;
                    
                case 'academy_update':
                    if (data.personaName && data.session) {
                        // Handle individual academy update
                        console.log('Academy update for:', data.personaName);
                    }
                    break;
                    
                case 'academy_status_push':
                    academySection?.updateAcademyStatus(data.status);
                    break;
                    
                case 'connection_banner':
                    // Connection is fully established - initialize continuum client
                    console.log('🎯 Connection banner received - system ready');
                    console.log('🔍 DEBUG: WebSocket state:', window.ws ? window.ws.readyState : 'no ws');
                    console.log('🔍 DEBUG: WebSocket.OPEN constant:', WebSocket.OPEN);
                    setTimeout(() => {
                        console.log('🔍 DEBUG: In setTimeout - checking WebSocket...');
                        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                            console.log('🚀 Dispatching continuum-ready event...');
                            document.dispatchEvent(new CustomEvent('continuum-ready'));
                            console.log('✅ continuum-ready event dispatched');
                        } else {
                            console.warn('❌ WebSocket not ready for continuum-ready dispatch');
                            console.log('   ws exists:', !!window.ws);
                            console.log('   ws.readyState:', window.ws ? window.ws.readyState : 'undefined');
                        }
                    }, 100); // Small delay to ensure all promises resolved
                    break;
                    
                default:
                    console.log('Unknown message type:', data.type);
            }
        }
        
        function sendMessage(message) {
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                console.error('WebSocket not connected');
                return;
            }
            
            // Add user message immediately
            addMessage({
                type: 'user',
                sender: 'You',
                content: message,
                timestamp: new Date().toLocaleTimeString()
            });
            
            // Send to server
            ws.send(JSON.stringify({
                type: 'message',
                content: message,
                room: currentRoom
            }));
        }
        
        function addMessage(message) {
            chatArea?.addMessage(message);
        }
        
        function clearChat() {
            if (confirm('Are you sure you want to clear the chat?')) {
                chatArea?.clearMessages();
            }
        }
        
        function switchRoom(roomId) {
            currentRoom = roomId;
            
            // Update UI based on room
            if (roomId === 'academy') {
                document.getElementById('academy-container').style.display = 'block';
                chatHeader?.setTitle('Academy Training');
                chatHeader?.setSubtitle('AI persona training and management');
                
                // Request academy status
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({ type: 'get_academy_status' }));
                }
            } else {
                document.getElementById('academy-container').style.display = 'none';
                chatHeader?.setTitle('Agent Coordination');
                chatHeader?.setSubtitle('Multi-agent AI workspace');
            }
            
            // Clear messages when switching rooms
            chatArea?.clearMessages();
        }
        
        function sendSheriffToAcademy() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: 'Sheriff-' + Date.now(),
                    specialization: 'protocol_enforcement',
                    rounds: 10
                }));
            }
        }
        
        function trainCustomPersona() {
            const personaName = prompt('Enter persona name:');
            const specialization = prompt('Enter specialization (e.g., protocol_enforcement, command_validation):') || 'protocol_enforcement';
            
            if (personaName && ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'start_academy_training',
                    personaName: personaName,
                    specialization: specialization,
                    rounds: 10
                }));
            }
        }
        
        // Global functions for backward compatibility
        window.sendMessage = sendMessage;
        window.clearChat = clearChat;
        window.sendSheriffToAcademy = sendSheriffToAcademy;
        window.trainCustomPersona = trainCustomPersona;
    </script>
</body>
</html>`;
  }

  generateWebSocketJS() {
    // This method is no longer needed as WebSocket logic is embedded in the HTML
    return '';
  }

  generateAcademyHTML() {
    // Academy HTML is now handled by the AcademySection component
    return this.academyInterface.generateAcademyHTML();
  }

  generateAcademyJS() {
    // Academy JS is now embedded in the main application logic
    return this.academyInterface.generateAcademyJS();
  }
}

module.exports = UIGeneratorModular;