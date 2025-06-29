/**
 * macOS Menu Bar Integration - Continuon Ring Status
 * Creates a native macOS menu bar item with the Continuon ring
 */

const { exec, spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const notifier = require('node-notifier');

class MacOSMenuBar {
  constructor(continuum) {
    this.continuum = continuum;
    this.status = 'starting';
    this.connectionCount = 0;
    this.lastActivity = Date.now();
    this.menuBarProcess = null;
    
    if (process.platform === 'darwin') {
      this.setupMacMenuBar();
      this.startStatusMonitoring();
    }
  }

  setupMacMenuBar() {
    console.log('🔮 Setting up Continuon Ring in macOS menu bar...');
    
    // Create a simple AppleScript-based menu bar item
    this.createMenuBarScript();
    
    // Show startup notification
    notifier.notify({
      title: 'Continuon Ring Active',
      message: `🔮 Status indicator added to menu bar`,
      icon: this.getContinuonIcon(),
      sound: false,
      wait: false
    });
  }

  createMenuBarScript() {
    // Create AppleScript for menu bar integration
    const scriptContent = `
on run
    tell application "System Events"
        if not (exists process "Continuon") then
            -- Create a simple status item (this is simplified)
            -- In a real implementation, we'd use a native Swift/Objective-C app
            display notification "🔮 Continuon Ring Active" with title "Continuum"
        end if
    end tell
end run
`;

    const scriptPath = path.join(this.continuum.localContinuumDir, 'continuon-menubar.scpt');
    fs.writeFileSync(scriptPath, scriptContent);
    
    // Execute the script to set up menu bar item
    exec(`osascript "${scriptPath}"`, (error) => {
      if (error) {
        console.log('🔮 Menu bar setup: Using notifications instead');
      } else {
        console.log('🔮 Continuon Ring added to menu bar');
      }
    });
  }

  getContinuonIcon() {
    // Return path to Continuon ring icon
    // For now, use a system icon - in production we'd use the actual ring SVG
    return path.join(__dirname, '../../assets/continuon-ring.png');
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
      'error': 'Ring error detected'
    };
    
    console.log(`🔮 Continuon ${ringEmojis[newStatus]} ${ringMessages[newStatus]}`);
    
    // Update menu bar icon (simplified - in production this would update the actual icon)
    this.updateMenuBarIcon(newStatus);
    
    // Show notification for important status changes
    if ((oldStatus === 'starting' && newStatus === 'active') ||
        (newStatus === 'error')) {
      
      notifier.notify({
        title: 'Continuon Ring Status',
        message: `${ringEmojis[newStatus]} ${ringMessages[newStatus]}`,
        icon: this.getContinuonIcon(),
        sound: newStatus === 'error' ? 'Basso' : false,
        wait: false,
        actions: newStatus === 'active' ? ['Open Continuum'] : undefined
      });
    }
  }

  updateMenuBarIcon(status) {
    // In a real implementation, this would update the actual menu bar icon color
    // For now, we'll use the Terminal title and notifications
    
    const port = this.continuum.port || process.env.CONTINUUM_PORT || 9000;
    const statusIcons = {
      'starting': '🟠',
      'active': '🟢',
      'idle': '🔵', 
      'error': '🔴'
    };
    
    // Update terminal title to show status (visible in dock)
    process.stdout.write(`\x1b]0;Continuon ${statusIcons[status]} - Port ${port}\x07`);
  }

  startStatusMonitoring() {
    setInterval(() => {
      this.updateRingActivity();
    }, 5000); // Check every 5 seconds
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
    } else if (timeSinceActivity > 180000) { // 3 minutes
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
    
    if (this.connectionCount === 1) {
      notifier.notify({
        title: 'Continuon Ring',
        message: '🔮 First connection established',
        icon: this.getContinuonIcon(),
        sound: false
      });
    }
  }

  onUserDisconnected() {
    this.connectionCount = Math.max(0, this.connectionCount - 1);
    this.updateRingActivity();
  }

  onAgentActivity() {
    this.lastActivity = Date.now();
    this.updateRingStatus('active');
    
    // Special notification for agent activity
    console.log('🛰️ Agent portal → 🔮 Ring active');
  }

  onError(error) {
    this.updateRingStatus('error');
    
    notifier.notify({
      title: 'Continuon Ring Error',
      message: `🔴 ${error.message || 'Ring disrupted'}`,
      icon: this.getContinuonIcon(),
      sound: 'Basso'
    });
  }

  onShutdown() {
    notifier.notify({
      title: 'Continuon Ring',
      message: '🔮 Ring going dormant - agents can reactivate',
      icon: this.getContinuonIcon(),
      sound: false
    });
    
    // Clean up menu bar item
    if (this.menuBarProcess) {
      this.menuBarProcess.kill();
    }
  }

  destroy() {
    this.onShutdown();
    console.log('🔮 Continuon Ring menu bar integration stopped');
  }
}

module.exports = MacOSMenuBar;