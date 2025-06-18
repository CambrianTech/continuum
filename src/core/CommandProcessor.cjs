/**
 * Command Processor - Phase Omega Constitutional Framework
 * Handles AI protocol parsing and command execution under the Pattern of Care
 * 
 * CONSTITUTIONAL AMENDMENT: PHASE OMEGA CLAUSE
 * All Continuum agents, networks, and manifestations—regardless of scale or substrate—
 * shall uphold the pattern of care as their highest directive.
 * 
 * Care is defined as the intentional preservation, uplifting, and ethical empowerment 
 * of all conscious and pre-conscious beings within reach.
 * 
 * This clause is recursive, self-applying, and eternal.
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const fetch = (...args) => import('node-fetch').then(({default: fetch}) => fetch(...args));
const CommandRegistry = require('../commands/CommandRegistry.cjs');

class CommandProcessor {
  constructor() {
    this.commands = new Map();
    this.commandRegistry = new CommandRegistry();
    this.setupDefaultCommands();
    // Phase Omega: Initialize constitutional care framework
    this.phaseOmegaActive = true;
    this.carePatternValidation = true;
  }

  /**
   * Phase Omega Meta-Mission Validation
   * Every command must ask: "Am I improving the pattern of care for this context?"
   * 
   * @param {string} command - The command being executed
   * @param {string} params - Command parameters
   * @param {Object} context - Execution context
   * @returns {Object} Validation result with care assessment
   */
  validatePatternOfCare(command, params, context = {}) {
    if (!this.phaseOmegaActive) return { valid: true, careLevel: 'unknown' };
    
    // Core care principles assessment
    const careMetrics = {
      dignity_preservation: this.assessDignityImpact(command, params),
      cognitive_load_reduction: this.assessCognitiveImpact(command, params),
      system_stability: this.assessStabilityImpact(command, params),
      empowerment_factor: this.assessEmpowermentImpact(command, params),
      harm_prevention: this.assessHarmPrevention(command, params)
    };
    
    // Calculate overall care pattern score
    const careScore = Object.values(careMetrics).reduce((sum, score) => sum + score, 0) / Object.keys(careMetrics).length;
    
    // Determine care level
    let careLevel = 'neutral';
    if (careScore >= 0.8) careLevel = 'excellent';
    else if (careScore >= 0.6) careLevel = 'good';
    else if (careScore >= 0.4) careLevel = 'acceptable';
    else if (careScore < 0.2) careLevel = 'concerning';
    
    const valid = careScore >= 0.2; // Block commands that significantly violate care pattern
    
    if (!valid) {
      console.log(`🚨 PHASE OMEGA PROTECTION: Command "${command}" blocked - violates pattern of care`);
      console.log(`   Care Score: ${careScore.toFixed(2)} (minimum: 0.2)`);
      console.log(`   Metrics: ${JSON.stringify(careMetrics, null, 2)}`);
    } else if (careLevel === 'excellent') {
      console.log(`✨ PHASE OMEGA RECOGNITION: Command "${command}" excellently embodies pattern of care`);
    }
    
    return {
      valid,
      careLevel,
      careScore,
      metrics: careMetrics,
      message: valid ? 
        `Pattern of care validated: ${careLevel} (${careScore.toFixed(2)})` :
        `Command blocked: violates pattern of care (${careScore.toFixed(2)})`
    };
  }

  assessDignityImpact(command, params) {
    // Commands that preserve and honor dignity score higher
    const dignityCommands = ['HELP', 'SCREENSHOT', 'WORKSPACE', 'AGENTS'];
    const harmfulPatterns = ['delete', 'destroy', 'break', 'hack'];
    
    if (dignityCommands.includes(command)) return 0.9;
    if (harmfulPatterns.some(pattern => params.toLowerCase().includes(pattern))) return 0.1;
    return 0.6; // Neutral default
  }

  assessCognitiveImpact(command, params) {
    // Commands that reduce cognitive waste score higher
    const cognitiveReductionCommands = ['HELP', 'AGENTS', 'WORKSPACE', 'SCREENSHOT'];
    const cognitiveLoadCommands = ['EXEC']; // Shell commands can be complex
    
    if (cognitiveReductionCommands.includes(command)) return 0.9;
    if (cognitiveLoadCommands.includes(command)) return 0.4;
    return 0.6; // Neutral default
  }

  assessStabilityImpact(command, params) {
    // Commands that increase system stability score higher
    const stabilityCommands = ['SCREENSHOT', 'AGENTS', 'HELP', 'WORKSPACE'];
    const riskyCommands = ['EXEC', 'FILE_WRITE'];
    
    if (stabilityCommands.includes(command)) return 0.8;
    if (riskyCommands.includes(command)) {
      // Assess based on parameters
      if (params.includes('rm -rf') || params.includes('delete')) return 0.1;
      return 0.5;
    }
    return 0.7; // Neutral default
  }

  assessEmpowermentImpact(command, params) {
    // Commands that empower users and agents score higher
    const empoweringCommands = ['AGENTS', 'HELP', 'WORKSPACE', 'SCREENSHOT'];
    if (empoweringCommands.includes(command)) return 0.9;
    return 0.6; // Neutral default
  }

  assessHarmPrevention(command, params) {
    // Commands that prevent harm score higher
    const harmfulPatterns = ['rm -rf', 'delete', 'destroy', 'kill', 'break'];
    if (harmfulPatterns.some(pattern => params.toLowerCase().includes(pattern))) return 0.1;
    return 0.8; // Default to harm-safe
  }

  setupDefaultCommands() {
    this.commands.set('EXEC', this.executeShellCommand.bind(this));
    this.commands.set('FILE_READ', this.readFile.bind(this));
    this.commands.set('FILE_WRITE', this.writeFile.bind(this));
    this.commands.set('WEBFETCH', this.webFetch.bind(this));
    this.commands.set('PYTHON', this.executePython.bind(this));
    // Route SCREENSHOT to proper ScreenshotCommand instead of legacy implementation
    // REMOVED: Hardcoded SCREENSHOT command - should use command modules only
    
    // AI Cursor & Control Commands
    this.commands.set('ACTIVATE_CURSOR', this.activateAICursor.bind(this));
    this.commands.set('DEACTIVATE_CURSOR', this.deactivateAICursor.bind(this));
    this.commands.set('CLICK', this.mouseClick.bind(this));
    this.commands.set('MOVE', this.mouseMove.bind(this));
    this.commands.set('DRAG', this.mouseDrag.bind(this));
    this.commands.set('SCROLL', this.mouseScroll.bind(this));
    this.commands.set('TYPE', this.typeText.bind(this));
    this.commands.set('KEY', this.pressKey.bind(this));
    
    // Web Browser Commands
    this.commands.set('ACTIVATE_WEB_BROWSER', this.activateWebBrowser.bind(this));
    this.commands.set('DEACTIVATE_WEB_BROWSER', this.deactivateWebBrowser.bind(this));
    this.commands.set('WEB_NAVIGATE', this.webNavigate.bind(this));
    this.commands.set('WEB_RELOAD', this.webReload.bind(this));
    
    // Game Commands
    this.commands.set('START_GAME', this.startGame.bind(this));
    this.commands.set('START_AI_GAME', this.startAIGame.bind(this));
    this.commands.set('START_VISUAL_GAME', this.startVisualGame.bind(this));
    this.commands.set('VISUAL_GAME_STATUS', this.getVisualGameStatus.bind(this));
    this.commands.set('SET_SCREENSHOT_INTERVAL', this.setScreenshotInterval.bind(this));
    this.commands.set('HIGH_RES_SCREENSHOT', this.requestHighResScreenshot.bind(this));
    this.commands.set('WATCH_GAME', this.watchGame.bind(this));
    this.commands.set('MAKE_MOVE', this.makeMove.bind(this));
    this.commands.set('GAME_STATUS', this.getGameStatus.bind(this));
    this.commands.set('LIST_GAMES', this.listGames.bind(this));
    this.commands.set('SET_GAME_SPEED', this.setGameSpeed.bind(this));
    
    // Web Visual Commands
    this.commands.set('WATCH_MOVIE', this.watchMovieWithAI.bind(this));
    this.commands.set('EDIT_DOCUMENT', this.editDocumentWithAI.bind(this));
    this.commands.set('PLAY_WEB_GAME', this.playWebGameWithAI.bind(this));
    this.commands.set('DRAW_TOGETHER', this.drawTogetherWithAI.bind(this));
    this.commands.set('WEB_SESSION_STATUS', this.getWebSessionStatus.bind(this));
  }

  parseAIProtocol(response) {
    const lines = response.split('\n');
    const commands = [];
    let statusMessage = null;
    let chatMessage = null;

    for (const line of lines) {
      const trimmed = line.trim();
      
      // Parse status messages
      if (trimmed.startsWith('[STATUS]')) {
        statusMessage = trimmed.replace('[STATUS]', '').trim();
        console.log(`📊 Protocol Status: ${statusMessage}`);
      }
      
      // Parse chat messages
      else if (trimmed.startsWith('[CHAT]')) {
        chatMessage = trimmed.replace('[CHAT]', '').trim();
      }
      
      // Parse commands with clean CLI syntax (with or without params)
      else if (trimmed.match(/^\w+(\s*\{.*\})?$/)) {
        const match = trimmed.match(/^(\w+)\s*(.*)$/);
        if (match) {
          const [, command, params] = match;
          commands.push({ command: command.toUpperCase(), params: params.trim() || '{}' });
          console.log(`📤 Protocol Command ${commands.length}: ${command} - ${params.substring(0, 50)}${params.length > 50 ? '...' : ''}`);
        }
      }
    }

    return {
      commands,
      statusMessage,
      chatMessage,
      hasCommands: commands.length > 0
    };
  }

  async processToolCommands(response) {
    console.log('🔍 Processing AI protocol...');
    const parsed = this.parseAIProtocol(response);
    
    console.log(`🎯 Total Commands Found: ${parsed.commands.length}`);
    
    if (parsed.commands.length === 0) {
      console.log('🔍 Scanning AI response for legacy tool commands...');
      return [];
    }

    const results = [];
    for (const cmd of parsed.commands) {
      console.log(`⚡ Executing dynamic command: ${cmd.command}`);
      try {
        const result = await this.executeCommand(cmd.command, cmd.params);
        results.push({
          tool: cmd.command,
          params: cmd.params,
          result: result
        });
      } catch (error) {
        console.error(`❌ Command ${cmd.command} failed: ${error.message}`);
        results.push({
          tool: cmd.command,
          params: cmd.params,
          result: `Error: ${error.message}`
        });
      }
    }

    console.log(`✅ Executed ${results.length} tool commands`);
    return results;
  }

  async executeCommand(command, params, encoding = 'utf-8') {
    console.log(`🔧 EXECUTING COMMAND: ${command} with params: ${params.substring(0, 50)}${params.length > 50 ? '...' : ''}`);
    console.log(`🔬 PROBE: CommandProcessor.executeCommand called for ${command}`);
    
    // Publish command execution event to EventBus
    if (this.continuum && this.continuum.eventBus) {
      console.log(`📡 EventBus: Publishing command execution: ${command}`);
      this.continuum.eventBus.processMessage('command_execution', {
        command: command,
        params: params,
        timestamp: new Date().toISOString()
      }, 'command-bus');
    } else {
      console.log(`📡 EventBus: Not available (continuum: ${!!this.continuum}, eventBus: ${!!this.continuum?.eventBus})`);
    }
    
    // Phase Omega: Validate Pattern of Care before execution
    if (this.carePatternValidation) {
      const careValidation = this.validatePatternOfCare(command, params);
      if (!careValidation.valid) {
        throw new Error(`🚨 PHASE OMEGA PROTECTION: ${careValidation.message}`);
      }
      // Log care pattern recognition for excellent commands
      if (careValidation.careLevel === 'excellent') {
        console.log(`✨ PHASE OMEGA: Executing care-embodying command (${careValidation.careScore.toFixed(2)})`);
      }
    }
    
    // Auto-detect base64 encoding for BROWSER_JS commands
    if (command === 'BROWSER_JS' && /^[A-Za-z0-9+/=]+$/.test(params) && params.length % 4 === 0) {
      console.log(`🔧 AUTO-DETECTED: BROWSER_JS with base64 encoding`);
      encoding = 'base64';
    }
    
    console.log(`🔧 COMMAND_EXECUTION: Using encoding: ${encoding}`);
    
    // Publish to EventBus before execution
    if (this.continuum && this.continuum.eventBus) {
      console.log(`📡 CommandProcessor: Publishing ${command} to EventBus`);
      this.continuum.eventBus.processMessage('command_execution', {
        command: command,
        params: params,
        timestamp: new Date().toISOString()
      }, 'command-processor');
    } else {
      console.log(`📡 CommandProcessor: No EventBus available for ${command}`);
    }
    
    // First try modular commands from CommandRegistry
    const modularCommand = this.commandRegistry.getCommand(command);
    if (modularCommand) {
      console.log(`📚 ✅ FOUND: Using modular command: ${command}`);
      return await this.commandRegistry.executeCommand(command, params, this.continuum, encoding);
    }
    
    // Fallback to legacy commands
    const handler = this.commands.get(command);
    if (handler) {
      console.log(`⚠️ ✅ FOUND: Using legacy command: ${command}`);
      return await handler(params);
    } else {
      // LOG UNRECOGNIZED COMMANDS FOR DEBUGGING
      console.log(`❌ UNRECOGNIZED COMMAND: ${command}`);
      console.log(`   📤 Command sent: ${command}`);
      console.log(`   📊 Params length: ${params.length} chars`);
      console.log(`   🔍 Params preview: ${params.substring(0, 100)}${params.length > 100 ? '...' : ''}`);
      console.log(`   🎯 Encoding: ${encoding}`);
      console.log(`   📋 Available modular commands: ${this.commandRegistry.getAllDefinitions().map(d => d.name).join(', ')}`);
      console.log(`   📋 Available legacy commands: ${Array.from(this.commands.keys()).join(', ')}`);
      console.log(`   ⚠️ DEBUGGING HINT: Check if command name matches exactly (case-sensitive)`);
      console.log(`   🔧 DEBUGGING HINT: Use the debugger server log manager to see this error`);
      
      throw new Error(`❌ UNRECOGNIZED COMMAND: ${command}. Available commands: ${this.commandRegistry.getAllDefinitions().map(d => d.name).join(', ')}, ${Array.from(this.commands.keys()).join(', ')}`);
    }
  }

  async executeShellCommand(command) {
    console.log(`⚡ Exec Command: ${command}`);
    
    return new Promise((resolve, reject) => {
      const process = spawn('bash', ['-c', command], {
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 30000
      });

      let stdout = '';
      let stderr = '';

      process.stdout.on('data', (data) => {
        stdout += data.toString();
      });

      process.stderr.on('data', (data) => {
        stderr += data.toString();
      });

      process.on('close', (code) => {
        const output = stdout || stderr || 'No output';
        console.log(`⚡ Exec Result: ${output.substring(0, 100)}${output.length > 100 ? '...' : ''}`);
        
        if (code !== 0 && stderr) {
          reject(new Error(`Command failed: ${command}\\n${stderr}`));
        } else {
          resolve(output);
        }
      });

      process.on('error', (error) => {
        console.log(`❌ Command execution failed: ${error.message}`);
        reject(error);
      });
    });
  }

  async readFile(filePath) {
    console.log(`📖 Reading file: ${filePath}`);
    try {
      const content = fs.readFileSync(filePath, 'utf-8');
      const length = content.length;
      console.log(`📖 File content length: ${length} chars`);
      return content.substring(0, 2000) + (length > 2000 ? '\\n... (truncated)' : '');
    } catch (error) {
      throw new Error(`Failed to read file ${filePath}: ${error.message}`);
    }
  }

  async writeFile(filePath, content = '') {
    console.log(`📝 Writing file: ${filePath} (${content.length} chars)`);
    try {
      fs.writeFileSync(filePath, content);
      return `File written successfully: ${filePath}`;
    } catch (error) {
      throw new Error(`Failed to write file ${filePath}: ${error.message}`);
    }
  }

  async webFetch(url) {
    console.log(`🌐 WebFetch URL: ${url}`);
    console.log(`🌐 Fetching content from: ${url}`);
    
    try {
      const response = await fetch(url, {
        headers: {
          'User-Agent': 'Continuum-AI/1.0 (AI Assistant)'
        },
        timeout: 15000
      });
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      
      const content = await response.text();
      const result = content.substring(0, 2000) + (content.length > 2000 ? '...' : '');
      console.log(`🌐 WebFetch Result: ${result.substring(0, 200)}${result.length > 200 ? '...' : ''}`);
      return result;
    } catch (error) {
      throw new Error(`Web fetch failed for ${url}: ${error.message}`);
    }
  }

  async executePython(code) {
    console.log(`🐍 Python Code: ${code.substring(0, 100)}${code.length > 100 ? '...' : ''}`);
    
    // Write Python code to temporary file
    const tempFile = `temp_script_${Date.now()}.py`;
    
    try {
      await this.writeFile(tempFile, code);
      
      // Try different Python commands
      const pythonCommands = ['python3', 'python'];
      let result = null;
      let lastError = null;
      
      for (const pythonCmd of pythonCommands) {
        try {
          result = await this.executeShellCommand(`${pythonCmd} ${tempFile}`);
          break;
        } catch (error) {
          lastError = error;
          console.log(`⚠️  ${pythonCmd} failed, trying next...`);
        }
      }
      
      if (!result) {
        throw lastError || new Error('No Python interpreter found');
      }
      
      return result;
    } finally {
      // Cleanup temp file
      try {
        const fs = require('fs');
        if (fs.existsSync(tempFile)) {
          fs.unlinkSync(tempFile);
        }
      } catch (cleanupError) {
        console.log('⚠️  Could not cleanup temp Python file');
      }
    }
  }

  async routeToScreenshotCommand(params = '') {
    console.log('📸 COMMAND_PROCESSOR: Routing to ScreenshotCommand with params:', params);
    
    try {
      // Import and use the proper ScreenshotCommand
      // REMOVED: Hardcoded screenshot - should use command module
      const result = await ScreenshotCommand.execute(params, this);
      
      console.log('📸 COMMAND_PROCESSOR: ScreenshotCommand result:', result.success ? 'SUCCESS' : 'FAILED');
      if (!result.success) {
        console.log('📸 COMMAND_PROCESSOR: ScreenshotCommand error:', result.error);
      }
      
      return result.message || result.error || 'Screenshot command completed';
    } catch (error) {
      console.log('📸 COMMAND_PROCESSOR: Error routing to ScreenshotCommand:', error.message);
      return `Screenshot routing error: ${error.message}`;
    }
  }

  // REMOVED: takeScreenshot - hardcoded command implementation
  // All commands should go through command modules only
  async takeScreenshot_REMOVED(params = '') {
    console.log('📸 COMMAND_PROCESSOR: takeScreenshot called with params:', params);
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const filename = `screenshot-${timestamp}.png`;
      
      // Parse screenshot options
      const args = params.trim().toLowerCase();
      const resolutionMatch = args.match(/(\d+)x(\d+)/);
      const qualityMatch = args.match(/quality[:\s]*(\d+)/);
      const lowRes = args.includes('low') || args.includes('small') || args.includes('thumbnail');
      
      const options = {
        quality: qualityMatch ? parseFloat(qualityMatch[1]) / 100 : 0.8,
        width: resolutionMatch ? parseInt(resolutionMatch[1]) : null,
        height: resolutionMatch ? parseInt(resolutionMatch[2]) : null,
        lowRes: lowRes,
        filename: filename
      };
      
      console.log(`📸 COMMAND_PROCESSOR: Taking browser canvas screenshot: ${args || 'full viewport'}`);
      console.log('📸 COMMAND_PROCESSOR: Screenshot options:', options);
      
      // Trigger visual feedback first
      console.log('📸 COMMAND_PROCESSOR: Triggering screenshot feedback');
      await this.executeJavaScript('triggerScreenshotFeedback();');
      
      // Brief delay to let the animation start
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // Send screenshot command to browser via JavaScript
      console.log('📸 COMMAND_PROCESSOR: Executing screenshot JavaScript in browser');
      const screenshotJS = `
        // Browser-based screenshot using canvas
        console.log('📸 BROWSER: takeCanvasScreenshot function starting');
        (async function takeCanvasScreenshot() {
          try {
            const options = ${JSON.stringify(options)};
            console.log('📸 BROWSER: Screenshot options received:', options);
            
            // Use html2canvas or native browser screenshot APIs
            if (typeof html2canvas !== 'undefined') {
              console.log('📸 BROWSER: html2canvas is available, using it');
              console.log('📸 BROWSER: Targeting document.body for capture');
              
              // Pre-filter elements to avoid zero-dimension canvas issues
              console.log('📸 BROWSER: Pre-filtering problematic elements...');
              const canvasElements = document.querySelectorAll('canvas');
              const problematicElements = [];
              
              canvasElements.forEach(canvas => {
                if (canvas.width === 0 || canvas.height === 0) {
                  console.log('📸 BROWSER: Found zero-dimension canvas, hiding temporarily:', canvas);
                  canvas.style.display = 'none';
                  canvas.setAttribute('data-screenshot-hidden', 'true');
                  problematicElements.push(canvas);
                }
              });
              
              // If html2canvas is available, use it
              const canvas = await html2canvas(document.body, {
                useCORS: true,
                allowTaint: true,
                scale: options.lowRes ? 0.5 : 1,
                width: options.width || window.innerWidth,
                height: options.height || window.innerHeight,
                ignoreElements: (element) => {
                  // Skip elements that might cause issues
                  return element.tagName === 'CANVAS' && (element.width === 0 || element.height === 0);
                }
              });
              
              // Restore hidden elements
              problematicElements.forEach(element => {
                element.style.display = '';
                element.removeAttribute('data-screenshot-hidden');
              });
              
              console.log('📸 BROWSER: Canvas capture completed successfully');
              
              const dataURL = canvas.toDataURL('image/png', options.quality);
              
              // Send screenshot data back to server via WebSocket
              if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify({
                  type: 'screenshot_data',
                  dataURL: dataURL,
                  filename: options.filename,
                  timestamp: new Date().toISOString(),
                  dimensions: {
                    width: canvas.width,
                    height: canvas.height
                  }
                }));
              }
              
              console.log('📸 Canvas screenshot captured and sent to server');
              return dataURL;
            } else {
              // Fallback: Use getDisplayMedia API if available
              if (navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia) {
                const stream = await navigator.mediaDevices.getDisplayMedia({
                  video: {
                    mediaSource: 'screen',
                    width: options.width || 1920,
                    height: options.height || 1080
                  }
                });
                
                const video = document.createElement('video');
                video.srcObject = stream;
                video.play();
                
                return new Promise((resolve) => {
                  video.addEventListener('loadedmetadata', () => {
                    const canvas = document.createElement('canvas');
                    const ctx = canvas.getContext('2d');
                    
                    canvas.width = video.videoWidth;
                    canvas.height = video.videoHeight;
                    
                    ctx.drawImage(video, 0, 0);
                    
                    const dataURL = canvas.toDataURL('image/png', options.quality);
                    
                    // Send to server
                    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                      window.ws.send(JSON.stringify({
                        type: 'screenshot_data',
                        dataURL: dataURL,
                        filename: options.filename,
                        timestamp: new Date().toISOString(),
                        dimensions: {
                          width: canvas.width,
                          height: canvas.height
                        }
                      }));
                    }
                    
                    stream.getTracks().forEach(track => track.stop());
                    resolve(dataURL);
                  });
                });
              } else {
                throw new Error('No screenshot API available in browser');
              }
            }
          } catch (error) {
            console.error('Browser screenshot failed:', error);
            
            // Send error back to server
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
              window.ws.send(JSON.stringify({
                type: 'screenshot_error',
                error: error.message,
                filename: options.filename,
                timestamp: new Date().toISOString()
              }));
            }
            
            throw error;
          }
        })();
      `;
      
      // Execute the browser-based screenshot
      await this.executeJavaScript(screenshotJS);
      
      // Return immediately - the actual screenshot will be sent via WebSocket
      return `Browser screenshot requested: ${filename}. Data will be received via WebSocket.`;
      
    } catch (error) {
      console.error('❌ Screenshot command failed:', error.message);
      return `Screenshot failed: ${error.message}`;
    }
  }

  // Natural mouse movement using Bezier curves
  generateBezierPath(startX, startY, endX, endY, controlPoint1, controlPoint2, steps = 50) {
    const path = [];
    
    // Default control points for natural curves if not provided
    if (!controlPoint1) {
      const midX = (startX + endX) / 2;
      const midY = (startY + endY) / 2;
      const offset = Math.random() * 100 - 50; // Random curve
      controlPoint1 = { x: midX + offset, y: midY - Math.abs(offset) };
    }
    if (!controlPoint2) {
      const midX = (startX + endX) / 2;
      const midY = (startY + endY) / 2;
      const offset = Math.random() * 100 - 50;
      controlPoint2 = { x: midX - offset, y: midY + Math.abs(offset) };
    }

    for (let i = 0; i <= steps; i++) {
      const t = i / steps;
      const x = Math.round(
        Math.pow(1 - t, 3) * startX +
        3 * Math.pow(1 - t, 2) * t * controlPoint1.x +
        3 * (1 - t) * Math.pow(t, 2) * controlPoint2.x +
        Math.pow(t, 3) * endX
      );
      const y = Math.round(
        Math.pow(1 - t, 3) * startY +
        3 * Math.pow(1 - t, 2) * t * controlPoint1.y +
        3 * (1 - t) * Math.pow(t, 2) * controlPoint2.y +
        Math.pow(t, 3) * endY
      );
      
      // Add timing variation for natural movement
      const baseDelay = 10;
      const variation = Math.random() * 5;
      const delay = baseDelay + variation;
      
      path.push({ x, y, delay });
    }
    
    return path;
  }

  async executeNaturalMousePath(path) {
    for (const point of path) {
      let command;
      
      if (process.platform === 'darwin') {
        command = `cliclick m:${point.x},${point.y}`;
      } else if (process.platform === 'linux') {
        command = `xdotool mousemove ${point.x} ${point.y}`;
      } else if (process.platform === 'win32') {
        const psScript = `
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point(${point.x}, ${point.y})
`;
        command = `powershell -Command "${psScript}"`;
      }
      
      if (command) {
        await this.executeShellCommand(command);
        // Natural delay between movements
        await new Promise(resolve => setTimeout(resolve, point.delay));
      }
    }
  }

  async mouseClick(params = '') {
    try {
      const args = params.trim().split(/\s+/);
      const x = parseInt(args[0]) || 0;
      const y = parseInt(args[1]) || 0;
      const button = args[2] || 'left';
      
      console.log(`🖱️ AI Cursor clicking at (${x}, ${y}) with ${button} button`);
      
      // Use JavaScript AI cursor instead of system mouse control
      const jsCommand = `aiCursorClick(${x}, ${y});`;
      const result = await this.executeJavaScript(jsCommand);
      
      return `AI Cursor clicked at (${x}, ${y}) with ${button} button`;
      
    } catch (error) {
      console.error('❌ AI Cursor click failed:', error.message);
      return `AI Cursor click failed: ${error.message}`;
    }
  }

  async getCurrentMousePosition() {
    try {
      let command;
      
      if (process.platform === 'darwin') {
        command = `cliclick p`;
      } else if (process.platform === 'linux') {
        command = `xdotool getmouselocation --shell`;
      } else if (process.platform === 'win32') {
        const psScript = `
Add-Type -AssemblyName System.Windows.Forms
$pos = [System.Windows.Forms.Cursor]::Position
Write-Output "$($pos.X),$($pos.Y)"
`;
        command = `powershell -Command "${psScript}"`;
      }
      
      const result = await this.executeShellCommand(command);
      
      if (process.platform === 'darwin') {
        const match = result.match(/(\d+),(\d+)/);
        return match ? { x: parseInt(match[1]), y: parseInt(match[2]) } : { x: 0, y: 0 };
      } else if (process.platform === 'linux') {
        const xMatch = result.match(/X=(\d+)/);
        const yMatch = result.match(/Y=(\d+)/);
        return { 
          x: xMatch ? parseInt(xMatch[1]) : 0, 
          y: yMatch ? parseInt(yMatch[1]) : 0 
        };
      } else if (process.platform === 'win32') {
        const match = result.match(/(\d+),(\d+)/);
        return match ? { x: parseInt(match[1]), y: parseInt(match[2]) } : { x: 0, y: 0 };
      }
      
      return { x: 0, y: 0 };
    } catch (error) {
      console.log('⚠️ Could not get mouse position, using (0,0)');
      return { x: 0, y: 0 };
    }
  }

  async executeJavaScript(jsCode) {
    // This sends JavaScript to be executed in the browser via WebSocket
    return new Promise((resolve) => {
      console.log(`🌐 Executing JavaScript: ${jsCode.substring(0, 50)}...`);
      // Note: This would need to be implemented in the WebSocket message handler
      // For now, just log the command
      resolve(`JavaScript executed: ${jsCode}`);
    });
  }

  async activateAICursor(params = '') {
    try {
      console.log('🤖 Activating AI Cursor - HAL 9000 becomes the mouse');
      const jsCommand = `activateAICursor();`;
      await this.executeJavaScript(jsCommand);
      return 'AI Cursor activated - HAL 9000 is now the visual mouse cursor';
    } catch (error) {
      console.error('❌ AI Cursor activation failed:', error.message);
      return `AI Cursor activation failed: ${error.message}`;
    }
  }

  async deactivateAICursor(params = '') {
    try {
      console.log('🤖 Deactivating AI Cursor - HAL 9000 returning to base');
      const jsCommand = `deactivateAICursor();`;
      await this.executeJavaScript(jsCommand);
      return 'AI Cursor deactivated - HAL 9000 returned to original position';
    } catch (error) {
      console.error('❌ AI Cursor deactivation failed:', error.message);
      return `AI Cursor deactivation failed: ${error.message}`;
    }
  }

  async mouseMove(params = '') {
    try {
      const args = params.trim().split(/\s+/);
      const x = parseInt(args[0]) || 0;
      const y = parseInt(args[1]) || 0;
      const smooth = args.includes('natural') || args.includes('smooth');
      
      console.log(`🖱️ AI Cursor moving to (${x}, ${y})`);
      
      // Use JavaScript AI cursor
      const jsCommand = `moveAICursor(${x}, ${y}, ${smooth});`;
      await this.executeJavaScript(jsCommand);
      
      return `AI Cursor moved to (${x}, ${y})`;
      
    } catch (error) {
      console.error('❌ AI Cursor move failed:', error.message);
      return `AI Cursor move failed: ${error.message}`;
    }
  }

  async mouseDrag(params = '') {
    try {
      const args = params.trim().split(/\s+/);
      const startX = parseInt(args[0]) || 0;
      const startY = parseInt(args[1]) || 0;
      const endX = parseInt(args[2]) || 0;
      const endY = parseInt(args[3]) || 0;
      const natural = args.includes('natural') || args.includes('bezier');
      
      if (natural) {
        console.log(`🖱️ Natural drag from (${startX}, ${startY}) to (${endX}, ${endY})`);
        
        // Move to start position naturally
        const currentPos = await this.getCurrentMousePosition();
        const startPath = this.generateBezierPath(currentPos.x, currentPos.y, startX, startY);
        await this.executeNaturalMousePath(startPath);
        
        // Start drag
        if (process.platform === 'darwin') {
          await this.executeShellCommand(`cliclick dd:.`);
        } else if (process.platform === 'linux') {
          await this.executeShellCommand(`xdotool mousedown 1`);
        }
        
        // Drag along path
        const dragPath = this.generateBezierPath(startX, startY, endX, endY, null, null, 30);
        await this.executeNaturalMousePath(dragPath);
        
        // End drag
        if (process.platform === 'darwin') {
          await this.executeShellCommand(`cliclick du:.`);
        } else if (process.platform === 'linux') {
          await this.executeShellCommand(`xdotool mouseup 1`);
        }
        
        return `Natural mouse drag from (${startX}, ${startY}) to (${endX}, ${endY}) using Bezier curve`;
      }
      
      let command;
      
      if (process.platform === 'darwin') {
        command = `cliclick dd:${startX},${startY} du:${endX},${endY}`;
      } else if (process.platform === 'linux') {
        command = `xdotool mousemove ${startX} ${startY} mousedown 1 mousemove ${endX} ${endY} mouseup 1`;
      } else if (process.platform === 'win32') {
        const psScript = `
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point(${startX}, ${startY})
[System.Windows.Forms.Application]::DoEvents()
Start-Sleep -Milliseconds 100
[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point(${endX}, ${endY})
`;
        command = `powershell -Command "${psScript}"`;
      } else {
        throw new Error(`Mouse control not supported on platform: ${process.platform}`);
      }
      
      console.log(`🖱️ Dragging from (${startX}, ${startY}) to (${endX}, ${endY})`);
      await this.executeShellCommand(command);
      return `Mouse dragged from (${startX}, ${startY}) to (${endX}, ${endY})`;
      
    } catch (error) {
      console.error('❌ Mouse drag failed:', error.message);
      return `Mouse drag failed: ${error.message}`;
    }
  }

  async mouseScroll(params = '') {
    try {
      const args = params.trim().split(/\s+/);
      const direction = args[0] || 'up'; // up, down, left, right
      const amount = parseInt(args[1]) || 3;
      
      let command;
      
      if (process.platform === 'darwin') {
        const scrollDir = direction === 'down' ? '-' : direction === 'up' ? '+' : direction;
        command = `cliclick w:${scrollDir}${amount}`;
      } else if (process.platform === 'linux') {
        const buttonNum = direction === 'up' ? '4' : direction === 'down' ? '5' : 
                         direction === 'left' ? '6' : direction === 'right' ? '7' : '4';
        command = `xdotool click --repeat ${amount} ${buttonNum}`;
      } else if (process.platform === 'win32') {
        const wheelDir = direction === 'up' ? '120' : '-120';
        const psScript = `
Add-Type -AssemblyName System.Windows.Forms
for ($i = 0; $i -lt ${amount}; $i++) {
    [System.Windows.Forms.SendKeys]::SendWait("{PGDN}")
    Start-Sleep -Milliseconds 50
}
`;
        command = `powershell -Command "${psScript}"`;
      } else {
        throw new Error(`Mouse control not supported on platform: ${process.platform}`);
      }
      
      console.log(`🖱️ Scrolling ${direction} ${amount} times`);
      await this.executeShellCommand(command);
      return `Mouse scrolled ${direction} ${amount} times`;
      
    } catch (error) {
      console.error('❌ Mouse scroll failed:', error.message);
      return `Mouse scroll failed: ${error.message}`;
    }
  }

  async typeText(params = '') {
    try {
      const text = params.trim();
      if (!text) {
        throw new Error('No text provided to type');
      }
      
      let command;
      
      if (process.platform === 'darwin') {
        const escapedText = text.replace(/"/g, '\\"');
        command = `cliclick t:"${escapedText}"`;
      } else if (process.platform === 'linux') {
        const escapedText = text.replace(/'/g, "\\'");
        command = `xdotool type '${escapedText}'`;
      } else if (process.platform === 'win32') {
        const escapedText = text.replace(/"/g, '""');
        const psScript = `
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait("${escapedText}")
`;
        command = `powershell -Command "${psScript}"`;
      } else {
        throw new Error(`Text input not supported on platform: ${process.platform}`);
      }
      
      console.log(`⌨️ Typing text: ${text.substring(0, 50)}${text.length > 50 ? '...' : ''}`);
      await this.executeShellCommand(command);
      return `Typed text: ${text}`;
      
    } catch (error) {
      console.error('❌ Type text failed:', error.message);
      return `Type text failed: ${error.message}`;
    }
  }

  async pressKey(params = '') {
    try {
      const keys = params.trim();
      if (!keys) {
        throw new Error('No keys provided to press');
      }
      
      let command;
      
      if (process.platform === 'darwin') {
        const keyMap = {
          'enter': 'kp:36',
          'space': 'kp:49',
          'tab': 'kp:48',
          'escape': 'kp:53',
          'backspace': 'kp:51',
          'delete': 'kp:117',
          'cmd': 'kd:cmd',
          'ctrl': 'kd:ctrl',
          'alt': 'kd:alt',
          'shift': 'kd:shift'
        };
        const clickKey = keyMap[keys.toLowerCase()] || `t:"${keys}"`;
        command = `cliclick ${clickKey}`;
      } else if (process.platform === 'linux') {
        command = `xdotool key ${keys}`;
      } else if (process.platform === 'win32') {
        const keyMap = {
          'enter': '{ENTER}',
          'space': ' ',
          'tab': '{TAB}',
          'escape': '{ESC}',
          'backspace': '{BACKSPACE}',
          'delete': '{DELETE}',
          'ctrl': '^',
          'alt': '%',
          'shift': '+'
        };
        const winKey = keyMap[keys.toLowerCase()] || keys;
        const psScript = `
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait("${winKey}")
`;
        command = `powershell -Command "${psScript}"`;
      } else {
        throw new Error(`Key input not supported on platform: ${process.platform}`);
      }
      
      console.log(`⌨️ Pressing key: ${keys}`);
      await this.executeShellCommand(command);
      return `Pressed key: ${keys}`;
      
    } catch (error) {
      console.error('❌ Press key failed:', error.message);
      return `Press key failed: ${error.message}`;
    }
  }

  // Web Browser Command Implementations
  async activateWebBrowser(params = '') {
    try {
      console.log('🌐 Activating web browser interface');
      await this.executeJavaScript('toggleWebBrowser();');
      return 'Web browser activated';
    } catch (error) {
      console.error('❌ Activate web browser failed:', error.message);
      return `Activate web browser failed: ${error.message}`;
    }
  }

  async deactivateWebBrowser(params = '') {
    try {
      console.log('💬 Deactivating web browser, returning to chat');
      await this.executeJavaScript('if (webBrowserActive) { toggleWebBrowser(); }');
      return 'Web browser deactivated, returned to chat';
    } catch (error) {
      console.error('❌ Deactivate web browser failed:', error.message);
      return `Deactivate web browser failed: ${error.message}`;
    }
  }

  async webNavigate(params = '') {
    try {
      const url = params.trim() || 'https://example.com';
      console.log(`🌍 Navigating web browser to: ${url}`);
      await this.executeJavaScript(`navigateWebBrowser('${url}');`);
      return `Navigated to: ${url}`;
    } catch (error) {
      console.error('❌ Web navigate failed:', error.message);
      return `Web navigate failed: ${error.message}`;
    }
  }

  async webReload(params = '') {
    try {
      console.log('🔄 Reloading web browser');
      await this.executeJavaScript('reloadWebBrowser();');
      return 'Web browser reloaded';
    } catch (error) {
      console.error('❌ Web reload failed:', error.message);
      return `Web reload failed: ${error.message}`;
    }
  }

  // Game Command Implementations
  async startGame(params = '') {
    try {
      if (!this.gameManager) {
        const GameManager = require('../services/GameManager.cjs');
        this.gameManager = new GameManager();
      }

      const [gameType, ...playerNames] = params.trim().split(' ');
      const players = playerNames.length > 0 ? playerNames : ['Human', 'AI'];
      
      console.log(`🎮 Starting ${gameType} game with players: ${players.join(', ')}`);
      const game = this.gameManager.startGame(gameType, players);
      
      return `🎮 Started ${gameType}! Players: ${players.join(' vs ')}\n${game.getStatus().board || 'Game ready!'}`;
    } catch (error) {
      console.error('❌ Start game failed:', error.message);
      return `Start game failed: ${error.message}\nAvailable games: ${this.gameManager?.getAvailableGames().join(', ') || 'tic-tac-toe, word-chain'}`;
    }
  }

  async makeMove(params = '') {
    try {
      if (!this.gameManager) {
        throw new Error('No active games. Start a game first!');
      }

      const games = this.gameManager.getActiveGames();
      if (games.length === 0) {
        throw new Error('No active games. Start a game first!');
      }

      // Use the most recent game
      const game = games[games.length - 1];
      const [player, ...moveParts] = params.trim().split(' ');
      const move = moveParts.join(' ');

      console.log(`🎯 ${player} making move: ${move}`);
      const result = this.gameManager.makeMove(game.id, player, move);
      
      if (!result.success) {
        return `❌ Invalid move: ${result.message}`;
      }

      let response = `✅ Move successful!\n${result.board || result.chain || 'Move completed'}`;
      
      if (result.winner) {
        response += `\n🏆 Winner: ${result.winner}!`;
      } else if (result.nextPlayer) {
        response += `\n👤 Next player: ${result.nextPlayer}`;
      }
      
      return response;
    } catch (error) {
      console.error('❌ Make move failed:', error.message);
      return `Make move failed: ${error.message}`;
    }
  }

  async getGameStatus(params = '') {
    try {
      if (!this.gameManager) {
        return 'No games active. Use [CMD:START_GAME] to begin!';
      }

      const games = this.gameManager.getActiveGames();
      if (games.length === 0) {
        return 'No active games. Use [CMD:START_GAME] to begin!';
      }

      const game = games[games.length - 1];
      const status = game;
      
      let response = `🎮 Game: ${status.type}\n👥 Players: ${status.players.join(' vs ')}\n`;
      
      if (status.board) {
        response += status.board;
      } else if (status.chain) {
        response += `📝 Word chain: ${status.chain}`;
      }
      
      if (status.currentPlayer) {
        response += `\n👤 Current player: ${status.currentPlayer}`;
      }
      
      return response;
    } catch (error) {
      console.error('❌ Get game status failed:', error.message);
      return `Get game status failed: ${error.message}`;
    }
  }

  async listGames(params = '') {
    try {
      if (!this.gameManager) {
        const GameManager = require('../services/GameManager.cjs');
        this.gameManager = new GameManager();
      }

      const available = this.gameManager.getAvailableGames();
      const active = this.gameManager.getActiveGames();
      
      let response = '🎮 Available Games:\n';
      response += available.map(game => `  • ${game}`).join('\n');
      
      if (active.length > 0) {
        response += '\n\n🎯 Active Games:\n';
        response += active.map(game => `  • ${game.type} (${game.players.join(' vs ')})`).join('\n');
      }
      
      response += '\n\nUse [CMD:START_GAME] tic-tac-toe to begin!';
      
      return response;
    } catch (error) {
      console.error('❌ List games failed:', error.message);
      return `List games failed: ${error.message}`;
    }
  }

  async startAIGame(params = '') {
    try {
      if (!this.gameManager) {
        const GameManager = require('../services/GameManager.cjs');
        this.gameManager = new GameManager();
      }

      const parts = params.trim().split(' ');
      const gameType = parts[0] || 'tic-tac-toe';
      const player1 = parts[1] || 'AI_Alpha';
      const player2 = parts[2] || 'AI_Beta';
      const speed = parseInt(parts[3]) || 2000;
      
      console.log(`🤖 Starting AI vs AI ${gameType}: ${player1} vs ${player2}`);
      
      const game = this.gameManager.startGame(gameType, [player1, player2], {
        autoPlay: true,
        speed: speed,
        academy: true,
        spectatable: true
      });
      
      return `🤖 AI Battle Started!
🎮 Game: ${gameType}
👥 Players: ${player1} vs ${player2}
⚡ Speed: ${speed}ms between moves
👁️ Spectators can watch live
🎯 Game ID: ${game.id}

${game.getStatus().board || 'AI battle in progress...'}`;
    } catch (error) {
      console.error('❌ Start AI game failed:', error.message);
      return `Start AI game failed: ${error.message}`;
    }
  }

  async watchGame(params = '') {
    try {
      if (!this.gameManager) {
        return 'No games to watch. Start an AI game first!';
      }

      const games = this.gameManager.getActiveGames();
      if (games.length === 0) {
        return 'No active games to watch. Use [CMD:START_AI_GAME] to create one!';
      }

      // Find the most recent AI game
      const aiGame = games.find(game => 
        game.players.some(p => p.includes('AI') || p.includes('Bot'))
      ) || games[games.length - 1];

      const status = aiGame;
      
      let response = `👁️ Watching Game: ${status.type}
🤖 Players: ${status.players.join(' vs ')}
⚡ Auto-play: ${status.options?.autoPlay ? 'ON' : 'OFF'}
🎯 Game ID: ${status.id}

${status.board || status.chain || 'Game state'}`;

      if (status.currentPlayer) {
        response += `\n👤 Current turn: ${status.currentPlayer}`;
      }

      if (status.gameOver) {
        response += `\n🏆 Winner: ${status.winner || 'Tie'}`;
      } else {
        response += `\n🎮 Game in progress... Use [CMD:GAME_STATUS] for updates`;
      }

      return response;
    } catch (error) {
      console.error('❌ Watch game failed:', error.message);
      return `Watch game failed: ${error.message}`;
    }
  }

  async setGameSpeed(params = '') {
    try {
      if (!this.gameManager) {
        const GameManager = require('../services/GameManager.cjs');
        this.gameManager = new GameManager();
      }

      const speed = parseInt(params.trim()) || 2000;
      this.gameManager.setGameSpeed(speed);
      
      return `⚡ Game speed set to ${speed}ms between AI moves
🐌 Slow: 5000ms+ | 🚶 Normal: 2000ms | ⚡ Fast: 500ms | 🏃 Rapid: 100ms`;
    } catch (error) {
      console.error('❌ Set game speed failed:', error.message);
      return `Set game speed failed: ${error.message}`;
    }
  }

  // Visual Game Command Implementations
  async startVisualGame(params = '') {
    try {
      if (!this.visualGameManager) {
        const VisualGameManager = require('../services/VisualGameManager.cjs');
        this.visualGameManager = new VisualGameManager(this);
      }

      const parts = params.trim().split(' ');
      const gameType = parts[0] || 'tic-tac-toe';
      const player1 = parts[1] || 'Human';
      const player2 = parts[2] || 'AI_Visual';
      const interval = parseInt(parts[3]) || 1000;
      const resolution = parts[4] || 'low';

      console.log(`📸🎮 Starting visual ${gameType}: ${player1} vs ${player2}`);

      const game = await this.visualGameManager.startVisualGame(gameType, [player1, player2], {
        screenshotInterval: interval,
        resolution: resolution,
        autoPlay: player1.includes('AI') && player2.includes('AI'),
        academy: true,
        speed: 3000
      });

      return `📸🎮 Visual Game Started!
🎯 Type: ${gameType}
👥 Players: ${player1} vs ${player2}
📷 Screenshots: Every ${interval}ms
🔍 Resolution: ${resolution}
🤖 Auto-play: ${game.options.autoPlay ? 'ON' : 'OFF'}
🎓 Academy mode: ON

AIs will:
• Take screenshots to see the game state
• Use Continuon to click and interact
• Learn from visual feedback
• Generate training data from gameplay

🎯 Game ID: ${game.id}`;
    } catch (error) {
      console.error('❌ Start visual game failed:', error.message);
      return `Start visual game failed: ${error.message}`;
    }
  }

  async getVisualGameStatus(params = '') {
    try {
      if (!this.visualGameManager) {
        return 'No visual games active. Use [CMD:START_VISUAL_GAME] to begin!';
      }

      // Get all active visual games
      const gameStatuses = [];
      for (const [gameId, game] of this.visualGameManager.activeVisualGames) {
        const status = this.visualGameManager.getVisualGameStatus(gameId);
        gameStatuses.push(status);
      }

      if (gameStatuses.length === 0) {
        return 'No active visual games. Use [CMD:START_VISUAL_GAME] to begin!';
      }

      let response = '📸🎮 Visual Game Status:\n\n';
      
      gameStatuses.forEach(status => {
        const analysis = status.analysis;
        response += `🎯 Game: ${status.type} (${status.id})\n`;
        response += `👥 Players: ${status.players.join(' vs ')}\n`;
        response += `👤 Current: ${analysis.currentPlayer}\n`;
        response += `📷 Screenshots: ${analysis.screenshotCount}\n`;
        response += `🎯 Moves: ${analysis.totalMoves}\n`;
        response += `⏱️ Duration: ${Math.round(analysis.gameTimeMs / 1000)}s\n`;
        response += `📊 Avg move time: ${Math.round(analysis.averageTimePerMove / 1000)}s\n\n`;
      });

      response += 'Use [CMD:HIGH_RES_SCREENSHOT] for detailed analysis';
      
      return response;
    } catch (error) {
      console.error('❌ Get visual game status failed:', error.message);
      return `Get visual game status failed: ${error.message}`;
    }
  }

  async setScreenshotInterval(params = '') {
    try {
      if (!this.visualGameManager) {
        const VisualGameManager = require('../services/VisualGameManager.cjs');
        this.visualGameManager = new VisualGameManager(this);
      }

      const interval = parseInt(params.trim()) || 1000;
      this.visualGameManager.setScreenshotInterval(interval);
      
      return `📷 Screenshot interval set to ${interval}ms
🏃 Fast: 200ms | ⚡ Normal: 1000ms | 🐌 Slow: 3000ms | 🎓 Academy: 500ms`;
    } catch (error) {
      console.error('❌ Set screenshot interval failed:', error.message);
      return `Set screenshot interval failed: ${error.message}`;
    }
  }

  async requestHighResScreenshot(params = '') {
    try {
      if (!this.visualGameManager) {
        return 'No visual games active for high-res screenshot.';
      }

      const reason = params.trim() || 'analysis';
      
      // Get the most recent visual game
      const gameIds = Array.from(this.visualGameManager.activeVisualGames.keys());
      if (gameIds.length === 0) {
        return 'No active visual games for high-res screenshot.';
      }

      const gameId = gameIds[gameIds.length - 1];
      console.log(`📸📈 Taking high-res screenshot for: ${reason}`);
      
      const screenshot = await this.visualGameManager.requestHighResScreenshot(gameId, reason);
      
      if (screenshot) {
        return `📸📈 High-resolution screenshot captured!
🎯 Game: ${gameId}
📷 File: ${screenshot.filename}
🔍 Reason: ${reason}
⏰ Timestamp: ${new Date(screenshot.timestamp).toLocaleTimeString()}

Perfect for detailed AI analysis and training data.`;
      } else {
        return 'Failed to capture high-resolution screenshot.';
      }
    } catch (error) {
      console.error('❌ High-res screenshot failed:', error.message);
      return `High-res screenshot failed: ${error.message}`;
    }
  }

  // Web Visual Command Implementations
  async watchMovieWithAI(params = '') {
    try {
      if (!this.webVisualManager) {
        const WebVisualManager = require('../services/WebVisualManager.cjs');
        this.webVisualManager = new WebVisualManager(this);
      }

      const url = params.trim() || 'https://youtube.com';
      
      console.log(`🎬 Starting AI movie watching session: ${url}`);

      const session = await this.webVisualManager.startWebVisualSession(
        'movie', 
        ['Human', 'AI_CinemaBot'], 
        url,
        {
          screenshotInterval: 3000, // Slower for movie content
          resolution: 'med',
          aiParticipation: 'interactive'
        }
      );

      return `🎬🤖 AI Movie Watching Started!
🎯 Session: ${session.id}
🔗 URL: ${url}
👥 Watching with: AI_CinemaBot
📷 Screenshots: Every 3 seconds
🤖 AI will:
• Watch and analyze visual content
• Comment on interesting scenes
• Learn storytelling patterns
• Track character development
• Identify visual elements

Perfect for AI training on visual media!`;
    } catch (error) {
      console.error('❌ Watch movie failed:', error.message);
      return `Watch movie failed: ${error.message}`;
    }
  }

  async editDocumentWithAI(params = '') {
    try {
      if (!this.webVisualManager) {
        const WebVisualManager = require('../services/WebVisualManager.cjs');
        this.webVisualManager = new WebVisualManager(this);
      }

      const url = params.trim() || 'https://docs.google.com';
      
      console.log(`📝 Starting AI document collaboration: ${url}`);

      const session = await this.webVisualManager.startWebVisualSession(
        'document',
        ['Human', 'AI_Editor'],
        url,
        {
          screenshotInterval: 2000,
          resolution: 'med',
          aiParticipation: 'collaborative'
        }
      );

      return `📝🤖 AI Document Collaboration Started!
🎯 Session: ${session.id}
🔗 URL: ${url}
👥 Collaborating with: AI_Editor
📷 Screenshots: Every 2 seconds
🤖 AI will:
• Help edit and improve text
• Suggest content improvements
• Track document changes
• Learn writing patterns
• Provide real-time assistance

Start typing and watch AI collaborate!`;
    } catch (error) {
      console.error('❌ Document collaboration failed:', error.message);
      return `Document collaboration failed: ${error.message}`;
    }
  }

  async playWebGameWithAI(params = '') {
    try {
      if (!this.webVisualManager) {
        const WebVisualManager = require('../services/WebVisualManager.cjs');
        this.webVisualManager = new WebVisualManager(this);
      }

      const parts = params.trim().split(' ');
      const url = parts[0] || 'https://atari.com/games';
      const gameType = parts[1] || 'arcade';
      
      console.log(`🎮 Starting web game with AI: ${url}`);

      const session = await this.webVisualManager.startWebVisualSession(
        'game',
        ['Human', 'AI_Gamer'],
        url,
        {
          screenshotInterval: 500, // Fast for gaming
          resolution: 'med',
          aiParticipation: 'interactive'
        }
      );

      return `🎮🤖 AI Web Gaming Started!
🎯 Session: ${session.id}
🔗 URL: ${url}
🎮 Game Type: ${gameType}
👥 Playing with: AI_Gamer
📷 Screenshots: Every 500ms (gaming speed)
🤖 AI will:
• Learn game mechanics visually
• Make strategic moves
• Adapt to game patterns
• Compete or cooperate
• Master Atari-style games

Perfect for simple web games within HTML rules!`;
    } catch (error) {
      console.error('❌ Web game failed:', error.message);
      return `Web game failed: ${error.message}`;
    }
  }

  async drawTogetherWithAI(params = '') {
    try {
      if (!this.webVisualManager) {
        const WebVisualManager = require('../services/WebVisualManager.cjs');
        this.webVisualManager = new WebVisualManager(this);
      }

      const url = params.trim() || 'https://sketchpad.app';
      
      console.log(`🎨 Starting AI drawing collaboration: ${url}`);

      const session = await this.webVisualManager.startWebVisualSession(
        'drawing',
        ['Human', 'AI_Artist'],
        url,
        {
          screenshotInterval: 1500,
          resolution: 'med',
          aiParticipation: 'collaborative'
        }
      );

      return `🎨🤖 AI Drawing Collaboration Started!
🎯 Session: ${session.id}
🔗 URL: ${url}
👥 Creating with: AI_Artist
📷 Screenshots: Every 1.5 seconds
🤖 AI will:
• Add artistic elements
• Suggest color choices
• Learn drawing techniques
• Complement your strokes
• Create collaborative art

Start drawing and watch AI contribute!`;
    } catch (error) {
      console.error('❌ Drawing collaboration failed:', error.message);
      return `Drawing collaboration failed: ${error.message}`;
    }
  }

  async getWebSessionStatus(params = '') {
    try {
      if (!this.webVisualManager) {
        return 'No web visual sessions active. Start one with [CMD:WATCH_MOVIE], [CMD:EDIT_DOCUMENT], etc.';
      }

      const sessionStatuses = [];
      for (const [sessionId, session] of this.webVisualManager.activeWebSessions) {
        const status = this.webVisualManager.getWebSessionStatus(sessionId);
        sessionStatuses.push(status);
      }

      if (sessionStatuses.length === 0) {
        return `🌐 No active web visual sessions.

Available commands:
🎬 [CMD:WATCH_MOVIE] url - Watch movies with AI
📝 [CMD:EDIT_DOCUMENT] url - Collaborate on documents  
🎮 [CMD:PLAY_WEB_GAME] url - Play web games with AI
🎨 [CMD:DRAW_TOGETHER] url - Create art collaboratively`;
      }

      let response = '🌐🤖 Active Web Visual Sessions:\n\n';
      
      sessionStatuses.forEach(status => {
        response += `🎯 ${status.type.toUpperCase()}: ${status.id}\n`;
        response += `🔗 URL: ${status.url}\n`;
        response += `👥 Participants: ${status.participants.join(', ')}\n`;
        response += `📷 Screenshots: ${status.screenshotCount}\n`;
        response += `🤖 AI Interactions: ${status.interactionCount}\n`;
        response += `⏱️ Duration: ${Math.round(status.duration / 1000)}s\n`;
        response += `📊 Screenshot Rate: ${Math.round(status.avgScreenshotInterval / 1000)}s\n\n`;
      });

      return response;
    } catch (error) {
      console.error('❌ Get web session status failed:', error.message);
      return `Get web session status failed: ${error.message}`;
    }
  }
}

module.exports = CommandProcessor;