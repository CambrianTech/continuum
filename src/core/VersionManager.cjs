/**
 * Version Manager
 * Handles version checking, comparison, and automatic restarts
 */

const fs = require('fs');
const path = require('path');
const { spawn, exec } = require('child_process');

class VersionManager {
  constructor(continuum) {
    this.continuum = continuum;
    this.packagePath = path.join(__dirname, '../../package.json');
    this.pidFile = path.join(process.cwd(), '.continuum/continuum.pid');
    this.versionCheckInterval = 30000; // Check every 30 seconds
    this.isRestarting = false;
  }

  /**
   * Get current package version
   */
  getCurrentVersion() {
    try {
      const packageJson = JSON.parse(fs.readFileSync(this.packagePath, 'utf8'));
      return packageJson.version;
    } catch (error) {
      console.error('❌ Failed to read package version:', error.message);
      return '0.0.0';
    }
  }

  /**
   * Get running server version via API
   */
  async getRunningVersion() {
    try {
      const response = await fetch(`http://localhost:${this.continuum.port}/api/status`);
      const data = await response.json();
      return data.version || '0.0.0';
    } catch (error) {
      // Server might not be running or no version endpoint
      return null;
    }
  }

  /**
   * Compare version strings (semver-like)
   */
  compareVersions(version1, version2) {
    const v1Parts = version1.split('.').map(n => parseInt(n) || 0);
    const v2Parts = version2.split('.').map(n => parseInt(n) || 0);
    
    // Pad arrays to same length
    const maxLength = Math.max(v1Parts.length, v2Parts.length);
    while (v1Parts.length < maxLength) v1Parts.push(0);
    while (v2Parts.length < maxLength) v2Parts.push(0);
    
    for (let i = 0; i < maxLength; i++) {
      if (v1Parts[i] > v2Parts[i]) return 1;
      if (v1Parts[i] < v2Parts[i]) return -1;
    }
    return 0;
  }

  /**
   * Check if restart is needed
   */
  async shouldRestart() {
    const currentVersion = this.getCurrentVersion();
    const runningVersion = await this.getRunningVersion();
    
    if (!runningVersion) {
      console.log('🔍 No running server detected');
      return false;
    }
    
    const comparison = this.compareVersions(currentVersion, runningVersion);
    
    if (comparison > 0) {
      console.log(`🔄 Version mismatch detected:`);
      console.log(`   📦 Package version: ${currentVersion}`);
      console.log(`   🖥️  Running version: ${runningVersion}`);
      console.log(`   ⬆️  Restart needed`);
      return true;
    }
    
    return false;
  }

  /**
   * Gracefully restart the server
   */
  async restartServer() {
    if (this.isRestarting) {
      console.log('⏳ Restart already in progress...');
      return;
    }

    this.isRestarting = true;
    console.log('🔄 Starting graceful server restart...');

    try {
      // Broadcast restart notification to all connected clients
      if (this.continuum.webSocketServer) {
        this.continuum.webSocketServer.broadcast({
          type: 'server_restart',
          message: '🔄 Server restarting to apply updates... Reconnecting shortly...'
        });
      }

      // Wait for clients to receive notification
      await this.sleep(2000);

      // Get current process info
      const currentPid = process.pid;
      const scriptPath = process.argv[1];
      
      console.log(`🛑 Stopping current process (PID: ${currentPid})`);
      
      // Start new process
      const newProcess = spawn('node', [scriptPath], {
        detached: true,
        stdio: 'ignore',
        cwd: process.cwd()
      });
      
      newProcess.unref();
      
      console.log(`🚀 Started new process (PID: ${newProcess.pid})`);
      console.log('✅ Graceful restart initiated');
      
      // Exit current process after a short delay
      setTimeout(() => {
        process.exit(0);
      }, 1000);
      
    } catch (error) {
      console.error('❌ Restart failed:', error.message);
      this.isRestarting = false;
    }
  }

  /**
   * Start automatic version checking
   */
  startVersionChecking() {
    console.log(`🔍 Starting version monitoring (checking every ${this.versionCheckInterval/1000}s)`);
    
    const checkVersion = async () => {
      try {
        if (await this.shouldRestart()) {
          await this.restartServer();
          return; // Exit the interval since we're restarting
        }
      } catch (error) {
        console.error('❌ Version check failed:', error.message);
      }
    };

    // Initial check
    setTimeout(checkVersion, 5000); // Wait 5s after startup
    
    // Regular checks
    this.versionCheckTimer = setInterval(checkVersion, this.versionCheckInterval);
  }

  /**
   * Stop version checking
   */
  stopVersionChecking() {
    if (this.versionCheckTimer) {
      clearInterval(this.versionCheckTimer);
      this.versionCheckTimer = null;
      console.log('🛑 Stopped version monitoring');
    }
  }

  /**
   * Manual version check and restart if needed
   */
  async checkAndRestart() {
    console.log('🔍 Manual version check requested...');
    
    if (await this.shouldRestart()) {
      await this.restartServer();
      return true;
    } else {
      console.log('✅ Version is up to date');
      return false;
    }
  }

  /**
   * Add version info to server status API
   */
  addVersionToStatus(statusData) {
    const currentVersion = this.getCurrentVersion();
    return {
      ...statusData,
      version: currentVersion,
      versionManager: {
        monitoring: !!this.versionCheckTimer,
        checkInterval: this.versionCheckInterval,
        lastCheck: new Date().toISOString()
      }
    };
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

module.exports = VersionManager;