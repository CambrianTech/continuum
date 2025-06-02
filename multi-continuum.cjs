#!/usr/bin/env node
/**
 * MULTI CONTINUUM
 * 
 * Handles multiple simultaneous responses and easy interfacing
 * Supports concurrent conversations and programmatic access
 * Multiple response modes: immediate, streaming, batch
 */
const http = require('http');
const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
function uuidv4() {
  return crypto.randomUUID();
}

class MultiContinuum {
  constructor() {
    this.claudePersonas = new Map();
    this.activeConversations = new Map();
    this.responseQueue = new Map();
    this.websocketConnections = new Set();
    this.httpServer = null;
    this.websocketServer = null;
    this.isRunning = false;
    this.responseMode = 'immediate'; // immediate, streaming, batch
  }

  async launchMultiContinuum() {
    console.log('🌌 MULTI CONTINUUM - Multiple Response Handler');
    console.log('=============================================');
    console.log('🔄 Supports: Concurrent conversations, streaming responses, batch processing');
    console.log('🌐 Interfaces: WebSocket, REST API, programmatic access');
    
    this.createPersonas();
    await this.createWebInterface();
    await this.createRestAPI();
    
    this.isRunning = true;
    console.log('🎉 Multi Continuum launched!');
    console.log('🌐 WebSocket: ws://localhost:5558/ws');
    console.log('🔗 REST API: http://localhost:5558/api');
    console.log('📱 Web UI: http://localhost:5558');
  }

  createPersonas() {
    this.claudePersonas.set('QuestionerClaude', {
      name: 'QuestionerClaude',
      role: 'Asks clarifying questions and explores requirements',
      capabilities: ['question_generation', 'requirement_analysis', 'clarification'],
      responseStyle: 'inquisitive',
      multiResponseSupport: true
    });

    this.claudePersonas.set('PlannerClaude', {
      name: 'PlannerClaude', 
      role: 'Creates detailed plans and strategic approaches',
      capabilities: ['planning', 'strategy', 'breakdown', 'prioritization'],
      responseStyle: 'structured',
      multiResponseSupport: true
    });

    this.claudePersonas.set('ImplementerClaude', {
      name: 'ImplementerClaude',
      role: 'Focuses on execution and practical solutions',
      capabilities: ['implementation', 'coding', 'execution', 'troubleshooting'],
      responseStyle: 'action-oriented',
      multiResponseSupport: true
    });

    this.claudePersonas.set('AnalyzerClaude', {
      name: 'AnalyzerClaude',
      role: 'Analyzes problems and provides multiple perspectives',
      capabilities: ['analysis', 'evaluation', 'comparison', 'insights'],
      responseStyle: 'analytical',
      multiResponseSupport: true
    });

    console.log(`✅ Created ${this.claudePersonas.size} personas with multi-response support`);
  }

  // MULTIPLE RESPONSE HANDLING
  async generateMultipleResponses(message, requestedPersonas = null, options = {}) {
    const conversationId = uuidv4();
    const personas = requestedPersonas || Array.from(this.claudePersonas.keys());
    const responses = new Map();
    
    console.log(`🔄 Generating multiple responses for conversation: ${conversationId}`);
    
    // Create conversation context
    const conversation = {
      id: conversationId,
      message: message,
      personas: personas,
      responses: responses,
      startTime: new Date(),
      status: 'processing',
      options: options
    };
    
    this.activeConversations.set(conversationId, conversation);
    
    // Generate responses from each persona
    const responsePromises = personas.map(async (personaName) => {
      try {
        const response = await this.generatePersonaResponse(personaName, message, conversationId);
        responses.set(personaName, {
          content: response,
          timestamp: new Date(),
          status: 'completed'
        });
        
        // Notify via WebSocket if streaming mode
        if (this.responseMode === 'streaming') {
          this.broadcastUpdate({
            type: 'persona_response',
            conversationId: conversationId,
            persona: personaName,
            content: response,
            partial: false
          });
        }
        
        return { persona: personaName, response };
      } catch (error) {
        responses.set(personaName, {
          error: error.message,
          timestamp: new Date(),
          status: 'error'
        });
        return { persona: personaName, error: error.message };
      }
    });
    
    // Wait for all responses
    const results = await Promise.all(responsePromises);
    
    conversation.status = 'completed';
    conversation.endTime = new Date();
    conversation.duration = conversation.endTime - conversation.startTime;
    
    console.log(`✅ Generated ${results.length} responses in ${conversation.duration}ms`);
    
    return {
      conversationId: conversationId,
      results: results,
      conversation: conversation
    };
  }

  // STREAMING RESPONSE SUPPORT
  async generateStreamingResponse(personaName, message, connectionId) {
    const response = await this.generatePersonaResponse(personaName, message);
    const chunks = this.chunkResponse(response);
    
    for (let i = 0; i < chunks.length; i++) {
      const chunk = chunks[i];
      const isLast = i === chunks.length - 1;
      
      this.broadcastToConnection(connectionId, {
        type: 'streaming_response',
        persona: personaName,
        chunk: chunk,
        isLast: isLast,
        chunkIndex: i,
        totalChunks: chunks.length
      });
      
      // Small delay for streaming effect
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }

  chunkResponse(response, chunkSize = 50) {
    const words = response.split(' ');
    const chunks = [];
    
    for (let i = 0; i < words.length; i += chunkSize) {
      chunks.push(words.slice(i, i + chunkSize).join(' '));
    }
    
    return chunks;
  }

  // EASY PROGRAMMING INTERFACE
  createProgrammaticAPI() {
    return {
      // Simple single response
      ask: async (message, persona = 'QuestionerClaude') => {
        return await this.generatePersonaResponse(persona, message);
      },
      
      // Multiple responses
      askAll: async (message, personas = null) => {
        return await this.generateMultipleResponses(message, personas);
      },
      
      // Batch processing
      batch: async (messages) => {
        const results = [];
        for (const msg of messages) {
          const result = await this.generateMultipleResponses(msg.message, msg.personas);
          results.push(result);
        }
        return results;
      },
      
      // Get conversation history
      getConversation: (conversationId) => {
        return this.activeConversations.get(conversationId);
      },
      
      // List available personas
      getPersonas: () => {
        return Array.from(this.claudePersonas.entries()).map(([name, config]) => ({
          name,
          role: config.role,
          capabilities: config.capabilities
        }));
      },
      
      // Set response mode
      setMode: (mode) => {
        this.responseMode = mode;
      }
    };
  }

  // REST API INTERFACE
  async createRestAPI() {
    // API endpoints will be handled in the main HTTP server
    console.log('🔗 REST API endpoints created:');
    console.log('  POST /api/ask - Single response');
    console.log('  POST /api/ask-all - Multiple responses'); 
    console.log('  POST /api/batch - Batch processing');
    console.log('  GET /api/personas - List personas');
    console.log('  GET /api/conversations/:id - Get conversation');
  }

  handleRestRequest(req, res) {
    const url = new URL(req.url, `http://${req.headers.host}`);
    const path = url.pathname;
    const method = req.method;

    // Handle CORS
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

    if (method === 'OPTIONS') {
      res.writeHead(200);
      res.end();
      return;
    }

    if (path === '/api/personas' && method === 'GET') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(this.createProgrammaticAPI().getPersonas()));
      return;
    }

    if (path === '/api/ask' && method === 'POST') {
      let body = '';
      req.on('data', chunk => body += chunk.toString());
      req.on('end', async () => {
        try {
          const { message, persona } = JSON.parse(body);
          const response = await this.generatePersonaResponse(persona || 'QuestionerClaude', message);
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ response, persona: persona || 'QuestionerClaude' }));
        } catch (error) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: error.message }));
        }
      });
      return;
    }

    if (path === '/api/ask-all' && method === 'POST') {
      let body = '';
      req.on('data', chunk => body += chunk.toString());
      req.on('end', async () => {
        try {
          const { message, personas } = JSON.parse(body);
          const result = await this.generateMultipleResponses(message, personas);
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify(result));
        } catch (error) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: error.message }));
        }
      });
      return;
    }

    // Handle conversations endpoint
    if (path.startsWith('/api/conversations/') && method === 'GET') {
      const conversationId = path.split('/api/conversations/')[1];
      const conversation = this.activeConversations.get(conversationId);
      if (conversation) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(conversation));
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Conversation not found' }));
      }
      return;
    }

    // Not found
    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: 'Endpoint not found' }));
  }

  async generatePersonaResponse(personaName, message, conversationId = null) {
    const persona = this.claudePersonas.get(personaName);
    if (!persona) {
      throw new Error(`Unknown persona: ${personaName}`);
    }

    // Generate contextual response based on persona style
    switch (persona.responseStyle) {
      case 'inquisitive':
        return this.generateInquisitiveResponse(message, conversationId);
      case 'structured':
        return this.generateStructuredResponse(message, conversationId);
      case 'action-oriented':
        return this.generateActionOrientedResponse(message, conversationId);
      case 'analytical':
        return this.generateAnalyticalResponse(message, conversationId);
      default:
        return `${personaName}: I understand your message "${message}". How can I help you with this?`;
    }
  }

  generateInquisitiveResponse(message, conversationId) {
    const questions = [
      "What specific outcomes are you hoping to achieve?",
      "Can you provide more context about your constraints?",
      "What approaches have you considered so far?",
      "How urgent is this requirement?",
      "What would success look like to you?",
      "Are there any dependencies I should know about?"
    ];
    
    const selectedQuestions = questions.slice(0, 2 + Math.floor(Math.random() * 2));
    return `I want to fully understand your request: "${message}"\n\n` + selectedQuestions.map((q, i) => `${i + 1}. ${q}`).join('\n');
  }

  generateStructuredResponse(message, conversationId) {
    return `📋 STRUCTURED ANALYSIS: "${message}"

🎯 OBJECTIVE BREAKDOWN:
- Primary goal: ${this.extractPrimaryGoal(message)}
- Scope: ${this.categorizeScope(message)}
- Complexity: ${this.assessComplexity(message)}

📝 IMPLEMENTATION PLAN:
1. Requirements gathering and validation
2. Resource identification and allocation
3. Step-by-step execution strategy
4. Quality assurance and testing
5. Delivery and optimization

🔄 NEXT ACTIONS:
- Clarify any ambiguous requirements
- Identify potential risks and mitigation strategies
- Establish timeline and milestones

Would you like me to elaborate on any section?`;
  }

  generateActionOrientedResponse(message, conversationId) {
    return `⚡ IMPLEMENTATION FOCUS: "${message}"

🔧 IMMEDIATE ACTIONS:
1. Analyze technical requirements
2. Select optimal tools and approaches
3. Create working prototypes
4. Test and iterate rapidly

💡 PRACTICAL APPROACH:
- Start with minimum viable solution
- Build incrementally with feedback loops
- Focus on measurable outcomes
- Maintain code quality and documentation

🚀 EXECUTION STRATEGY:
Ready to begin implementation. What specific aspect would you like me to tackle first?`;
  }

  generateAnalyticalResponse(message, conversationId) {
    return `🔍 ANALYTICAL PERSPECTIVE: "${message}"

📊 PROBLEM ANALYSIS:
- Core issue: ${this.identifyCore(message)}
- Contributing factors: Multiple variables detected
- Impact assessment: ${this.assessImpact(message)}

🎯 SOLUTION VECTORS:
1. Direct approach: Address root cause immediately
2. Iterative approach: Gradual improvement strategy  
3. Systematic approach: Comprehensive restructuring

⚖️ TRADE-OFF ANALYSIS:
- Speed vs. Thoroughness
- Cost vs. Quality
- Risk vs. Reward

📈 RECOMMENDED PATH:
Based on analysis, I suggest a hybrid approach combining elements from multiple solution vectors.`;
  }

  // Helper methods for analysis
  extractPrimaryGoal(message) {
    if (message.includes('build') || message.includes('create')) return 'Creation/Development';
    if (message.includes('fix') || message.includes('solve')) return 'Problem Resolution';
    if (message.includes('improve') || message.includes('optimize')) return 'Enhancement';
    return 'General Assistance';
  }

  categorizeScope(message) {
    const wordCount = message.split(' ').length;
    if (wordCount < 10) return 'Small';
    if (wordCount < 25) return 'Medium';
    return 'Large';
  }

  assessComplexity(message) {
    const complexityIndicators = ['multiple', 'integrate', 'system', 'architecture', 'complex'];
    const matches = complexityIndicators.filter(word => message.toLowerCase().includes(word));
    if (matches.length >= 2) return 'High';
    if (matches.length >= 1) return 'Medium';
    return 'Low';
  }

  identifyCore(message) {
    if (message.includes('?')) return 'Information request';
    if (message.includes('help')) return 'Assistance needed';
    if (message.includes('problem')) return 'Problem solving';
    return 'Task execution';
  }

  assessImpact(message) {
    if (message.includes('urgent') || message.includes('critical')) return 'High';
    if (message.includes('important') || message.includes('priority')) return 'Medium';
    return 'Standard';
  }

  // WEBSOCKET INTERFACE
  async createWebInterface() {
    this.httpServer = http.createServer((req, res) => {
      if (req.url.startsWith('/api/')) {
        this.handleRestRequest(req, res);
      } else if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateMultiUI());
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    this.websocketServer = new WebSocket.Server({ 
      server: this.httpServer,
      path: '/ws'
    });

    this.websocketServer.on('connection', (ws) => {
      const connectionId = uuidv4();
      this.websocketConnections.add(ws);
      
      console.log(`👤 User connected: ${connectionId}`);
      
      ws.send(JSON.stringify({
        type: 'connection_established',
        connectionId: connectionId,
        availablePersonas: Array.from(this.claudePersonas.keys()),
        responseModes: ['immediate', 'streaming', 'batch'],
        currentMode: this.responseMode
      }));

      ws.on('message', async (data) => {
        try {
          const message = JSON.parse(data);
          await this.handleWebSocketMessage(ws, message, connectionId);
        } catch (error) {
          console.error('❌ WebSocket error:', error);
          ws.send(JSON.stringify({
            type: 'error',
            message: error.message,
            timestamp: new Date().toISOString()
          }));
        }
      });

      ws.on('close', () => {
        this.websocketConnections.delete(ws);
        console.log(`👤 User disconnected: ${connectionId}`);
      });
    });

    return new Promise((resolve) => {
      this.httpServer.listen(5558, () => {
        console.log('🌐 Multi Continuum interface ready at http://localhost:5558');
        resolve();
      });
    });
  }

  async handleWebSocketMessage(ws, message, connectionId) {
    console.log(`📨 WebSocket message: ${message.type}`);
    
    switch (message.type) {
      case 'single_request':
        const singleResponse = await this.generatePersonaResponse(
          message.persona || 'QuestionerClaude', 
          message.content
        );
        ws.send(JSON.stringify({
          type: 'single_response',
          persona: message.persona || 'QuestionerClaude',
          content: singleResponse,
          timestamp: new Date().toISOString()
        }));
        break;

      case 'multi_request':
        const multiResult = await this.generateMultipleResponses(
          message.content, 
          message.personas
        );
        ws.send(JSON.stringify({
          type: 'multi_response',
          ...multiResult,
          timestamp: new Date().toISOString()
        }));
        break;

      case 'streaming_request':
        await this.generateStreamingResponse(
          message.persona || 'QuestionerClaude',
          message.content,
          connectionId
        );
        break;

      case 'set_mode':
        this.responseMode = message.mode;
        ws.send(JSON.stringify({
          type: 'mode_changed',
          newMode: this.responseMode,
          timestamp: new Date().toISOString()
        }));
        break;

      default:
        ws.send(JSON.stringify({
          type: 'error',
          message: `Unknown message type: ${message.type}`,
          timestamp: new Date().toISOString()
        }));
    }
  }

  broadcastUpdate(update) {
    const message = JSON.stringify(update);
    for (const ws of this.websocketConnections) {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(message);
      }
    }
  }

  broadcastToConnection(connectionId, message) {
    // In a real implementation, you'd track connections by ID
    this.broadcastUpdate(message);
  }

  generateMultiUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Multi Continuum - Multiple Response Interface</title>
    <style>
        body { 
            background: linear-gradient(135deg, #2d1b69 0%, #11998e 100%); 
            color: #fff; 
            font-family: 'Segoe UI', sans-serif; 
            padding: 20px; 
            margin: 0;
        }
        .header { 
            text-align: center; 
            background: rgba(255,255,255,0.1); 
            padding: 30px; 
            border-radius: 15px;
            margin-bottom: 30px;
        }
        .controls {
            display: flex;
            gap: 15px;
            margin: 20px 0;
            flex-wrap: wrap;
        }
        .response-modes {
            display: flex;
            gap: 10px;
            margin: 20px 0;
        }
        .mode-btn {
            padding: 10px 20px;
            background: rgba(255,255,255,0.2);
            border: none;
            color: white;
            border-radius: 25px;
            cursor: pointer;
        }
        .mode-btn.active {
            background: #11998e;
        }
        .personas-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 15px;
            margin: 20px 0;
        }
        .persona-card {
            background: rgba(255,255,255,0.1);
            padding: 15px;
            border-radius: 10px;
            border-left: 4px solid #11998e;
        }
        .responses {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
            gap: 20px;
            margin: 30px 0;
        }
        .response-box {
            background: rgba(255,255,255,0.1);
            padding: 20px;
            border-radius: 10px;
            min-height: 200px;
        }
        .input-area {
            background: rgba(255,255,255,0.1);
            padding: 25px;
            border-radius: 15px;
            margin: 30px 0;
        }
        input, textarea, button {
            padding: 12px;
            border: none;
            border-radius: 8px;
            margin: 5px;
        }
        textarea {
            width: calc(100% - 24px);
            height: 100px;
            resize: vertical;
        }
        button {
            background: #11998e;
            color: white;
            cursor: pointer;
            font-weight: bold;
        }
        button:hover {
            background: #0d7377;
        }
        .streaming-indicator {
            display: none;
            color: #11998e;
            font-style: italic;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🌌 Multi Continuum</h1>
        <p>Multiple Response Interface with Streaming Support</p>
        <div class="response-modes">
            <button class="mode-btn active" onclick="setMode('immediate')">Immediate</button>
            <button class="mode-btn" onclick="setMode('streaming')">Streaming</button>
            <button class="mode-btn" onclick="setMode('batch')">Batch</button>
        </div>
    </div>

    <div class="personas-grid" id="personasGrid">
        <!-- Personas will be populated by JavaScript -->
    </div>

    <div class="input-area">
        <textarea id="messageInput" placeholder="Enter your message here..."></textarea>
        <div class="controls">
            <button onclick="sendToSingle()">Send to Selected Persona</button>
            <button onclick="sendToAll()">Send to All Personas</button>
            <button onclick="sendStreaming()">Send Streaming</button>
        </div>
        <div class="streaming-indicator" id="streamingIndicator">⚡ Streaming responses...</div>
    </div>

    <div class="responses" id="responses">
        <!-- Responses will appear here -->
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5558/ws');
        let currentMode = 'immediate';
        let selectedPersona = 'QuestionerClaude';
        let availablePersonas = [];

        ws.onopen = () => {
            console.log('Connected to Multi Continuum');
        };

        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            handleMessage(data);
        };

        function handleMessage(data) {
            switch (data.type) {
                case 'connection_established':
                    availablePersonas = data.availablePersonas;
                    currentMode = data.currentMode;
                    renderPersonas();
                    updateModeButtons();
                    break;
                
                case 'single_response':
                    addResponse(data.persona, data.content);
                    break;
                
                case 'multi_response':
                    displayMultiResponse(data);
                    break;
                
                case 'streaming_response':
                    handleStreamingResponse(data);
                    break;
                
                case 'mode_changed':
                    currentMode = data.newMode;
                    updateModeButtons();
                    break;
            }
        }

        function renderPersonas() {
            const grid = document.getElementById('personasGrid');
            grid.innerHTML = availablePersonas.map(persona => 
                '<div class="persona-card" onclick="selectPersona(\'' + persona + '\')">' +
                    '<h3>' + persona + '</h3>' +
                    '<p>Click to select for single responses</p>' +
                '</div>'
            ).join('');
        }

        function selectPersona(persona) {
            selectedPersona = persona;
            document.querySelectorAll('.persona-card').forEach(card => {
                card.style.border = '1px solid transparent';
            });
            event.target.closest('.persona-card').style.border = '2px solid #11998e';
        }

        function setMode(mode) {
            currentMode = mode;
            ws.send(JSON.stringify({
                type: 'set_mode',
                mode: mode
            }));
        }

        function updateModeButtons() {
            document.querySelectorAll('.mode-btn').forEach(btn => {
                btn.classList.remove('active');
            });
            document.querySelector('[onclick="setMode(\'' + currentMode + '\')"]').classList.add('active');
        }

        function sendToSingle() {
            const message = document.getElementById('messageInput').value.trim();
            if (!message) return;

            ws.send(JSON.stringify({
                type: 'single_request',
                content: message,
                persona: selectedPersona
            }));

            addMessage('You', message);
        }

        function sendToAll() {
            const message = document.getElementById('messageInput').value.trim();
            if (!message) return;

            ws.send(JSON.stringify({
                type: 'multi_request',
                content: message,
                personas: availablePersonas
            }));

            addMessage('You', message);
        }

        function sendStreaming() {
            const message = document.getElementById('messageInput').value.trim();
            if (!message) return;

            document.getElementById('streamingIndicator').style.display = 'block';

            ws.send(JSON.stringify({
                type: 'streaming_request',
                content: message,
                persona: selectedPersona
            }));

            addMessage('You', message);
        }

        function addMessage(sender, content) {
            const responses = document.getElementById('responses');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'response-box';
            messageDiv.innerHTML = '<h4>' + sender + '</h4><p>' + content + '</p>';
            responses.appendChild(messageDiv);
            responses.scrollTop = responses.scrollHeight;
        }

        function addResponse(persona, content) {
            const responses = document.getElementById('responses');
            const responseDiv = document.createElement('div');
            responseDiv.className = 'response-box';
            responseDiv.innerHTML = '<h4>🎭 ' + persona + '</h4><p>' + content + '</p>';
            responses.appendChild(responseDiv);
            responses.scrollTop = responses.scrollHeight;
        }

        function displayMultiResponse(data) {
            data.results.forEach(result => {
                if (result.response) {
                    addResponse(result.persona, result.response);
                } else if (result.error) {
                    addResponse(result.persona, 'Error: ' + result.error);
                }
            });
        }

        function handleStreamingResponse(data) {
            let responseBox = document.getElementById('streaming-' + data.persona);
            
            if (!responseBox) {
                responseBox = document.createElement('div');
                responseBox.id = 'streaming-' + data.persona;
                responseBox.className = 'response-box';
                responseBox.innerHTML = '<h4>🎭 ' + data.persona + ' (Streaming)</h4><p id="streaming-content-' + data.persona + '"></p>';
                document.getElementById('responses').appendChild(responseBox);
            }

            const contentArea = document.getElementById('streaming-content-' + data.persona);
            contentArea.textContent += data.chunk + ' ';

            if (data.isLast) {
                document.getElementById('streamingIndicator').style.display = 'none';
                responseBox.querySelector('h4').textContent = '🎭 ' + data.persona;
            }
        }

        // Auto-focus textarea
        document.getElementById('messageInput').focus();
    </script>
</body>
</html>`;
  }
}

// Launch if run directly
if (require.main === module) {
  const continuum = new MultiContinuum();
  continuum.launchMultiContinuum().catch(console.error);
}

module.exports = MultiContinuum;