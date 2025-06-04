/**
 * Command Processor
 * Handles AI protocol parsing and command execution
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const fetch = (...args) => import('node-fetch').then(({default: fetch}) => fetch(...args));

class CommandProcessor {
  constructor() {
    this.commands = new Map();
    this.setupDefaultCommands();
  }

  setupDefaultCommands() {
    this.commands.set('EXEC', this.executeShellCommand.bind(this));
    this.commands.set('FILE_READ', this.readFile.bind(this));
    this.commands.set('FILE_WRITE', this.writeFile.bind(this));
    this.commands.set('WEBFETCH', this.webFetch.bind(this));
    this.commands.set('PYTHON', this.executePython.bind(this));
    this.commands.set('SCREENSHOT', this.takeScreenshot.bind(this));
    
    // AI Cursor & Control Commands
    this.commands.set('ACTIVATE_CURSOR', this.activateAICursor.bind(this));
    this.commands.set('DEACTIVATE_CURSOR', this.deactivateAICursor.bind(this));
    this.commands.set('CLICK', this.mouseClick.bind(this));
    this.commands.set('MOVE', this.mouseMove.bind(this));
    this.commands.set('DRAG', this.mouseDrag.bind(this));
    this.commands.set('SCROLL', this.mouseScroll.bind(this));
    this.commands.set('TYPE', this.typeText.bind(this));
    this.commands.set('KEY', this.pressKey.bind(this));
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
      
      // Parse commands
      else if (trimmed.startsWith('[CMD:')) {
        const match = trimmed.match(/\[CMD:(\w+)\]\s*(.+)/);
        if (match) {
          const [, command, params] = match;
          commands.push({ command, params: params.trim() });
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

  async executeCommand(command, params) {
    console.log(`🔧 EXECUTING COMMAND: ${command} with params: ${params.substring(0, 50)}${params.length > 50 ? '...' : ''}`);
    
    const handler = this.commands.get(command);
    if (handler) {
      return await handler(params);
    } else {
      throw new Error(`Unknown command: ${command}`);
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

  async takeScreenshot(params = '') {
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const filename = `screenshot-${timestamp}.png`;
      const outputPath = path.join(process.cwd(), '.continuum', filename);
      
      // Ensure .continuum directory exists
      const continuumDir = path.join(process.cwd(), '.continuum');
      if (!fs.existsSync(continuumDir)) {
        fs.mkdirSync(continuumDir, { recursive: true });
      }
      
      let command;
      const args = params.trim().toLowerCase();
      
      // Parse resolution and quality options
      const resolutionMatch = args.match(/(\d+)x(\d+)/);
      const qualityMatch = args.match(/quality[:\s]*(\d+)/);
      const region = args.includes('region') || args.includes('select');
      const window = args.includes('window');
      const lowRes = args.includes('low') || args.includes('small') || args.includes('thumbnail');
      
      if (process.platform === 'darwin') {
        // macOS screencapture with cursor visible and resolution options
        if (region) {
          command = `screencapture -C -i "${outputPath}"`;
        } else if (window) {
          command = `screencapture -C -w "${outputPath}"`;
        } else {
          command = `screencapture -C "${outputPath}"`;
        }
        
        // Add resolution scaling if requested
        if (resolutionMatch || lowRes) {
          const width = resolutionMatch ? resolutionMatch[1] : '800';
          const height = resolutionMatch ? resolutionMatch[2] : '600';
          const tempPath = outputPath.replace('.png', '-temp.png');
          command = `${command.replace(outputPath, tempPath)} && sips -Z ${Math.max(width, height)} "${tempPath}" --out "${outputPath}" && rm "${tempPath}"`;
        }
        
      } else if (process.platform === 'linux') {
        // Linux with resolution options
        if (region) {
          command = `gnome-screenshot -a -f "${outputPath}" || import "${outputPath}"`;
        } else if (window) {
          command = `gnome-screenshot -w -f "${outputPath}" || import -window root "${outputPath}"`;
        } else {
          command = `gnome-screenshot -f "${outputPath}" || import -window root "${outputPath}"`;
        }
        
        // Add ImageMagick resize if requested
        if (resolutionMatch || lowRes) {
          const width = resolutionMatch ? resolutionMatch[1] : '800';
          const height = resolutionMatch ? resolutionMatch[2] : '600';
          command += ` && convert "${outputPath}" -resize ${width}x${height} "${outputPath}"`;
        }
        
      } else if (process.platform === 'win32') {
        // Windows using PowerShell
        const psCommand = `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('%{PRTSC}')`;
        command = `powershell -Command "${psCommand}"`;
      } else {
        throw new Error(`Screenshots not supported on platform: ${process.platform}`);
      }
      
      const resInfo = resolutionMatch ? `${resolutionMatch[1]}x${resolutionMatch[2]}` : 
                     lowRes ? 'low-res' : 'full-res';
      console.log(`📸 Taking ${resInfo} screenshot: ${args || 'full screen'}`);
      
      // Trigger AI cursor screenshot effect
      await this.executeJavaScript('aiCursorScreenshot();');
      
      await this.executeShellCommand(command);
      
      // Verify screenshot was created
      if (fs.existsSync(outputPath)) {
        const stats = fs.statSync(outputPath);
        const fileSizeKB = Math.round(stats.size / 1024);
        console.log(`✅ Screenshot saved: ${filename} (${fileSizeKB}KB, ${resInfo})`);
        return `Screenshot saved: ${outputPath} (${fileSizeKB}KB, ${resInfo})`;
      } else {
        throw new Error('Screenshot file was not created');
      }
      
    } catch (error) {
      console.error('❌ Screenshot failed:', error.message);
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
}

module.exports = CommandProcessor;