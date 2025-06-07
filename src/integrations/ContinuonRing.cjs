/**
 * Continuon Ring Status Indicator
 * Ring-based system tray integration with color status
 */

const os = require('os');
const { exec } = require('child_process');

class ContinuonRing {
  constructor(continuum) {
    this.continuum = continuum;
    this.status = 'starting';
    this.connectionCount = 0;
    this.lastActivity = Date.now();
    this.platform = os.platform();
    
    this.setupRingIndicator();
    this.startStatusMonitoring();
  }

  setupRingIndicator() {
    console.log(`🔮 Initializing Continuon Ring for ${this.platform}`);
    
    // Show startup notification with ring branding
    // Use the same port that Continuum resolved from config.env
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    this.showRingNotification(
      'Continuon Active', 
      `🔮 Ring status: Starting on port ${port}`
    );
  }

  showRingNotification(title, message) {
    switch (this.platform) {
      case 'darwin': // macOS
        this.showMacRingNotification(title, message);
        break;
      case 'win32': // Windows
        this.showWindowsRingNotification(title, message);
        break;
      case 'linux': // Linux
        this.showLinuxRingNotification(title, message);
        break;
    }
  }

  showMacRingNotification(title, message) {
    // macOS notification with ring icon
    const script = `osascript -e 'display notification "${message}" with title "${title}" sound name "Glass"'`;
    exec(script, (error) => {
      if (error) console.log('🔮 Ring notification not available');
    });
  }

  showWindowsRingNotification(title, message) {
    // Windows notification
    const script = `powershell -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('${message}', '${title}', 'OK', 'Information')"`;
    exec(script, (error) => {
      if (error) console.log('🔮 Ring notification not available');
    });
  }

  showLinuxRingNotification(title, message) {
    // Linux notify-send with ring icon
    exec(`notify-send "${title}" "${message}"`, (error) => {
      if (error) console.log('🔮 Ring notification not available');
    });
  }

  updateRingStatus(newStatus) {
    if (this.status === newStatus) return;
    
    const oldStatus = this.status;
    this.status = newStatus;
    
    // Ring status with colored indicators
    const ringColors = {
      'starting': '🟠', // Orange ring
      'active': '🟢',   // Green ring (Continuum brand)
      'idle': '🔵',     // Blue ring
      'error': '🔴'     // Red ring
    };
    
    const ringMessages = {
      'starting': 'Ring initializing...',
      'active': `Ring active (${this.connectionCount} connections)`,
      'idle': 'Ring idle - ready for agents',
      'error': 'Ring error detected'
    };
    
    console.log(`🔮 Continuon Ring ${ringColors[newStatus]} ${ringMessages[newStatus]}`);
    
    // Show notification for important ring status changes
    if ((oldStatus === 'starting' && newStatus === 'active') ||
        (newStatus === 'error')) {
      this.showRingNotification(
        'Continuon Ring Status',
        `${ringColors[newStatus]} ${ringMessages[newStatus]}`
      );
    }
  }

  startStatusMonitoring() {
    // Monitor ring status - more frequent for responsive UI
    setInterval(() => {
      this.updateRingActivity();
    }, 5000); // Check every 5 seconds for ring responsiveness
  }

  updateRingActivity() {
    const now = Date.now();
    const timeSinceActivity = now - this.lastActivity;
    
    // Update connection count from WebSocket server
    this.connectionCount = this.continuum.activeConnections?.size || 0;
    
    // Ring status logic - more responsive than traditional indicators
    if (this.continuum.hasError) {
      this.updateRingStatus('error');
    } else if (this.connectionCount > 0) {
      this.updateRingStatus('active');
      this.lastActivity = now;
    } else if (timeSinceActivity > 180000) { // 3 minutes for ring (faster than traditional)
      this.updateRingStatus('idle');
    } else {
      this.updateRingStatus('active');
    }
  }

  // Ring-specific event handlers
  onUserConnected() {
    this.connectionCount++;
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
    
    // Special ring activation message
    if (this.connectionCount === 1) {
      console.log('🔮 Ring activated - first connection established');
    }
  }

  onUserDisconnected() {
    this.connectionCount = Math.max(0, this.connectionCount - 1);
    this.updateRingActivity();
    
    // Ring deactivation message
    if (this.connectionCount === 0) {
      console.log('🔮 Ring standby - all connections closed');
    }
  }

  onAgentActivity() {
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
    
    // Special ring notification for agent portal activity
    console.log('🛰️ Agent portal → 🔮 Ring active');
  }

  onError(error) {
    this.updateRingStatus('error');
    this.showRingNotification(
      'Continuon Ring Error', 
      `🔴 Ring disrupted: ${error.message || 'Unknown error'}`
    );
  }

  onShutdown() {
    this.showRingNotification(
      'Continuon Ring Shutdown',
      '🔮 Ring going dormant - agents can reactivate with "heal" command'
    );
  }

  // Ring-specific utility methods
  getRingStatusIcon() {
    const icons = {
      'starting': '🟠',
      'active': '🟢', 
      'idle': '🔵',
      'error': '🔴'
    };
    return icons[this.status] || '🟠';
  }

  getRingStatusText() {
    const texts = {
      'starting': 'Initializing',
      'active': 'Active',
      'idle': 'Standby', 
      'error': 'Error'
    };
    return texts[this.status] || 'Unknown';
  }

  // Future: Integration with actual ring UI component
  broadcastRingStatus() {
    if (this.continuum.webSocketServer) {
      this.continuum.webSocketServer.broadcast({
        type: 'continuon_ring_status',
        status: this.status,
        icon: this.getRingStatusIcon(),
        text: this.getRingStatusText(),
        connections: this.connectionCount,
        lastActivity: this.lastActivity
      });
    }
  }

  destroy() {
    console.log('🔮 Continuon Ring indicator stopped');
  }
}

module.exports = ContinuonRing;