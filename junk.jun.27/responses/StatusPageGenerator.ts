/**
 * Status Page Generator - Creates HTML status pages for WebSocket daemon
 * Extracted from WebSocketDaemon for better modularity
 */

export interface DaemonInfo {
  name: string;
  status: string;
  uptime?: number;
}

export class StatusPageGenerator {
  constructor(
    private serverName: string,
    private serverVersion: string,
    private port: number
  ) {}

  async generateStatusPage(registeredDaemons: Map<string, any>): Promise<string> {
    const uptime = process.uptime();
    const uptimeFormatted = this.formatUptime(uptime);
    
    const daemonList = Array.from(registeredDaemons.entries())
      .map(([name, daemon]) => ({
        name,
        status: daemon.getStatus ? daemon.getStatus() : 'unknown'
      }));

    return `
<!DOCTYPE html>
<html>
<head>
    <title>Continuum Service Status</title>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        ${this.getStatusPageStyles()}
    </style>
</head>
<body>
    <div class="container">
        <h1><span class="emoji">🌐</span> Continuum Service</h1>
        <div class="status">
            <div class="status-item">
                <span class="label">Status:</span>
                <span class="value healthy">Running</span>
            </div>
            <div class="status-item">
                <span class="label">Server:</span>
                <span class="value">${this.serverName}</span>
            </div>
            <div class="status-item">
                <span class="label">Version:</span>
                <span class="value">${this.serverVersion}</span>
            </div>
            <div class="status-item">
                <span class="label">Port:</span>
                <span class="value">${this.port}</span>
            </div>
            <div class="status-item">
                <span class="label">Uptime:</span>
                <span class="value">${uptimeFormatted}</span>
            </div>
        </div>
        
        <h2>Registered Daemons</h2>
        <div class="daemons">
            ${daemonList.map(daemon => `
                <div class="daemon-item">
                    <span class="daemon-name">${daemon.name}</span>
                    <span class="daemon-status ${daemon.status}">${daemon.status}</span>
                </div>
            `).join('')}
        </div>
        
        <div class="endpoints">
            <h2>Available Endpoints</h2>
            <ul>
                <li><a href="/health">/health</a> - Health check</li>
                <li><a href="/status">/status</a> - This page</li>
                <li>/api/* - API endpoints</li>
                <li>ws:// - WebSocket connection</li>
            </ul>
        </div>
    </div>
</body>
</html>`;
  }

  private formatUptime(seconds: number): string {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    
    if (days > 0) {
      return `${days}d ${hours}h ${minutes}m`;
    } else if (hours > 0) {
      return `${hours}h ${minutes}m`;
    } else {
      return `${minutes}m`;
    }
  }

  private getStatusPageStyles(): string {
    return `
        body {
            font-family: 'SF Mono', Monaco, 'Cascadia Code', 'Roboto Mono', Consolas, 'Courier New', monospace;
            margin: 0;
            padding: 20px;
            background: linear-gradient(135deg, #1e3c72 0%, #2a5298 100%);
            color: #fff;
            min-height: 100vh;
        }
        .container {
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 10px;
            backdrop-filter: blur(10px);
        }
        h1 {
            text-align: center;
            margin-bottom: 30px;
            font-size: 2.5em;
        }
        .emoji {
            font-size: 1.2em;
            margin-right: 10px;
        }
        .status {
            background: rgba(255, 255, 255, 0.1);
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 30px;
        }
        .status-item {
            display: flex;
            justify-content: space-between;
            margin-bottom: 10px;
            padding: 5px 0;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }
        .label {
            font-weight: bold;
        }
        .value {
            color: #90EE90;
        }
        .healthy {
            color: #00ff00;
        }
        .daemons {
            background: rgba(255, 255, 255, 0.1);
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 30px;
        }
        .daemon-item {
            display: flex;
            justify-content: space-between;
            padding: 10px;
            margin-bottom: 5px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 5px;
        }
        .daemon-status.running {
            color: #00ff00;
        }
        .daemon-status.stopped {
            color: #ff6b6b;
        }
        .endpoints {
            background: rgba(255, 255, 255, 0.1);
            padding: 20px;
            border-radius: 8px;
        }
        .endpoints ul {
            list-style: none;
            padding: 0;
        }
        .endpoints li {
            padding: 8px 0;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }
        .endpoints a {
            color: #90EE90;
            text-decoration: none;
        }
        .endpoints a:hover {
            text-decoration: underline;
        }
    `;
  }
}