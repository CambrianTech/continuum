/**
 * Browser Detection Module
 * Detects browser type from WebSocket connection headers
 */

class BrowserDetector {
  static detectFromUserAgent(userAgent) {
    const ua = userAgent.toLowerCase();
    
    if (ua.includes('opera') || ua.includes('opr/')) {
      return { type: 'opera', port: 9222 };
    } else if (ua.includes('edg/')) {
      return { type: 'edge', port: 9223 };
    } else if (ua.includes('chrome/') && !ua.includes('edg/')) {
      return { type: 'chrome', port: 9222 };
    } else if (ua.includes('safari/') && !ua.includes('chrome/')) {
      return { type: 'safari', port: 9224 };
    } else if (ua.includes('firefox/')) {
      return { type: 'firefox', port: 6000 };
    }
    
    return { type: 'unknown', port: 9222 };
  }

  static extractClientInfo(ws) {
    const headers = ws.upgradeReq?.headers || ws._socket?.parser?.incoming?.headers || {};
    
    return {
      userAgent: headers['user-agent'] || 'unknown',
      origin: headers['origin'] || 'unknown',
      host: headers['host'] || 'unknown',
      connectedAt: Date.now()
    };
  }
}

module.exports = BrowserDetector;