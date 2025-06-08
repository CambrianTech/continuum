/**
 * System Tray Integration - Continuon Status Indicator
 * Cross-platform system tray with colored status dot
 */

const { app, Menu, Tray, shell, BrowserWindow } = require('electron');
const path = require('path');

class SystemTrayManager {
  constructor(continuum) {
    this.continuum = continuum;
    this.tray = null;
    this.status = 'starting'; // starting, active, idle, error
    this.connectionCount = 0;
    this.lastActivity = Date.now();
    
    this.setupTray();
    this.startStatusMonitoring();
  }

  setupTray() {
    // Create tray icon with initial status
    this.tray = new Tray(this.getStatusIcon('starting'));
    this.tray.setToolTip('Continuum Academy - Starting...');
    
    // Set context menu
    this.updateContextMenu();
    
    // Click handler - open web interface
    this.tray.on('click', () => {
      const port = this.continuum.options.port || process.env.CONTINUUM_PORT || 9000;
      shell.openExternal(`http://localhost:${port}`);
    });
    
    console.log('🎯 Continuon system tray initialized');
  }

  getStatusIcon(status) {
    // Generate colored dot icons based on status
    // In a real implementation, these would be small PNG files
    // For now, we'll use system icons and plan for custom ones
    
    const iconMap = {
      'starting': this.createColoredDot('#FFA500'), // Orange
      'active': this.createColoredDot('#00FF41'),   // Green (Continuum brand)
      'idle': this.createColoredDot('#4FC3F7'),     // Blue
      'error': this.createColoredDot('#F44336')     // Red
    };
    
    return iconMap[status] || iconMap['starting'];
  }

  createColoredDot(color) {
    // For macOS, we can create a small colored circle
    // For Windows/Linux, we'd use different approaches
    
    if (process.platform === 'darwin') {
      // macOS: Create template image
      return this.createMacOSDot(color);
    } else if (process.platform === 'win32') {
      // Windows: Use ICO file
      return this.createWindowsDot(color);
    } else {
      // Linux: Use PNG
      return this.createLinuxDot(color);
    }
  }

  createMacOSDot(color) {
    // Create a simple colored circle for macOS
    // This would be a small PNG in practice
    const { nativeImage } = require('electron');
    
    // For now, return a default icon path
    // TODO: Generate actual colored dots
    return nativeImage.createFromPath(
      path.join(__dirname, '../../assets/icons/continuon-dot.png')
    );
  }

  createWindowsDot(color) {
    // Windows implementation
    return path.join(__dirname, '../../assets/icons/continuon-dot.ico');
  }

  createLinuxDot(color) {
    // Linux implementation  
    return path.join(__dirname, '../../assets/icons/continuon-dot.png');
  }

  updateStatus(newStatus) {
    if (this.status === newStatus) return;
    
    this.status = newStatus;
    this.tray.setImage(this.getStatusIcon(newStatus));
    
    const statusMessages = {
      'starting': 'Continuum Academy - Starting...',
      'active': `Continuum Academy - Active (${this.connectionCount} connections)`,
      'idle': 'Continuum Academy - Idle',
      'error': 'Continuum Academy - Error'
    };
    
    this.tray.setToolTip(statusMessages[newStatus]);
    this.updateContextMenu();
    
    console.log(`🎯 Continuon status: ${newStatus}`);
  }

  updateContextMenu() {
    const port = this.continuum.options.port || process.env.CONTINUUM_PORT || 9000;
    const idleTimeout = this.continuum.options.idleTimeout;
    const timeoutMinutes = idleTimeout ? Math.round(idleTimeout / 60000) : 30;
    
    const contextMenu = Menu.buildFromTemplate([
      {
        label: `Continuum Academy`,
        type: 'normal',
        enabled: false
      },
      { type: 'separator' },
      {
        label: `Status: ${this.status.charAt(0).toUpperCase() + this.status.slice(1)}`,
        type: 'normal',
        enabled: false
      },
      {
        label: `Connections: ${this.connectionCount}`,
        type: 'normal',
        enabled: false
      },
      {
        label: `Port: ${port}`,
        type: 'normal',
        enabled: false
      },
      { type: 'separator' },
      {
        label: 'Open Web Interface',
        type: 'normal',
        click: () => {
          shell.openExternal(`http://localhost:${port}`);
        }
      },
      {
        label: 'Agent Portal Help',
        type: 'normal',
        click: () => {
          shell.openExternal(`http://localhost:${port}#agent-scripts`);
        }
      },
      { type: 'separator' },
      {
        label: `Auto-shutdown: ${timeoutMinutes}min`,
        type: 'normal',
        enabled: false
      },
      {
        label: 'Restart Server',
        type: 'normal',
        click: () => {
          this.continuum.restart();
        }
      },
      {
        label: 'Quit Continuum',
        type: 'normal',
        click: () => {
          app.quit();
        }
      }
    ]);

    this.tray.setContextMenu(contextMenu);
  }

  startStatusMonitoring() {
    // Monitor connection count and activity
    setInterval(() => {
      this.updateActivityStatus();
    }, 5000); // Check every 5 seconds
  }

  updateActivityStatus() {
    const now = Date.now();
    const timeSinceActivity = now - this.lastActivity;
    
    // Update connection count
    this.connectionCount = this.continuum.activeConnections?.size || 0;
    
    // Determine status based on activity and connections
    if (this.continuum.hasError) {
      this.updateStatus('error');
    } else if (this.connectionCount > 0) {
      this.updateStatus('active');
      this.lastActivity = now; // Reset activity timer
    } else if (timeSinceActivity > 300000) { // 5 minutes
      this.updateStatus('idle');
    } else {
      this.updateStatus('active');
    }
  }

  // Called when user connects
  onUserConnected() {
    this.connectionCount++;
    this.lastActivity = Date.now();
    this.updateStatus('active');
  }

  // Called when user disconnects  
  onUserDisconnected() {
    this.connectionCount = Math.max(0, this.connectionCount - 1);
    this.updateActivityStatus();
  }

  // Called when agent portal is used
  onAgentActivity() {
    this.lastActivity = Date.now();
    this.updateStatus('active');
  }

  // Called on server error
  onError() {
    this.updateStatus('error');
  }

  destroy() {
    if (this.tray) {
      this.tray.destroy();
      this.tray = null;
    }
  }
}

module.exports = SystemTrayManager;