/**
 * Simple Menu Bar Integration - Click to Open Web Portal
 * Focus on the web interface as the main desktop solution
 */

const { exec } = require('child_process');
const notifier = require('node-notifier');

class SimpleMenuBar {
  constructor(continuum) {
    this.continuum = continuum;
    this.status = 'starting';
    this.connectionCount = 0;
    this.lastActivity = Date.now();
    
    this.setupSimpleIntegration();
    this.startStatusMonitoring();
  }

  setupSimpleIntegration() {
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    
    console.log('🔮 Continuon Ring active');
    console.log(`💡 Web interface: http://localhost:${port}`);
    console.log('📱 Browser should open automatically');
    
    // NO automatic notifications - just console output
    // Users can enable notifications later if they want them
  }

  openWebPortal() {
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    const url = `http://localhost:${port}`;
    
    console.log(`🌐 Opening Continuum web portal: ${url}`);
    
    // Open in user's default browser - cross platform
    const openCommand = process.platform === 'darwin' ? 'open' : 
                       process.platform === 'win32' ? 'start' : 'xdg-open';
    
    exec(`${openCommand} "${url}"`, (error) => {
      if (error) {
        console.log(`⚠️ Could not open browser: ${error.message}`);
        console.log(`💡 Manually navigate to: ${url}`);
      }
    });
  }

  updateRingStatus(newStatus) {
    if (this.status === newStatus) return;
    
    const oldStatus = this.status;
    this.status = newStatus;
    
    const ringEmojis = {
      'starting': '🟠',
      'active': '🟢', 
      'idle': '🔵',
      'error': '🔴'
    };
    
    const ringMessages = {
      'starting': 'Ring initializing...',
      'active': `Ring active (${this.connectionCount} connections)`,
      'idle': 'Ring idle - ready for agents',
      'error': 'Ring error - click to diagnose'
    };
    
    console.log(`🔮 Continuon ${ringEmojis[newStatus]} ${ringMessages[newStatus]}`);
    
    // Update terminal/dock title with current status
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    process.stdout.write(`\x1b]0;Continuon ${ringEmojis[newStatus]} - Port ${port}\x07`);
    
    // Only show notifications for actual errors - be respectful of permissions
    if (newStatus === 'error') {
      this.showErrorNotification(ringEmojis[newStatus], ringMessages[newStatus]);
    }
  }

  showErrorNotification(emoji, message) {
    // Only show notifications for actual errors - user already gave permission
    console.log(`🚨 ${emoji} ${message}`);
    console.log('💡 Open web interface to diagnose: http://localhost:' + (this.continuum.port || 9000));
    
    // Optional: Show notification only if user previously enabled them
    // For now, just console output to be respectful of permissions
  }

  restartServer() {
    console.log('🔄 Restarting Continuum server...');
    
    // Restart the server (this will trigger process restart)
    setTimeout(() => {
      process.exit(0); // Graceful restart - daemon will restart automatically
    }, 1000);
  }

  startStatusMonitoring() {
    setInterval(() => {
      this.updateRingActivity();
    }, 10000); // Check every 10 seconds - less frequent since we're focused on web portal
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

  // Simple event handlers - focus on web portal integration
  onUserConnected() {
    this.connectionCount++;
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
  }

  onUserDisconnected() {
    this.connectionCount = Math.max(0, this.connectionCount - 1);
    this.updateRingActivity();
  }

  onAgentActivity() {
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
    console.log('🛰️ Agent portal active → Web interface updated');
  }

  onError(error) {
    this.updateRingStatus('error');
    
    // Just console output - no notification permissions required
    console.log(`🚨 Continuum Error: ${error.message || 'Error detected'}`);
    console.log('💡 Open web interface for diagnostics: http://localhost:' + (this.continuum.port || 9000));
  }

  onShutdown() {
    console.log('🔮 Ring going dormant');
    console.log('💡 Agents can reactivate with "heal" command');
  }

  destroy() {
    this.onShutdown();
    console.log('🔮 Simple menu bar integration stopped');
  }
}

module.exports = SimpleMenuBar;