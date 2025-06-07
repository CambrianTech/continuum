/**
 * Continuon System Tray - Real Menu Bar Icon
 * Cross-platform system tray icon just like Docker
 */

const SysTray = require('systray').default;
const { exec } = require('child_process');
const path = require('path');

class ContinuonTray {
  constructor(continuum) {
    this.continuum = continuum;
    this.status = 'starting';
    this.connectionCount = 0;
    this.lastActivity = Date.now();
    this.systray = null;
    
    this.setupSystemTray();
    this.startStatusMonitoring();
  }

  setupSystemTray() {
    console.log('🔮 Setting up Continuon Ring in system tray...');
    
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    
    // Create system tray with Continuon branding
    this.systray = new SysTray({
      menu: {
        // Use a simple text icon for now - in production we'd use actual ring icon
        icon: this.getStatusIcon('starting'),
        title: "Continuon",
        tooltip: `Continuum Academy - Port ${port}`,
        items: [
          {
            title: `Continuon Ring - Starting`,
            tooltip: "Status indicator",
            enabled: false
          },
          SysTray.separator,
          {
            title: "Open Web Interface",
            tooltip: `Open http://localhost:${port}`,
            enabled: true,
            key: "open_web"
          },
          {
            title: "Agent Portal Help",
            tooltip: "Learn about agent automation",
            enabled: true,
            key: "agent_help"
          },
          SysTray.separator,
          {
            title: `Port: ${port}`,
            enabled: false
          },
          {
            title: "Auto-shutdown: 30min",
            enabled: false
          },
          SysTray.separator,
          {
            title: "Restart Server",
            tooltip: "Restart Continuum",
            enabled: true,
            key: "restart"
          },
          {
            title: "Quit Continuum",
            tooltip: "Shutdown server",
            enabled: true,
            key: "quit"
          }
        ]
      },
      debug: false,
      copyDir: true // Copy binaries to temp directory
    });

    // Handle menu clicks
    this.systray.onClick(action => {
      switch (action.item.key) {
        case 'open_web':
          this.openWebInterface();
          break;
        case 'agent_help':
          this.openAgentHelp();
          break;
        case 'restart':
          this.restartServer();
          break;
        case 'quit':
          this.quitServer();
          break;
      }
    });

    console.log('🔮 Continuon Ring added to system tray - just like Docker!');
  }

  getStatusIcon(status) {
    // For cross-platform compatibility, use base64 encoded small icons
    // These would be actual colored ring icons in production
    const icons = {
      'starting': this.createColorIcon('#FFA500'), // Orange ring
      'active': this.createColorIcon('#00FF41'),   // Green ring (Continuum brand)
      'idle': this.createColorIcon('#4FC3F7'),     // Blue ring
      'error': this.createColorIcon('#F44336')     // Red ring
    };
    
    return icons[status] || icons['starting'];
  }

  createColorIcon(color) {
    // Create a simple colored circle icon (base64 PNG)
    // In production, this would be the actual Continuon ring SVG converted to PNG
    
    // For now, return a minimal icon path or base64 data
    // This is a placeholder - would need actual ring icons
    return path.join(__dirname, '../../assets/continuon-ring.png');
  }

  updateRingStatus(newStatus) {
    if (this.status === newStatus) return;
    
    const oldStatus = this.status;
    this.status = newStatus;
    
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    const statusText = {
      'starting': 'Starting...',
      'active': `Active (${this.connectionCount} connections)`,
      'idle': 'Idle - Ready for agents',
      'error': 'Error detected'
    };
    
    // Update system tray menu
    if (this.systray) {
      this.systray.sendAction({
        type: 'update-item',
        item: {
          title: `Continuon Ring - ${statusText[newStatus]}`,
          tooltip: `Status: ${statusText[newStatus]}`
        },
        seq_id: 0 // First menu item
      });
      
      // Update icon color
      this.systray.sendAction({
        type: 'update-icon',
        icon: this.getStatusIcon(newStatus)
      });
      
      // Update tooltip
      this.systray.sendAction({
        type: 'update-tooltip',
        tooltip: `Continuum Academy - ${statusText[newStatus]} - Port ${port}`
      });
    }
    
    console.log(`🔮 Continuon Ring: ${statusText[newStatus]}`);
  }

  openWebInterface() {
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    const url = `http://localhost:${port}`;
    
    console.log(`🌐 Opening Continuum web interface: ${url}`);
    
    // Cross-platform browser opening
    const openCommand = process.platform === 'darwin' ? 'open' :
                       process.platform === 'win32' ? 'start' : 'xdg-open';
    
    exec(`${openCommand} "${url}"`, (error) => {
      if (error) {
        console.log(`⚠️ Could not open browser automatically`);
        console.log(`💡 Manually navigate to: ${url}`);
      }
    });
  }

  openAgentHelp() {
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    const url = `http://localhost:${port}#agent-scripts`;
    
    console.log('🛰️ Opening agent portal help...');
    
    const openCommand = process.platform === 'darwin' ? 'open' :
                       process.platform === 'win32' ? 'start' : 'xdg-open';
    
    exec(`${openCommand} "${url}"`, (error) => {
      if (error) {
        console.log(`💡 Agent help: Run "continuum --help" for agent portal info`);
      }
    });
  }

  restartServer() {
    console.log('🔄 Restarting Continuum server via system tray...');
    
    // Update tray to show restarting
    if (this.systray) {
      this.systray.sendAction({
        type: 'update-item',
        item: {
          title: `Continuon Ring - Restarting...`,
          tooltip: 'Server is restarting'
        },
        seq_id: 0
      });
    }
    
    // Graceful restart
    setTimeout(() => {
      process.exit(0); // Let daemon restart automatically
    }, 1000);
  }

  quitServer() {
    console.log('👋 Shutting down Continuum via system tray...');
    
    if (this.systray) {
      this.systray.kill();
    }
    
    process.exit(0);
  }

  startStatusMonitoring() {
    setInterval(() => {
      this.updateRingActivity();
    }, 10000); // Update every 10 seconds
  }

  updateRingActivity() {
    const now = Date.now();
    const timeSinceActivity = now - this.lastActivity;
    
    this.connectionCount = this.continuum.activeConnections?.size || 0;
    
    if (this.continuum.hasError) {
      this.updateRingStatus('error');
    } else if (this.connectionCount > 0) {
      this.updateRingStatus('active');
      this.lastActivity = now;
    } else if (timeSinceActivity > 300000) { // 5 minutes
      this.updateRingStatus('idle');
    } else {
      this.updateRingStatus('active');
    }
  }

  // Event handlers for Continuon Ring
  onUserConnected() {
    this.connectionCount++;
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
    
    console.log(`🔮 Ring: User connected (${this.connectionCount} total)`);
  }

  onUserDisconnected() {
    this.connectionCount = Math.max(0, this.connectionCount - 1);
    this.updateRingActivity();
    
    console.log(`🔮 Ring: User disconnected (${this.connectionCount} remaining)`);
  }

  onAgentActivity() {
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
    
    console.log('🛰️ Agent portal → 🔮 Ring activated');
  }

  onError(error) {
    this.updateRingStatus('error');
    
    console.log(`🚨 Continuon Ring Error: ${error.message || 'Unknown error'}`);
    console.log('💡 Right-click tray icon for restart option');
  }

  onShutdown() {
    console.log('🔮 Continuon Ring shutting down...');
    
    if (this.systray) {
      this.systray.kill();
    }
  }

  destroy() {
    this.onShutdown();
  }
}

module.exports = ContinuonTray;