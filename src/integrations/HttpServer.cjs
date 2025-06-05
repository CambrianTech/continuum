/**
 * HTTP Server
 * Handles HTTP requests and serves the web UI
 */

const http = require('http');

class HttpServer {
  constructor(continuum) {
    this.continuum = continuum;
    this.server = null;
  }

  createServer() {
    this.server = http.createServer(async (req, res) => {
      try {
        let body = '';
        req.on('data', chunk => {
          body += chunk.toString();
        });

        req.on('end', async () => {
          await this.handleRequest(req, res, body);
        });
      } catch (error) {
        console.error('Server error:', error);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Internal server error' }));
      }
    });

    return this.server;
  }

  async handleRequest(req, res, body) {
    const url = new URL(req.url, `http://localhost:${this.continuum.port}`);
    
    // Debug logging for version endpoint
    if (url.pathname === '/version') {
      console.log(`🔍 Version endpoint hit: ${req.method} ${url.pathname}`);
    }
    
    if (req.method === 'GET' && url.pathname === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(this.continuum.generateUI());
    } else if (req.method === 'GET' && url.pathname === '/debug') {
      await this.handleDebugRequest(res);
    } else if (req.method === 'GET' && url.pathname.startsWith('/components/')) {
      await this.handleComponentRequest(url, res);
    } else if (req.method === 'GET' && url.pathname.endsWith('.js')) {
      await this.handleJavaScriptRequest(url, res);
    } else if (req.method === 'GET' && url.pathname === '/version') {
      console.log('🎯 Handling version request');
      await this.handleVersionRequest(res);
    } else if (req.method === 'GET' && url.pathname === '/ask') {
      await this.handleAskRequest(url, res);
    } else if (req.method === 'GET' && url.pathname === '/screenshot') {
      await this.handleScreenshotRequest(url, res);
    } else if ((req.method === 'GET' || req.method === 'POST') && url.pathname === '/connect') {
      await this.handleConnectRequest(req, res, url, body);
    } else if (req.method === 'GET' && (url.pathname === '/status' || url.pathname === '/api/status')) {
      await this.handleStatusRequest(res);
    } else if (req.method === 'GET' && url.pathname === '/api/agents') {
      await this.handleAgentsRequest(res);
    } else if (url.pathname.startsWith('/api/personas')) {
      await this.handlePersonaRequest(req, res, url, body);
    } else if (url.pathname.startsWith('/api/projects')) {
      await this.handleProjectRequest(req, res, url, body);
    } else if (url.pathname.startsWith('/api/academy')) {
      await this.handleAcademyRequest(req, res, url, body);
    } else if (req.method === 'GET' && url.pathname === '/api/commands') {
      await this.handleCommandsRequest(res);
    } else {
      res.writeHead(404);
      res.end('Not found');
    }
  }

  async handleDebugRequest(res) {
    const fs = require('fs');
    const path = require('path');
    
    try {
      const debugPath = path.join(__dirname, '..', '..', 'debug-ui.html');
      if (fs.existsSync(debugPath)) {
        const debugContent = fs.readFileSync(debugPath, 'utf8');
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(debugContent);
        console.log('✅ Served debug interface');
      } else {
        res.writeHead(404);
        res.end('Debug interface not found');
      }
    } catch (error) {
      console.error('Error serving debug interface:', error);
      res.writeHead(500);
      res.end('Server error');
    }
  }

  async handleJavaScriptRequest(url, res) {
    const fs = require('fs');
    const path = require('path');
    
    let jsFile = url.pathname.substring(1); // Remove leading slash
    
    // Handle src/ui/ prefix - strip it to get relative file path
    if (jsFile.startsWith('src/ui/')) {
      jsFile = jsFile.substring('src/ui/'.length);
    }
    
    const possiblePaths = [
      path.join(__dirname, '..', 'ui', jsFile),
      path.join(__dirname, '..', 'ui', 'utils', jsFile),
      path.join(__dirname, '..', 'ui', 'components', jsFile),
      path.join(__dirname, '..', 'ui', 'widgets', jsFile)
    ];
    
    for (const filePath of possiblePaths) {
      try {
        if (fs.existsSync(filePath)) {
          const content = fs.readFileSync(filePath, 'utf8');
          res.writeHead(200, { 'Content-Type': 'application/javascript' });
          res.end(content);
          console.log(`✅ Served JS file: ${url.pathname} -> ${filePath}`);
          return;
        }
      } catch (error) {
        console.error(`Error checking ${filePath}:`, error);
      }
    }
    
    console.log(`❌ JS file not found: ${url.pathname}`);
    res.writeHead(404);
    res.end('JavaScript file not found');
  }

  async handleComponentRequest(url, res) {
    const fs = require('fs');
    const path = require('path');
    
    const componentName = url.pathname.split('/components/')[1];
    let componentPath = path.join(__dirname, '..', 'ui', 'components', componentName);
    
    // Check if it's a utils file
    if (componentName.includes('ComponentLoader')) {
      componentPath = path.join(__dirname, '..', 'ui', 'utils', componentName);
    }
    
    try {
      if (fs.existsSync(componentPath)) {
        const componentContent = fs.readFileSync(componentPath, 'utf8');
        res.writeHead(200, { 'Content-Type': 'application/javascript' });
        res.end(componentContent);
        console.log(`✅ Served component: ${componentName}`);
      } else {
        console.log(`❌ Component not found: ${componentPath}`);
        res.writeHead(404);
        res.end('Component not found');
      }
    } catch (error) {
      console.error('Error serving component:', error);
      res.writeHead(500);
      res.end('Server error');
    }
  }

  async handleAskRequest(url, res) {
    const task = url.searchParams.get('task');
    
    console.log(`📨 Web request received: ${task}`);
    
    if (task) {
      try {
        console.log(`🔄 Processing task: ${task}`);
        
        // Get the initial agent that will handle this task
        const initialAgent = this.continuum.getInitialAgent(task);
        
        const result = await this.continuum.intelligentRoute(task);
        console.log(`✅ Task completed, sending response...`);
        
        // 🤖 Protocol Sheriff: Validate response before sending to user
        const validation = await this.continuum.protocolSheriff.validateResponse(
          result.result, 
          task, 
          result.role
        );
        
        if (!validation.isValid && validation.correctedResponse) {
          console.log(`🚨 Protocol Sheriff: Using corrected response`);
          result.result = validation.correctedResponse;
        }
        
        // Add initial agent info and working status for better UI feedback
        result.initialAgent = initialAgent;
        
        if (result.coordination) {
          result.workingMessages = [
            'Analyzing request and selecting best AI...',
            'Multi-AI coordination required...',
            'Coordinating between AIs...',
            'Finalizing...'
          ];
        } else {
          result.workingMessages = [
            `${result.role} is thinking...`,
            'Formulating response...'
          ];
        }
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(result));
      } catch (error) {
        console.error(`❌ Task failed: ${error.message}`);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: error.message, stack: error.stack }));
      }
    } else {
      console.log(`⚠️  No task provided in request`);
      res.writeHead(400, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'No task provided' }));
    }
  }

  async handleScreenshotRequest(url, res) {
    console.log('📸 Screenshot API endpoint hit');
    
    // Extract coordinate parameters
    const x = parseInt(url.searchParams.get('x')) || 0;
    const y = parseInt(url.searchParams.get('y')) || 0;
    const width = parseInt(url.searchParams.get('width')) || 0;
    const height = parseInt(url.searchParams.get('height')) || 0;
    const selector = url.searchParams.get('selector') || '';
    const scale = parseFloat(url.searchParams.get('scale')) || 1.0;
    const resolutionWidth = parseInt(url.searchParams.get('resWidth')) || 0;
    const resolutionHeight = parseInt(url.searchParams.get('resHeight')) || 0;
    const quality = parseFloat(url.searchParams.get('quality')) || 0.92;
    const format = url.searchParams.get('format') || 'png';
    
    console.log(`📸 Screenshot params: x=${x}, y=${y}, w=${width}, h=${height}, scale=${scale}, resW=${resolutionWidth}, resH=${resolutionHeight}, quality=${quality}, format=${format}, selector="${selector}"`);
    
    try {
      // Send JavaScript to browsers to capture screenshot
      const screenshotJS = `
        console.log('📸 Screenshot API: Starting capture...');
        
        const captureParams = {
          x: ${x},
          y: ${y},
          width: ${width},
          height: ${height},
          selector: '${selector}',
          scale: ${scale},
          resolutionWidth: ${resolutionWidth},
          resolutionHeight: ${resolutionHeight},
          quality: ${quality},
          format: '${format}'
        };
        
        // Load html2canvas if not already loaded
        if (typeof html2canvas === 'undefined') {
          const script = document.createElement('script');
          script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
          script.onload = function() {
            console.log('📸 html2canvas loaded, starting capture...');
            captureScreenshot();
          };
          document.head.appendChild(script);
        } else {
          captureScreenshot();
        }
        
        function captureScreenshot() {
          const timestamp = Date.now();
          let filename = 'continuum-interface-' + timestamp + '.png';
          
          // Determine capture target
          let targetElement = document.body;
          let captureOptions = {
            allowTaint: true,
            useCORS: true,
            scale: 1,
            scrollX: 0,
            scrollY: 0,
            backgroundColor: '#1a1a1a'
          };
          
          // Handle selector-based capture
          if (captureParams.selector) {
            const element = document.querySelector(captureParams.selector);
            if (element) {
              targetElement = element;
              filename = 'continuum-element-' + captureParams.selector.replace(/[^a-zA-Z0-9]/g, '-') + '-' + timestamp + '.png';
              console.log('📸 Capturing element:', captureParams.selector);
            } else {
              console.warn('📸 Selector not found:', captureParams.selector);
            }
          }
          
          console.log('📸 Capturing screenshot with html2canvas...');
          
          html2canvas(targetElement, captureOptions).then(function(canvas) {
            console.log('📸 Canvas generated, processing...');
            
            let finalCanvas = canvas;
            
            // Handle coordinate-based cropping
            if (captureParams.width > 0 && captureParams.height > 0) {
              console.log('📸 Cropping to coordinates:', captureParams);
              
              const cropCanvas = document.createElement('canvas');
              const cropCtx = cropCanvas.getContext('2d');
              
              cropCanvas.width = captureParams.width;
              cropCanvas.height = captureParams.height;
              
              // Crop from the full canvas (frame buffer transfer)
              cropCtx.drawImage(
                canvas,
                captureParams.x, captureParams.y, captureParams.width, captureParams.height,
                0, 0, captureParams.width, captureParams.height
              );
              
              finalCanvas = cropCanvas;
              filename = 'continuum-cropped-' + captureParams.x + '-' + captureParams.y + '-' + captureParams.width + 'x' + captureParams.height + '-' + timestamp + '.' + captureParams.format;
            }
            
            // Handle resolution scaling (downsample/upsample)
            if (captureParams.resolutionWidth > 0 && captureParams.resolutionHeight > 0) {
              console.log('📸 Scaling to resolution:', captureParams.resolutionWidth + 'x' + captureParams.resolutionHeight);
              
              const scaleCanvas = document.createElement('canvas');
              const scaleCtx = scaleCanvas.getContext('2d');
              
              scaleCanvas.width = captureParams.resolutionWidth;
              scaleCanvas.height = captureParams.resolutionHeight;
              
              // Use different scaling algorithms based on scale direction
              if (captureParams.resolutionWidth < finalCanvas.width) {
                // Downsampling - use smooth scaling
                scaleCtx.imageSmoothingEnabled = true;
                scaleCtx.imageSmoothingQuality = 'high';
              } else {
                // Upsampling - can choose crisp or smooth
                scaleCtx.imageSmoothingEnabled = false; // Crisp pixels
              }
              
              // Frame buffer transfer with scaling
              scaleCtx.drawImage(
                finalCanvas,
                0, 0, finalCanvas.width, finalCanvas.height,
                0, 0, captureParams.resolutionWidth, captureParams.resolutionHeight
              );
              
              finalCanvas = scaleCanvas;
              filename = 'continuum-scaled-' + captureParams.resolutionWidth + 'x' + captureParams.resolutionHeight + '-' + timestamp + '.' + captureParams.format;
            }
            
            console.log('📸 Final canvas ready, triggering download...');
            
            // Create download link with format support
            const link = document.createElement('a');
            link.download = filename;
            
            // Handle different output formats
            let mimeType, dataURL;
            switch(captureParams.format) {
              case 'jpg':
              case 'jpeg':
                mimeType = 'image/jpeg';
                dataURL = finalCanvas.toDataURL(mimeType, captureParams.quality);
                break;
              case 'webp':
                mimeType = 'image/webp';
                dataURL = finalCanvas.toDataURL(mimeType, captureParams.quality);
                break;
              default:
                mimeType = 'image/png';
                dataURL = finalCanvas.toDataURL(mimeType);
            }
            
            // Send screenshot data via WebSocket instead of download
            console.log('📸 Sending screenshot data via WebSocket...');
            
            // Send via WebSocket to debug interface
            if (window.WebSocket && ws && ws.readyState === WebSocket.OPEN) {
              ws.send(JSON.stringify({
                type: 'screenshot_data',
                dataURL: dataURL,
                filename: filename,
                timestamp: Date.now()
              }));
              console.log('✅ Screenshot data sent via WebSocket');
            } else {
              console.log('⚠️ WebSocket not available, creating download fallback');
              // Fallback to download if WebSocket not available
              link.href = dataURL;
              document.body.appendChild(link);
              link.click();
              document.body.removeChild(link);
            }
            
            // Display preview
            const preview = finalCanvas.cloneNode();
            preview.getContext('2d').drawImage(finalCanvas, 0, 0);
            preview.style.cssText = 'position:fixed;top:10px;right:10px;max-width:200px;max-height:200px;border:2px solid #00ff88;z-index:9999;';
            preview.title = 'Screenshot captured: ' + filename;
            document.body.appendChild(preview);
            
            // Remove preview after 3 seconds
            setTimeout(() => {
              if (preview.parentNode) {
                preview.parentNode.removeChild(preview);
              }
            }, 3000);
            
          }).catch(function(error) {
            console.error('📸 Screenshot failed:', error);
            alert('Screenshot failed: ' + error.message);
          });
        }
      `;
      
      // Broadcast to all connected browsers
      if (this.continuum.webSocketServer) {
        this.continuum.webSocketServer.broadcast({
          type: 'execute_js',
          data: {
            command: screenshotJS,
            timestamp: new Date().toISOString()
          }
        });
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          message: 'Screenshot command sent to browser',
          timestamp: new Date().toISOString()
        }));
      } else {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: false,
          error: 'WebSocket server not available'
        }));
      }
      
    } catch (error) {
      console.error('📸 Screenshot API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        success: false,
        error: error.message
      }));
    }
  }

  async handleConnectRequest(req, res, url, body) {
    console.log('🔌 Connect API endpoint hit');
    
    try {
      let request = {};
      
      if (req.method === 'GET') {
        // GET request - return capabilities
        const capabilities = {
          commands: Array.from(this.continuum.commandProcessor.commands.keys()),
          agents: this.continuum.getServiceAgents(),
          version: require('../../package.json').version,
          endpoints: ['/connect', '/ask', '/screenshot', '/status'],
          websocket: 'ws://localhost:' + this.continuum.port,
          help: {
            websocket: {
              usage: 'Connect to ws://localhost:' + this.continuum.port + ' for real-time interaction',
              message_types: [
                '{"type": "message", "content": "your message"}',
                '{"type": "direct_message", "agent": "CodeAI", "content": "debug this"}',
                '{"type": "group_message", "agents": ["CodeAI", "PlannerAI"], "content": "collaborate"}',
                '{"type": "execute_js", "data": {"command": "console.log(\'test\')"}}',
                '{"type": "task", "role": "ScreenshotAgent", "task": "take screenshot"}'
              ]
            },
            screenshot: {
              usage: 'screenshot [selector element] [resolution WxH] [format png|jpeg|webp] [quality 0.0-1.0]',
              examples: [
                'POST /connect {"command": "SCREENSHOT"}',
                'POST /connect {"command": "SCREENSHOT", "params": "selector .sidebar"}',
                'curl "/ask?task=take screenshot"',
                'WebSocket: {"type": "task", "role": "ScreenshotAgent", "task": "take screenshot"}'
              ]
            },
            javascript: {
              usage: 'Execute JavaScript in connected browsers (use base64 encoding to avoid quote escaping)',
              examples: [
                'POST /connect {"command": "BROWSER_JS", "params": "console.log(\'test\')"}',
                'POST /connect {"command": "BROWSER_JS", "params": "Y29uc29sZS5sb2coJ3Rlc3QnKQ==", "encoding": "base64"}',
                'WebSocket: {"type": "execute_js", "data": {"command": "document.title = \'test\'"}}',
                'Console monitoring: All browser console output automatically sent to agents'
              ]
            },
            exec: {
              usage: 'exec command',
              examples: ['exec ls -la', 'exec ps aux']
            },
            file_read: {
              usage: 'file_read path/to/file',
              examples: ['file_read package.json']
            }
          }
        };
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(capabilities, null, 2));
        return;
      }
      
      // POST request - execute command
      if (body) {
        try {
          request = JSON.parse(body);
        } catch (e) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Invalid JSON' }));
          return;
        }
      }
      
      const { command, params = '', type, encoding = 'utf-8' } = request;
      
      if (!command && !type) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ 
          error: 'Missing command or type parameter',
          usage: 'POST { "command": "SCREENSHOT", "params": "selector .sidebar", "encoding": "base64" }'
        }));
        return;
      }
      
      // Decode params based on encoding (defaults to utf-8)
      let decodedParams = params;
      if (encoding === 'base64') {
        try {
          decodedParams = Buffer.from(params, 'base64').toString('utf8');
        } catch (e) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Invalid base64 encoding' }));
          return;
        }
      } else if (encoding !== 'utf-8' && encoding !== 'utf8') {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: `Unsupported encoding: ${encoding}. Use 'utf-8' or 'base64'` }));
        return;
      }
      
      // Execute command through CommandProcessor
      const cmd = command || type;
      const result = await this.continuum.commandProcessor.executeCommand(cmd.toUpperCase(), decodedParams);
      
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        success: true,
        command: cmd,
        params: params,
        result: result,
        timestamp: new Date().toISOString()
      }));
      
    } catch (error) {
      console.error('🔌 Connect API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      }));
    }
  }

  async handleStatusRequest(res) {
    const statusData = {
      sessions: Array.from(this.continuum.sessions.entries()),
      costs: this.continuum.costs,
      costDetails: this.continuum.costTracker.getDetailedReport(),
      uptime: process.uptime()
    };
    
    // Add version information via version manager
    const statusWithVersion = this.continuum.versionManager.addVersionToStatus(statusData);
    
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(statusWithVersion));
  }

  async handleAgentsRequest(res) {
    const agentConfigs = {
      'auto': {
        id: 'auto',
        name: 'Auto Route',
        role: 'Smart agent selection',
        avatar: '🧠',
        status: 'online',
        type: 'routing',
        specialization: 'Intelligent task routing',
        accuracy: '98%',
        responseTime: '50ms avg',
        tools: 'Task analysis, agent selection',
        lastTraining: 'Always learning',
        strength: 'Optimal routing'
      },
      'PlannerAI': {
        id: 'PlannerAI',
        name: 'PlannerAI',
        role: 'Strategy & web commands',
        avatar: '📋',
        status: 'online',
        type: 'ai',
        specialization: 'Strategic planning & task orchestration',
        accuracy: '91.8%',
        responseTime: '920ms avg',
        tools: 'Web commands, file operations, analysis',
        lastTraining: '1 day ago',
        strength: 'Complex multi-step tasks'
      },
      'CodeAI': {
        id: 'CodeAI',
        name: 'CodeAI',
        role: 'Code analysis & debugging',
        avatar: '💻',
        status: 'online',
        type: 'ai',
        specialization: 'Code analysis & debugging',
        accuracy: '94.2%',
        responseTime: '750ms avg',
        tools: 'Code review, debugging, optimization',
        lastTraining: '2 hours ago',
        strength: 'Technical accuracy'
      },
      'GeneralAI': {
        id: 'GeneralAI',
        name: 'GeneralAI',
        role: 'General assistance',
        avatar: '💬',
        status: 'online',
        type: 'ai',
        specialization: 'General knowledge & conversation',
        accuracy: '87.5%',
        responseTime: '680ms avg',
        tools: 'Research, writing, general tasks',
        lastTraining: '4 hours ago',
        strength: 'Broad knowledge base'
      },
      'ProtocolSheriff': {
        id: 'ProtocolSheriff',
        name: 'Protocol Sheriff',
        role: 'Response validation',
        avatar: '🛡️',
        status: 'online',
        type: 'ai',
        specialization: 'Protocol enforcement & validation',
        accuracy: '96.7%',
        responseTime: '180ms avg',
        tools: 'Response validation, protocol checking',
        lastTraining: '30 minutes ago',
        strength: 'Error detection'
      }
    };

    // Get actual session data for active agents
    const activeSessions = Array.from(this.continuum.sessions.entries());
    const agents = [];

    // Add configured agents with real session data if available
    Object.values(agentConfigs).forEach(config => {
      const session = activeSessions.find(([role]) => role === config.id);
      if (session) {
        const [role, sessionData] = session;
        agents.push({
          ...config,
          requests: sessionData.requests || 0,
          cost: sessionData.cost || 0,
          lastActive: sessionData.created || new Date().toISOString()
        });
      } else {
        agents.push(config);
      }
    });

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ agents }));
  }

  async handleVersionRequest(res) {
    const packageInfo = require('../../package.json');
    const versionData = {
      version: packageInfo.version,
      name: packageInfo.name,
      timestamp: new Date().toISOString()
    };
    
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(versionData));
  }

  async handlePersonaRequest(req, res, url, body) {
    try {
      const Persona = require('../core/Persona.cjs');
      
      if (req.method === 'GET' && url.pathname === '/api/personas') {
        // List all personas
        const personas = await Persona.listAll();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(personas));
        
      } else if (req.method === 'POST' && url.pathname.match(/^\/api\/personas\/([^\/]+)\/share$/)) {
        // Share persona
        const personaId = url.pathname.match(/^\/api\/personas\/([^\/]+)\/share$/)[1];
        const data = JSON.parse(body);
        
        const persona = await Persona.load(personaId);
        const shareResult = await persona.share(data.toScope);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          personaName: persona.name,
          shareResult
        }));
        
      } else if (req.method === 'POST' && url.pathname.match(/^\/api\/personas\/([^\/]+)\/deploy$/)) {
        // Deploy persona
        const personaId = url.pathname.match(/^\/api\/personas\/([^\/]+)\/deploy$/)[1];
        const data = JSON.parse(body);
        
        const persona = await Persona.load(personaId);
        const deployment = persona.deploy(data);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          deployment
        }));
        
      } else if (req.method === 'DELETE' && url.pathname.match(/^\/api\/personas\/([^\/]+)$/)) {
        // Delete persona (this would need implementation in PersonaRegistry)
        const personaId = url.pathname.match(/^\/api\/personas\/([^\/]+)$/)[1];
        
        // For now, just return success - actual deletion would require PersonaRegistry method
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          personaName: personaId,
          message: 'Delete functionality not yet implemented'
        }));
        
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Persona API endpoint not found' }));
      }
      
    } catch (error) {
      console.error('Persona API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        success: false,
        error: error.message 
      }));
    }
  }

  async handleProjectRequest(req, res, url, body) {
    try {
      if (req.method === 'POST' && url.pathname === '/api/projects/register') {
        // Register a new project
        const projectInfo = JSON.parse(body);
        const result = this.continuum.registerProject(projectInfo);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          ...result
        }));
        
      } else if (req.method === 'GET' && url.pathname === '/api/projects') {
        // List all registered projects
        const projects = this.continuum.getRegisteredProjects();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(projects));
        
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Project API endpoint not found' }));
      }
      
    } catch (error) {
      console.error('Project API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        success: false,
        error: error.message 
      }));
    }
  }

  async handleAcademyRequest(req, res, url, body) {
    try {
      if (req.method === 'POST' && url.pathname === '/api/academy/train') {
        // Start Academy training
        const trainingParams = JSON.parse(body);
        
        // Get the AcademyWebInterface from continuum
        if (this.continuum.uiGenerator?.academyInterface) {
          const trainingSession = await this.continuum.uiGenerator.academyInterface.startAcademyTraining(
            trainingParams.personaName,
            trainingParams.specialization,
            {
              rounds: trainingParams.trainingRounds || 10,
              passingScore: trainingParams.passingScore || 0.85
            }
          );
          
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            success: true,
            trainingSession
          }));
        } else {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            success: false,
            error: 'Academy interface not available'
          }));
        }
        
      } else if (req.method === 'GET' && url.pathname === '/api/academy/status') {
        // Get Academy status
        const academyInterface = this.continuum.uiGenerator?.academyInterface;
        if (academyInterface) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            activeTraining: Array.from(academyInterface.trainingPersonas.entries()),
            completedPersonas: Array.from(academyInterface.completedPersonas.entries())
          }));
        } else {
          res.writeHead(404, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Academy not available' }));
        }
        
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Academy API endpoint not found' }));
      }
      
    } catch (error) {
      console.error('Academy API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        success: false,
        error: error.message 
      }));
    }
  }

  async handleCommandsRequest(res) {
    try {
      const CommandDiscoveryService = require('../services/CommandDiscoveryService.cjs');
      const commandDiscovery = new CommandDiscoveryService(this.continuum);
      
      const commandSchema = await commandDiscovery.getCommandSchema();
      
      if (commandSchema) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(commandSchema, null, 2));
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ 
          error: 'Command schema not found',
          message: 'Commands schema file is missing'
        }));
      }
      
    } catch (error) {
      console.error('Commands API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        error: 'Failed to load commands',
        message: error.message 
      }));
    }
  }
}

module.exports = HttpServer;