/**
 * Simple Client Manager
 * Just tracks browser info and launches Opera when needed
 */

class SimpleClientManager {
  constructor(continuum) {
    this.continuum = continuum;
    this.clients = new Map();
  }

  addClient(sessionId, ws) {
    const headers = ws.upgradeReq?.headers || ws._socket?.parser?.incoming?.headers || {};
    const userAgent = headers['user-agent'] || 'unknown';
    
    const browserType = this.detectBrowser(userAgent);
    const client = {
      sessionId,
      userAgent,
      browserType,
      connectedAt: Date.now()
    };

    this.clients.set(sessionId, client);
    console.log(`🌐 Client: ${browserType} connected`);

    // Launch Opera if detected
    if (browserType === 'opera') {
      this.launchOpera();
    }
  }

  detectBrowser(userAgent) {
    const ua = userAgent.toLowerCase();
    if (ua.includes('opera') || ua.includes('opr/')) return 'opera';
    if (ua.includes('edg/')) return 'edge';
    if (ua.includes('chrome/')) return 'chrome';
    if (ua.includes('safari/')) return 'safari';
    return 'unknown';
  }

  launchOpera() {
    const { spawn } = require('child_process');
    const fs = require('fs');
    
    const operaPath = '/Applications/Opera GX.app/Contents/MacOS/Opera';
    
    if (fs.existsSync(operaPath)) {
      console.log('🚀 Launching Opera with DevTools...');
      
      spawn(operaPath, [
        '--remote-debugging-port=9222',
        'http://localhost:9000'
      ], { detached: true, stdio: 'ignore' });
      
      this.continuum.continuonStatus.updateStatusText(
        '🚀 Launching Opera with DevTools...',
        { duration: 3000 }
      );
    }
  }

  removeClient(sessionId) {
    this.clients.delete(sessionId);
  }

  getClients() {
    return Array.from(this.clients.values());
  }
}

module.exports = SimpleClientManager;