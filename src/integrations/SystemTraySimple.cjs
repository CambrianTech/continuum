/**
 * Simple System Tray Integration - Continuon Status Indicator
 * Cross-platform system tray without Electron dependency
 * Uses native system notification APIs for now
 */

const os = require('os');
const { exec } = require('child_process');

class SimpleSystemTray {
  constructor(continuum) {
    this.continuum = continuum;
    this.status = 'starting';
    this.connectionCount = 0;
    this.lastActivity = Date.now();
    this.platform = os.platform();
    
    this.setupPlatformIntegration();
    this.startStatusMonitoring();
  }

  setupPlatformIntegration() {
    console.log(`🎯 Initializing Continuon status indicator for ${this.platform}`);
    
    // Show startup notification
    this.showNotification(
      'Continuum Academy Started', 
      `🎯 Continuon active on port ${this.continuum.options.port || 5555}`
    );
  }

  showNotification(title, message) {
    switch (this.platform) {
      case 'darwin': // macOS
        this.showMacNotification(title, message);
        break;
      case 'win32': // Windows
        this.showWindowsNotification(title, message);
        break;
      case 'linux': // Linux
        this.showLinuxNotification(title, message);
        break;
    }
  }

  showMacNotification(title, message) {
    const script = `osascript -e 'display notification "${message}" with title "${title}"'`;
    exec(script, (error) => {
      if (error) console.log('📱 Notification not available');
    });
  }

  showWindowsNotification(title, message) {
    // Windows PowerShell notification
    const script = `powershell -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('${message}', '${title}', 'OK', 'Information')"`;
    exec(script, (error) => {
      if (error) console.log('📱 Notification not available');
    });
  }

  showLinuxNotification(title, message) {
    // Linux notify-send
    exec(`notify-send "${title}" "${message}"`, (error) => {
      if (error) console.log('📱 Notification not available');
    });
  }

  updateStatus(newStatus) {
    if (this.status === newStatus) return;
    
    const oldStatus = this.status;
    this.status = newStatus;
    
    const statusColors = {
      'starting': '🟠', // Orange
      'active': '🟢',   // Green (Continuum brand)
      'idle': '🔵',     // Blue
      'error': '🔴'     // Red
    };
    
    const statusMessages = {
      'starting': 'Starting up...',
      'active': `Active (${this.connectionCount} connections)`,
      'idle': 'Idle - ready for connections',
      'error': 'Error detected'
    };
    
    console.log(`🎯 Continuon ${statusColors[newStatus]} ${statusMessages[newStatus]}`);
    
    // Show notification for important status changes
    if ((oldStatus === 'starting' && newStatus === 'active') ||
        (newStatus === 'error')) {
      this.showNotification(
        'Continuon Status Update',
        `${statusColors[newStatus]} ${statusMessages[newStatus]}`
      );
    }
  }

  startStatusMonitoring() {
    // Monitor connection count and activity
    setInterval(() => {
      this.updateActivityStatus();
    }, 10000); // Check every 10 seconds
  }

  updateActivityStatus() {
    const now = Date.now();
    const timeSinceActivity = now - this.lastActivity;
    
    // Update connection count from WebSocket server
    this.connectionCount = this.continuum.activeConnections?.size || 0;
    
    // Determine status based on activity and connections
    if (this.continuum.hasError) {
      this.updateStatus('error');
    } else if (this.connectionCount > 0) {
      this.updateStatus('active');
      this.lastActivity = now;
    } else if (timeSinceActivity > 300000) { // 5 minutes
      this.updateStatus('idle');
    } else {
      this.updateStatus('active');
    }
  }

  // Integration points for Continuum events
  onUserConnected() {
    this.connectionCount++;
    this.lastActivity = Date.now();
    this.updateStatus('active');
  }

  onUserDisconnected() {
    this.connectionCount = Math.max(0, this.connectionCount - 1);
    this.updateActivityStatus();
  }

  onAgentActivity() {
    this.lastActivity = Date.now();
    this.updateStatus('active');
    
    // Special notification for first agent activity
    if (this.status === 'idle') {
      console.log('🛰️ Agent portal activated - Continuon status: active');
    }
  }

  onError(error) {
    this.updateStatus('error');
    this.showNotification(
      'Continuum Error', 
      `🔴 ${error.message || 'Unknown error occurred'}`
    );
  }

  onShutdown() {
    this.showNotification(
      'Continuum Shutdown',
      '🎯 Continuon going idle - agents can wake with "heal" command'
    );
  }

  destroy() {
    console.log('🎯 Continuon status indicator stopped');
  }
}

module.exports = SimpleSystemTray;