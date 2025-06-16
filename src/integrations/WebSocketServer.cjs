/**
 * WebSocket Server
 * Handles WebSocket connections and message routing
 */

const WebSocket = require('ws');
const EventEmitter = require('events');
const MessageQueue = require('../core/MessageQueue.cjs');
const TabManager = require('../services/TabManager.cjs');
const RemoteAgentManager = require('../services/RemoteAgentManager.cjs');
const BrowserLogger = require('../core/BrowserLogger.cjs');

class WebSocketServer extends EventEmitter {
  constructor(continuum, httpServer) {
    super(); // Call EventEmitter constructor
    this.continuum = continuum;
    this.messageQueue = new MessageQueue();
    this.wss = new WebSocket.Server({ server: httpServer });
    this.tabManager = new TabManager();
    this.remoteAgentManager = new RemoteAgentManager(continuum);
    this.browserLogger = new BrowserLogger(continuum);
    this.setupWebSocket();
  }

  setupWebSocket() {
    this.wss.on('connection', (ws) => {
      // Create unique session for this connection
      const sessionId = `session_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      // Store session ID on WebSocket for later retrieval
      ws.sessionId = sessionId;
      
      // Initialize session data with message queue
      this.messageQueue.initializeSession(ws, sessionId);
      this.continuum.activeConnections.set(sessionId, ws);
      this.continuum.conversationThreads.set(sessionId, []);
      
      console.log(`👤 User connected (Session: ${sessionId})`);
      
      // Update Continuon status indicator
      if (this.continuum.systemTray) {
        this.continuum.systemTray.onUserConnected();
      }
      
      // Send status immediately (not queued)
      this.sendStatus(ws, sessionId);
      
      // Send connection banner with available commands (like terminal login)
      this.sendConnectionBanner(ws, sessionId);
      
      // Queue the greeting to be sent after any tasks
      this.messageQueue.queueGreeting(ws, this.continuum);
      
      ws.on('message', (message) => this.handleMessage(ws, message));
      ws.on('close', () => this.handleDisconnect(sessionId));
      ws.on('error', (error) => this.handleError(sessionId, error));
    });
  }

  getSessionId(ws) {
    return ws.sessionId;
  }

  sendConnectionBanner(ws, sessionId) {
    const packageInfo = require('../../package.json');
    
    // Build command info dynamically from loaded commands
    const commands = {};
    const examples = {};
    
    for (const [commandName, commandClass] of this.continuum.commandProcessor.commands.entries()) {
      try {
        const definition = commandClass.getDefinition();
        commands[commandName] = {
          description: definition.description,
          usage: `{"type": "task", "role": "system", "task": "[CMD:${commandName}] ${definition.params}"}`,
          params: definition.params,
          category: definition.category,
          icon: definition.icon
        };
        
        // Use first example if available
        if (definition.examples && definition.examples.length > 0) {
          examples[`${commandName} example`] = `{"type": "task", "role": "system", "task": "[CMD:${commandName}] ${definition.examples[0]}"}`;
        }
      } catch (error) {
        // Fallback for commands without proper getDefinition
        commands[commandName] = {
          description: 'Available command',
          usage: `{"type": "task", "role": "system", "task": "[CMD:${commandName}] <params>"}`
        };
      }
    }
    
    // Get agents dynamically
    const agents = this.continuum.getServiceAgents ? this.continuum.getServiceAgents() : [];
    
    const banner = {
      type: 'connection_banner',
      data: {
        welcome: `Continuum v${packageInfo.version} - Agent Command Interface`,
        motd: 'WebSocket Agent Terminal - Dynamic command discovery',
        session: sessionId,
        commands: {
          available: Object.keys(commands),
          details: commands
        },
        agents: {
          available: agents.map(agent => ({ name: agent.name || agent, role: agent.role || agent })),
          usage: 'Send messages: {"type": "message", "content": "your message", "agent": "AgentName"}'
        },
        examples: {
          ...examples,
          'Chat with AI': '{"type": "message", "content": "Hello", "room": "general"}',
          'Direct agent': '{"type": "direct_message", "agent": "CodeAI", "content": "help me"}',
          'Complete UI workflow': 'python python-client/examples/fix_ui_styling_with_feedback.py'
        },
        learning: {
          'Development Guide': './docs/AGENT_DEVELOPMENT_GUIDE.md',
          'UI Workflow Example': './python-client/examples/fix_ui_styling_with_feedback.py',
          'Screenshot Examples': './python-client/examples/',
          'Help Command': 'continuum --help'
        },
        connection_info: {
          protocol: 'WebSocket',
          endpoint: `ws://localhost:${this.continuum.port}`,
          session_timeout: '30 minutes idle',
          encoding: 'JSON messages',
          command_count: Object.keys(commands).length
        }
      }
    };
    
    ws.send(JSON.stringify(banner));
  }

  sendStatus(ws, sessionId) {
    // Clear require cache to get fresh version info
    delete require.cache[require.resolve('../../package.json')];
    const packageInfo = require('../../package.json');
    ws.send(JSON.stringify({
      type: 'status',
      data: {
        message: 'Ready to help',
        sessionId: sessionId,
        version: packageInfo.version,
        workingDir: process.cwd(),
        nodeVersion: process.version,
        pid: process.pid,
        uptime: process.uptime(),
        sessions: Array.from(this.continuum.sessions.entries()),
        costs: this.continuum.costs
      }
    }));
  }

  async handleMessage(ws, message) {
    try {
      const messageStr = message.toString();
      console.log('📨 RAW MESSAGE RECEIVED:', messageStr.length > 200 ? messageStr.substring(0, 200) + '...' : messageStr);
      const data = JSON.parse(message);
      console.log('📋 PARSED MESSAGE TYPE:', data.type);
      
      // SPECIAL DEBUG: Track screenshot_data messages
      if (data.type === 'screenshot_data') {
        console.log('🔥 SCREENSHOT_DATA MESSAGE DETECTED IN WEBSOCKET HANDLER!');
        console.log('🔥 Data keys:', Object.keys(data));
        console.log('🔥 Filename:', data.filename);
        console.log('🔥 DataURL length:', data.dataURL ? data.dataURL.length : 'no dataURL');
      }
      
      if (data.type === 'tabRegister') {
        await this.tabManager.registerTab(ws, data);
        
      } else if (data.type === 'js_executed') {
        // Handle JavaScript execution results from browser (like webpack HMR)
        console.log('🔥 SERVER: JavaScript execution result received from client:', data);
        console.log('🔥 SERVER: Console output from browser:', data.output);
        
        if (data.success) {
          console.log('✅ Browser JavaScript execution successful');
          if (data.output && data.output.length > 0) {
            console.log('📱 Browser console output:');
            data.output.forEach(entry => {
              console.log(`   [${entry.level}] ${entry.message}`);
            });
          }
          if (data.result) {
            console.log('📤 JavaScript return value:', data.result);
          }
        } else {
          console.error('❌ Browser JavaScript execution failed:', data.error);
          if (data.stack) {
            console.error('📚 Error stack:', data.stack);
          }
        }
        
        // Emit result to waiting command if execution ID exists
        if (data.executionId) {
          console.log(`🎯 SERVER: Emitting js_result_${data.executionId} with data:`, data);
          this.emit(`js_result_${data.executionId}`, data);
        } else {
          console.log('⚠️ SERVER: No executionId found in js_executed response:', data);
        }
        
        // Forward js_executed messages back to all connected clients for validation
        this.broadcast({
          type: 'js_executed',
          data: data
        });
        
      } else if (data.type === 'agent_register' || data.type === 'agent_message') {
        // Handle agent connections and messages
        this.remoteAgentManager.handleAgentMessage(ws, message, this.getSessionId(ws));
        
        // Create AgentClientConnection and trigger cross-client validation
        if (data.type === 'agent_register') {
          const sessionId = this.getSessionId(ws);
          console.log('🤖 AgentClientConnection registered for session:', sessionId);
          
          // Create AgentClientConnection
          const agentConnection = {
            sessionId: sessionId,
            ws: ws,
            clientType: 'agent',
            agentName: data.agentName || 'Unknown',
            capabilities: data.capabilities || [],
            connected: true,
            startTime: Date.now(),
            
            async validate() {
              console.log('🔥 AgentClientConnection.validate() - agent validation logic here');
              return { success: true, message: 'Agent validation complete' };
            }
          };
          
          // Store AgentClientConnection
          if (!this.continuum.agentConnections) {
            this.continuum.agentConnections = new Map();
          }
          this.continuum.agentConnections.set(sessionId, agentConnection);
          
          // Trigger validation on all browser connections when agent connects
          console.log('🤖 Agent client connected - triggering browser validations');
          if (this.activeBrowserConnections) {
            for (const [browserSessionId, browserConnection] of this.activeBrowserConnections.entries()) {
              try {
                console.log(`📸 Triggering validation for browser ${browserSessionId} due to agent connection`);
                const result = await browserConnection.validate();
                console.log('📊 Cross-client validation result:', result);
              } catch (error) {
                console.error(`❌ Cross-client validation failed for ${browserSessionId}:`, error);
              }
            }
          }
        }
        
      } else if (data.type === 'task') {
        const { role, task } = data;
        
        console.log(`🎯 WEBSOCKET_TASK: ${role} -> ${task}`);
        console.log(`🎯 WEBSOCKET_TASK: Full task string: "${task}"`);
        console.log(`🎯 WEBSOCKET_TASK: Task contains [CMD:? ${task.includes('[CMD:')}`);
        
        // Send working status immediately (not queued)
        ws.send(JSON.stringify({
          type: 'working',
          data: `🤖 ${role} processing: ${task.substring(0, 50)}...`
        }));
        
        console.log(`🎯 WEBSOCKET_TASK: Calling messageQueue.queueTaskResult`);
        
        // Queue the task result - this should call intelligentRoute
        this.messageQueue.queueTaskResult(ws, task, role, this.continuum);
        
      } else if (data.type === 'message') {
        // Handle general chat messages with auto-routing and Sheriff validation
        const { content, room = 'general' } = data;
        
        console.log(`💬 ${room} message (auto-route): ${content}`);
        
        try {
          // Check if AI models are available
          if (!this.continuum.modelRegistry || this.continuum.modelRegistry.getAvailableModels().length === 0) {
            ws.send(JSON.stringify({
              type: 'response',
              message: 'I need API keys configured to process AI requests. Please add ANTHROPIC_API_KEY or OPENAI_API_KEY to your .continuum/config.env file.',
              agent: 'System',
              room: room,
              sheriff_status: 'NO_API_KEYS'
            }));
            return;
          }

          const result = await this.continuum.intelligentRoute(content);
          
          // Always run through Protocol Sheriff for validation (if available)
          let finalResponse = result.result;
          let sheriffStatus = 'VALID';
          
          if (this.continuum.protocolSheriff && this.continuum.modelCaliber) {
            try {
              const validation = await this.continuum.protocolSheriff.validateResponse(
                result.result, 
                content, 
                result.role
              );
              
              if (!validation.isValid && validation.correctedResponse) {
                console.log(`🚨 Protocol Sheriff: Using corrected response`);
                finalResponse = validation.correctedResponse;
                sheriffStatus = 'CORRECTED';
              }
            } catch (validationError) {
              console.log('⚠️ Protocol Sheriff validation failed, using original response');
              sheriffStatus = 'SHERIFF_ERROR';
            }
          } else {
            sheriffStatus = 'SHERIFF_UNAVAILABLE';
          }
          
          ws.send(JSON.stringify({
            type: 'response',
            message: finalResponse,
            agent: result.role,
            room: room,
            sheriff_status: sheriffStatus
          }));
        } catch (error) {
          console.error('Message processing error:', error);
          ws.send(JSON.stringify({
            type: 'response',
            message: `Sorry, I encountered an error: ${error.message}. Please check that your API keys are properly configured.`,
            agent: 'System',
            room: room,
            sheriff_status: 'ERROR'
          }));
        }
        
      } else if (data.type === 'direct_message') {
        // Handle direct messages to specific agents
        const { content, agent, room = 'general' } = data;
        
        console.log(`📋 Direct message to ${agent}: ${content}`);
        
        try {
          const result = await this.continuum.sendTask(agent, content);
          
          // Always run through Protocol Sheriff for validation
          const validation = await this.continuum.protocolSheriff.validateResponse(
            result, 
            content, 
            agent
          );
          
          let finalResponse = result;
          if (!validation.isValid && validation.correctedResponse) {
            console.log(`🚨 Protocol Sheriff: Using corrected response`);
            finalResponse = validation.correctedResponse;
          }
          
          ws.send(JSON.stringify({
            type: 'response',
            message: finalResponse,
            agent: agent,
            room: room,
            sheriff_status: validation.isValid ? 'VALID' : 'CORRECTED'
          }));
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'response',
            message: `Sorry, ${agent} encountered an error: ${error.message}`,
            agent: 'System',
            room: room
          }));
        }
        
      } else if (data.type === 'group_message') {
        // Handle group chat messages to multiple agents
        const { content, agents, room = 'general' } = data;
        
        console.log(`👥 Group message to [${agents.join(', ')}]: ${content}`);
        
        // Send to each agent and collect responses
        const responses = [];
        for (const agent of agents) {
          try {
            const result = await this.continuum.sendTask(agent, 
              `${content}\n\n[GROUP CHAT CONTEXT: You are in a group chat with ${agents.filter(a => a !== agent).join(', ')}. Keep your response concise and collaborative.]`
            );
            
            // Always run through Protocol Sheriff for validation
            const validation = await this.continuum.protocolSheriff.validateResponse(
              result, 
              content, 
              agent
            );
            
            let finalResponse = result;
            if (!validation.isValid && validation.correctedResponse) {
              console.log(`🚨 Protocol Sheriff: Using corrected response for ${agent}`);
              finalResponse = validation.correctedResponse;
            }
            
            responses.push({
              agent: agent,
              message: finalResponse,
              sheriff_status: validation.isValid ? 'VALID' : 'CORRECTED'
            });
          } catch (error) {
            responses.push({
              agent: agent,
              message: `Error: ${error.message}`,
              sheriff_status: 'ERROR'
            });
          }
        }
        
        // Send all responses
        for (const response of responses) {
          ws.send(JSON.stringify({
            type: 'response',
            message: response.message,
            agent: response.agent,
            room: room,
            sheriff_status: response.sheriff_status,
            group_chat: true
          }));
        }
        
      } else if (data.type === 'academy_message') {
        // Handle Academy chat messages
        const { content } = data;
        
        console.log(`🎓 Academy message: ${content}`);
        
        ws.send(JSON.stringify({
          type: 'response',
          message: 'Academy training system is ready. Use the buttons to start training agents.',
          agent: 'Academy System',
          room: 'academy'
        }));
        
      } else if (data.type === 'start_academy_training') {
        // Handle Academy training requests
        const { personaName, specialization, rounds, customPrompt, trainingIntensity } = data;
        
        if (this.continuum.uiGenerator && this.continuum.uiGenerator.academyInterface) {
          const session = await this.continuum.uiGenerator.academyInterface.startAcademyTraining(
            personaName, 
            specialization, 
            { 
              rounds,
              customPrompt,
              trainingIntensity
            }
          );
          
          const message = customPrompt ? 
            `🎓 ${personaName} enrolled in custom Academy training for ${specialization}!` :
            `🎓 ${personaName} has been enrolled in Academy training for ${specialization}!`;
          
          ws.send(JSON.stringify({
            type: 'response',
            message: message,
            agent: 'Academy System',
            room: 'academy'
          }));
        }
        
      } else if (data.type === 'get_initial_academy_status') {
        // INTERRUPT-DRIVEN: Send initial Academy status once, then rely on push updates
        console.log('🎓 Initial Academy status requested');
        
        if (this.continuum.uiGenerator && this.continuum.uiGenerator.academyInterface) {
          const status = this.continuum.uiGenerator.academyInterface.getAcademyStatus();
          
          ws.send(JSON.stringify({
            type: 'academy_status_push',
            status: status
          }));
          console.log('🎓 PUSH: Initial Academy status sent');
        }
        
      } else if (data.type === 'get_academy_status') {
        // OLD POLLING METHOD - DISABLED
        console.log('🎓 Old polling request ignored - use push-based updates instead');
        return;
        
        if (this.continuum.uiGenerator && this.continuum.uiGenerator.academyInterface) {
          console.log('🎓 Getting Academy status...');
          const status = this.continuum.uiGenerator.academyInterface.getAcademyStatus();
          console.log('🎓 Academy status retrieved:', JSON.stringify(status, null, 2));
          
          const response = {
            type: 'academy_status',
            status: status
          };
          console.log('🎓 Sending response:', JSON.stringify(response, null, 2));
          
          try {
            ws.send(JSON.stringify(response));
            console.log('🎓 ✅ Academy status sent successfully to client');
          } catch (sendError) {
            console.error('🎓 ❌ Failed to send Academy status:', sendError);
          }
        } else {
          console.log('🎓 Academy interface not available, sending empty status');
          console.log('🎓 UIGenerator available:', !!this.continuum.uiGenerator);
          if (this.continuum.uiGenerator) {
            console.log('🎓 Academy interface available:', !!this.continuum.uiGenerator.academyInterface);
          }
          
          // Send empty status if academy interface is not available
          const emptyResponse = {
            type: 'academy_status',
            status: {
              activeTraining: [],
              completed: [],
              stats: {
                totalPersonas: 0,
                activeTraining: 0,
                graduated: 0,
                failed: 0
              }
            }
          };
          
          try {
            ws.send(JSON.stringify(emptyResponse));
            console.log('🎓 ✅ Empty Academy status sent successfully to client');
          } catch (sendError) {
            console.error('🎓 ❌ Failed to send empty Academy status:', sendError);
          }
        }
        
      } else if (data.type === 'console_log') {
        // Handle console.log messages from browser
        try {
          const { level, message, timestamp } = data;
          const sessionId = this.getSessionId(ws);
          
          // Log to console immediately (non-blocking)
          console.log(`🖥️ Browser [${level}]:`, message);
          
          // Log to file (non-blocking)
          this.browserLogger.logConsoleMessage(level, message, {
            sessionId,
            tabId: data.tabId,
            url: data.url,
            timestamp
          }).catch(err => console.error('Failed to log browser console message:', err));
        } catch (error) {
          console.error('Error handling console_log message:', error);
        }
        
      } else if (data.type === 'javascript_error') {
        // Handle JavaScript errors from browser
        try {
          const { error, stack, url, line, column, timestamp } = data;
          const sessionId = this.getSessionId(ws);
          
          // Log to console immediately (non-blocking)
          console.error('🚨 Browser JS Error:', error);
          
          // Log to file (non-blocking)
          this.browserLogger.logJavaScriptError(error, {
            sessionId,
            tabId: data.tabId,
            stack,
            url,
            line,
            column,
            timestamp
          }).catch(err => console.error('Failed to log browser JS error:', err));
        } catch (error) {
          console.error('Error handling javascript_error message:', error);
        }
        
      } else if (data.type === 'js_error') {
        // Handle JavaScript errors from browser (legacy)
        try {
          console.error('🚨 JavaScript Error from Browser:', {
            message: data.message,
            url: data.url,
            line: data.line,
            column: data.column,
            error: data.error,
            timestamp: data.timestamp
          });
          
          const sessionId = this.getSessionId(ws);
          this.browserLogger.logJavaScriptError(data.error || data.message, {
            sessionId,
            tabId: data.tabId,
            url: data.url,
            line: data.line,
            column: data.column,
            timestamp: data.timestamp
          }).catch(err => console.error('Failed to log legacy JS error:', err));
        } catch (error) {
          console.error('Error handling js_error message:', error);
        }
        
      } else if (data.type === 'js_promise_error') {
        // Handle promise rejection errors from browser
        try {
          console.error('🚨 JavaScript Promise Error from Browser:', {
            message: data.message,
            timestamp: data.timestamp
          });
          
          const sessionId = this.getSessionId(ws);
          this.browserLogger.logJavaScriptError(data.message, {
            sessionId,
            tabId: data.tabId,
            timestamp: data.timestamp,
            type: 'promise_rejection'
          }).catch(err => console.error('Failed to log promise error:', err));
        } catch (error) {
          console.error('Error handling js_promise_error message:', error);
        }
        
      } else if (data.type === 'version_check') {
        // Handle manual version check requests
        console.log('🔍 Manual version check requested via WebSocket');
        
        try {
          const restarted = await this.continuum.versionManager.checkAndRestart();
          
          if (!restarted) {
            ws.send(JSON.stringify({
              type: 'response',
              message: '✅ Version is up to date - no restart needed',
              agent: 'Version Manager',
              room: 'system'
            }));
          }
          // If restarted, the connection will be closed anyway
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'response',
            message: `❌ Version check failed: ${error.message}`,
            agent: 'Version Manager',
            room: 'system'
          }));
        }
        
      } else if (data.type === 'client_initialize') {
        // Handle client initialization - create server-side ClientConnection
        try {
          console.log('🚀 CLIENT INITIALIZATION:', data.clientType);
          const sessionId = this.getSessionId(ws);
          
          if (data.clientType === 'browser') {
            // Create BrowserClientConnection for this session
            console.log('🌐 Creating BrowserClientConnection for session:', sessionId);
            
            // Store the browser client connection
            if (!this.continuum.browserConnections) {
              this.continuum.browserConnections = new Map();
            }
            
            const browserConnection = {
              sessionId: sessionId,
              ws: ws,
              capabilities: data.capabilities || [],
              connected: true,
              startTime: Date.now(),
              
              async validate() {
                console.log('🔥 BrowserClientConnection.validate() starting...');
                
                try {
                  // Wait for client to be ready
                  console.log('⏳ Waiting for browser client ready state...');
                  await new Promise(resolve => setTimeout(resolve, 500));
                  
                  // Send validation script to browser
                  console.log('📤 Sending validation script to browser...');
                  const validationScript = `
                    console.log('🔥 SERVER-TRIGGERED BROWSER VALIDATION STARTED');
                    console.log('⏰ Timestamp:', new Date().toISOString());
                    console.log('🌐 User Agent:', navigator.userAgent);
                    
                    // Test version reading
                    const versionBadge = document.querySelector('.version-badge');
                    const versionText = versionBadge ? versionBadge.textContent.trim() : 'NO_VERSION_FOUND';
                    console.log('📋 VERSION_READ_RESULT:', versionText);
                    
                    // Test error generation
                    console.warn('⚠️ TEST WARNING from server validation');
                    console.error('🔴 TEST ERROR from server validation');
                    
                    // Auto screenshot if possible  
                    if (typeof html2canvas !== 'undefined' && versionBadge && window.ws && window.ws.readyState === WebSocket.OPEN) {
                      console.log('📸 Server-triggered FULL PAGE screenshot with dark background...');
                      
                      // Use working html2canvas approach for automatic validation
                      html2canvas(document.querySelector("body > div") || document.body, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 0.8,
                        backgroundColor: "#0f1419"
                      }).then(canvas => {
                        console.log('✅ Server-triggered screenshot successful!');
                        console.log('📐 Canvas size:', canvas.width + 'x' + canvas.height);
                        
                        const dataURL = canvas.toDataURL('image/png');
                        const timestamp = Date.now();
                        const filename = 'auto-validation-' + timestamp + '.png';
                        
                        const screenshotData = {
                          type: 'screenshot_data',
                          filename: filename,
                          dataURL: dataURL,
                          timestamp: timestamp,
                          source: 'server_auto_validation',
                          dimensions: { width: canvas.width, height: canvas.height }
                        };
                        
                        console.log('📤 SENDING AUTO-VALIDATION SCREENSHOT TO SERVER');
                        window.ws.send(JSON.stringify(screenshotData));
                        console.log('✅ Auto-validation screenshot sent successfully');
                        
                      }).catch(error => {
                        console.log('❌ Server-triggered screenshot failed:', error.message);
                        
                        // Fallback to version badge only if full page fails
                        html2canvas(versionBadge, {
                          allowTaint: true,
                          useCORS: true,
                          scale: 1
                        }).then(canvas => {
                          const dataURL = canvas.toDataURL('image/png');
                          const timestamp = Date.now();
                          const filename = 'fallback-auto-' + timestamp + '.png';
                          
                          const screenshotData = {
                            type: 'screenshot_data',
                            filename: filename,
                            dataURL: dataURL,
                            timestamp: timestamp,
                            source: 'server_fallback_validation',
                            dimensions: { width: canvas.width, height: canvas.height }
                          };
                          
                          console.log('📤 Sending fallback auto-validation screenshot');
                          window.ws.send(JSON.stringify(screenshotData));
                        });
                      });
                      
                      // Legacy getDisplayMedia code (requires user permission, commenting out)
                      /*
                      console.log('🔥 SCREENSHOT METHOD SELECTION STARTING...');
                      console.log('🔍 Checking navigator.mediaDevices:', !!navigator.mediaDevices);
                      console.log('🔍 Checking getDisplayMedia:', !!(navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia));
                      
                      if (navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia) {
                        console.log('✅ getDisplayMedia available - attempting screen capture');
                        
                        try {
                          console.log('🚀 Calling getDisplayMedia...');
                          const captureStream = await navigator.mediaDevices.getDisplayMedia({
                            video: { mediaSource: 'window' }
                          });
                          console.log('✅ getDisplayMedia permission granted, stream received');
                          
                          const video = document.createElement('video');
                          video.srcObject = captureStream;
                          console.log('📺 Video element created and stream assigned');
                          
                          await video.play();
                          console.log('▶️ Video started playing');
                          
                          video.onloadedmetadata = () => {
                            console.log('📊 Video metadata loaded:', {
                              width: video.videoWidth,
                              height: video.videoHeight
                            });
                            
                            const canvas = document.createElement('canvas');
                            const context = canvas.getContext('2d');
                            canvas.width = video.videoWidth;
                            canvas.height = video.videoHeight;
                            console.log('🎨 Canvas created with dimensions:', canvas.width + 'x' + canvas.height);
                            
                            context.drawImage(video, 0, 0);
                            console.log('🖼️ Video frame drawn to canvas');
                            
                            const dataURL = canvas.toDataURL('image/png');
                            const timestamp = Date.now();
                            const filename = 'getDisplayMedia-validation-' + timestamp + '.png';
                            console.log('💾 DataURL generated, length:', dataURL.length);
                            
                            // Stop the capture stream
                            captureStream.getTracks().forEach(track => track.stop());
                            console.log('⏹️ Capture stream stopped');
                            
                            const screenshotData = {
                              type: 'screenshot_data',
                              filename: filename,
                              dataURL: dataURL,
                              timestamp: timestamp,
                              source: 'getDisplayMedia_validation',
                              dimensions: { width: canvas.width, height: canvas.height }
                            };
                            
                            console.log('📤 SENDING GETDISPLAYMEDIA SCREENSHOT TO SERVER');
                            window.ws.send(JSON.stringify(screenshotData));
                            console.log('✅ getDisplayMedia screenshot sent successfully');
                          };
                          
                        } catch (error) {
                          console.log('❌ getDisplayMedia FAILED with error:', error.message);
                          console.log('🔄 FALLING BACK TO HTML2CANVAS');
                          
                          html2canvas(versionBadge, {
                            allowTaint: true,
                            useCORS: true,
                            scale: 1
                          }).then(canvas => {
                            console.log('📸 html2canvas fallback successful');
                            const dataURL = canvas.toDataURL('image/png');
                            const timestamp = Date.now();
                            const filename = 'fallback-validation-' + timestamp + '.png';
                            
                            const screenshotData = {
                              type: 'screenshot_data',
                              filename: filename,
                              dataURL: dataURL,
                              timestamp: timestamp,
                              source: 'html2canvas_fallback',
                              dimensions: { width: canvas.width, height: canvas.height }
                            };
                            
                            console.log('📤 Sending fallback screenshot to server');
                            window.ws.send(JSON.stringify(screenshotData));
                          });
                        }
                      } else {
                        console.log('❌ getDisplayMedia NOT AVAILABLE');
                        console.log('🔄 USING HTML2CANVAS DIRECTLY');
                        
                        html2canvas(versionBadge, {
                          allowTaint: true,
                          useCORS: true,
                          scale: 1
                        }).then(canvas => {
                          console.log('📸 html2canvas direct capture successful');
                          const dataURL = canvas.toDataURL('image/png');
                          const timestamp = Date.now();
                          const filename = 'html2canvas-validation-' + timestamp + '.png';
                          
                          const screenshotData = {
                            type: 'screenshot_data',
                            filename: filename,
                            dataURL: dataURL,
                            timestamp: timestamp,
                            source: 'html2canvas_validation',
                            dimensions: { width: canvas.width, height: canvas.height }
                          };
                          
                          console.log('📤 Sending html2canvas screenshot to server');
                          window.ws.send(JSON.stringify(screenshotData));
                        });
                      }
                      */
                    }
                    
                    console.log('🎯 SERVER-TRIGGERED BROWSER VALIDATION COMPLETE');
                    "SERVER_VALIDATION_COMPLETE";
                  `;
                  
                  // Send via BROWSER_JS command
                  const encoded = Buffer.from(validationScript).toString('base64');
                  const command = {
                    type: 'task',
                    role: 'system',
                    task: `[CMD:BROWSER_JS] ${encoded}`
                  };
                  
                  ws.send(JSON.stringify(command));
                  console.log('✅ BrowserClientConnection validation script sent');
                  
                  return { success: true, message: 'Browser validation triggered' };
                  
                } catch (error) {
                  console.error('❌ BrowserClientConnection.validate() failed:', error);
                  return { success: false, error: error.message };
                }
              }
            };
            
            this.continuum.browserConnections.set(sessionId, browserConnection);
            console.log('✅ BrowserClientConnection created for session:', sessionId);
            
            // Send confirmation to browser client
            const confirmationMessage = {
              type: 'client_connection_confirmed',
              clientType: 'browser',
              sessionId: sessionId,
              timestamp: Date.now()
            };
            ws.send(JSON.stringify(confirmationMessage));
            console.log('📤 Browser client connection confirmation sent');
            
            // Call validate() automatically after confirmation
            setTimeout(async () => {
              console.log('🔥 Auto-calling BrowserClientConnection.validate()...');
              const result = await browserConnection.validate();
              console.log('📊 Validation result:', result);
            }, 200);
            
            // Store reference for cross-client validation
            if (!this.activeBrowserConnections) {
              this.activeBrowserConnections = new Map();
            }
            this.activeBrowserConnections.set(sessionId, browserConnection);
            
          } else {
            console.log('🤖 Other client type:', data.clientType);
          }
          
        } catch (error) {
          console.error('❌ Failed to handle client initialization:', error);
        }
        
      } else if (data.type === 'screenshot_data') {
        // Handle browser canvas screenshot data using centralized ScreenshotService
        try {
          console.log('📸 PROCESSING SCREENSHOT_DATA MESSAGE');
          console.log('📸 Data keys:', Object.keys(data));
          console.log('📸 Filename:', data.filename);
          console.log('📸 DataURL length:', data.dataURL ? data.dataURL.length : 'undefined');
          console.log('📸 Dimensions:', data.dimensions);
          
          const { dataURL, filename, dimensions } = data;
          const sessionId = this.getSessionId(ws);
          
          console.log(`📸 Received browser screenshot: ${filename} (${dimensions?.width}x${dimensions?.height})`);
          
          // Delegate to FileSave command - proper command architecture
          try {
            const FileSaveCommand = require('../commands/core/FileSaveCommand.cjs');
            const fileSaveCmd = new FileSaveCommand();
            
            const fileSaveParams = JSON.stringify({
              filename: filename,
              data: dataURL,
              baseDirectory: '.continuum/screenshots',
              mimeType: 'image/png'
            });
            
            console.log(`💾 Delegating to FileSave command: ${filename}`);
            const result = await fileSaveCmd.execute(fileSaveParams, this.continuum);
            
            if (result.success) {
              console.log(`✅ FileSave completed: ${result.data.filename} (${result.data.fileSizeKB}KB)`);
              
              // Log to browser logger with actual file size
              this.browserLogger.logUserInteraction('screenshot', filename, {
                sessionId,
                tabId: data.tabId,
                dimensions,
                fileSize: result.data.fileSize,
                timestamp: data.timestamp
              }).catch(err => console.error('Failed to log screenshot interaction:', err));
              
            } else {
              console.error(`❌ FileSave failed: ${result.message}`);
            }
          } catch (error) {
            console.error(`❌ Failed to execute FileSave command: ${error.message}`);
          }
          
          // Use ShareCommand for auto-opening (when available)
          try {
            const ShareCommand = require('../commands/core/ShareCommand.cjs');
            await ShareCommand.share(outputPath, 'user');
            console.log(`📤 Screenshot shared via ShareCommand`);
          } catch (error) {
            console.log(`⚠️ ShareCommand not available: ${error.message}`);
          }
          
          // Log to browser logger
          this.browserLogger.logUserInteraction('screenshot', filename, {
            sessionId,
            tabId: data.tabId,
            dimensions,
            fileSize: buffer.length,
            timestamp: data.timestamp
          }).catch(err => console.error('Failed to log screenshot interaction:', err));
          
        } catch (error) {
          console.error('❌ Failed to save browser screenshot:', error);
        }
        
      } else if (data.type === 'screenshot_error') {
        // Handle browser screenshot errors
        try {
          const { error, filename } = data;
          const sessionId = this.getSessionId(ws);
          
          console.error(`❌ Browser screenshot failed: ${filename} - ${error}`);
          
          // Log error
          this.browserLogger.logJavaScriptError(`Screenshot failed: ${error}`, {
            sessionId,
            tabId: data.tabId,
            filename,
            timestamp: data.timestamp,
            type: 'screenshot_error'
          }).catch(err => console.error('Failed to log screenshot error:', err));
          
        } catch (error) {
          console.error('Error handling screenshot error message:', error);
        }
        
      } else if (data.type === 'get_component_css') {
        // Handle CSS loading for modular components
        console.log('📄 ================================');
        console.log('📄 CSS REQUEST RECEIVED');
        console.log('📄 Component:', data.component);
        console.log('📄 Path:', data.path);
        console.log('📄 ================================');
        
        try {
          const fs = require('fs');
          const path = require('path');
          
          if (data.path && data.component) {
            // Path should be relative to project root, not relative to WebSocketServer
            const projectRoot = path.join(__dirname, '..', '..');
            const cssPath = path.join(projectRoot, 'src', data.path.replace(/^\//, ''));
            
            console.log('📄 Project root:', projectRoot);
            console.log('📄 Requested path:', data.path);
            console.log('📄 Final CSS path:', cssPath);
            console.log('📄 File exists check:', fs.existsSync(cssPath));
            
            if (fs.existsSync(cssPath)) {
              console.log('📄 Reading CSS file...');
              const cssContent = fs.readFileSync(cssPath, 'utf8');
              console.log('📄 ✅ CSS loaded successfully, length:', cssContent.length);
              
              const response = {
                type: 'component_css_response',
                component: data.component,
                css: cssContent,
                path: data.path
              };
              
              console.log('📄 Sending CSS response...');
              ws.send(JSON.stringify(response));
              console.log('📄 ✅ CSS response sent successfully');
              
            } else {
              console.log('📄 ❌ CSS file not found at:', cssPath);
              
              // Try listing the directory to see what's there
              const dir = path.dirname(cssPath);
              console.log('📄 Directory listing for:', dir);
              try {
                const files = fs.readdirSync(dir);
                console.log('📄 Files in directory:', files);
              } catch (dirError) {
                console.log('📄 Could not read directory:', dirError.message);
              }
              
              ws.send(JSON.stringify({
                type: 'component_css_response',
                component: data.component,
                css: null,
                error: 'CSS file not found'
              }));
            }
          } else {
            console.log('📄 ❌ Invalid CSS request, missing component or path');
            ws.send(JSON.stringify({
              type: 'component_css_response',
              error: 'Invalid request - missing component or path'
            }));
          }
        } catch (error) {
          console.error('📄 ❌ Error handling CSS request:', error);
          console.error('📄 Error stack:', error.stack);
          ws.send(JSON.stringify({
            type: 'component_css_response',
            error: error.message
          }));
        }
        
        console.log('📄 ================================');
        
      }
    } catch (error) {
      console.error('Message error:', error);
      ws.send(JSON.stringify({
        type: 'error',
        data: `Message error: ${error.message}`
      }));
    }
  }

  handleDisconnect(sessionId) {
    console.log(`👤 User disconnected (Session: ${sessionId})`);
    
    // Clean up ClientConnections
    if (this.activeBrowserConnections) {
      this.activeBrowserConnections.delete(sessionId);
    }
    if (this.continuum.agentConnections) {
      this.continuum.agentConnections.delete(sessionId);
    }
    if (this.continuum.browserConnections) {
      this.continuum.browserConnections.delete(sessionId);
    }
    
    // Update Continuon status indicator
    if (this.continuum.systemTray) {
      this.continuum.systemTray.onUserDisconnected();
    }
    this.continuum.activeConnections.delete(sessionId);
    this.continuum.conversationThreads.delete(sessionId);
    this.messageQueue.cleanup(sessionId);
    this.tabManager.cleanupSession(sessionId);
  }

  handleError(sessionId, error) {
    console.error(`WebSocket error for session ${sessionId}:`, error);
  }

  /**
   * Get tab management status
   */
  getTabStatus() {
    return this.tabManager.getStatus();
  }

  /**
   * Trigger version update broadcast to all tabs
   */
  async broadcastVersionUpdate(newVersion) {
    return this.tabManager.broadcastVersionUpdate(newVersion);
  }

  /**
   * Broadcast message to all connected clients
   */
  broadcast(message) {
    const messageString = JSON.stringify(message);
    
    this.continuum.activeConnections.forEach((ws, sessionId) => {
      if (ws.readyState === ws.OPEN) {
        try {
          ws.send(messageString);
        } catch (error) {
          console.error(`Failed to broadcast to session ${sessionId}:`, error);
          this.handleDisconnect(sessionId);
        }
      }
    });
  }
}

module.exports = WebSocketServer;