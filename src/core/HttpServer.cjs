/**
 * HTTP Server
 * Handles HTTP requests and serves the web UI
 */

const http = require('http');

class HttpServer {
  constructor(continuum) {
    this.continuum = continuum;
    this.server = null;
  }

  createServer() {
    this.server = http.createServer(async (req, res) => {
      try {
        let body = '';
        req.on('data', chunk => {
          body += chunk.toString();
        });

        req.on('end', async () => {
          await this.handleRequest(req, res, body);
        });
      } catch (error) {
        console.error('Server error:', error);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Internal server error' }));
      }
    });

    return this.server;
  }

  async handleRequest(req, res, body) {
    const url = new URL(req.url, `http://localhost:${this.continuum.port}`);
    
    if (req.method === 'GET' && url.pathname === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(this.continuum.generateUI());
    } else if (req.method === 'GET' && url.pathname === '/ask') {
      await this.handleAskRequest(url, res);
    } else if (req.method === 'GET' && url.pathname === '/status') {
      await this.handleStatusRequest(res);
    } else {
      res.writeHead(404);
      res.end('Not found');
    }
  }

  async handleAskRequest(url, res) {
    const task = url.searchParams.get('task');
    
    console.log(`📨 Web request received: ${task}`);
    
    if (task) {
      try {
        console.log(`🔄 Processing task: ${task}`);
        
        // Get the initial agent that will handle this task
        const initialAgent = this.continuum.getInitialAgent(task);
        
        const result = await this.continuum.intelligentRoute(task);
        console.log(`✅ Task completed, sending response...`);
        
        // Add initial agent info and working status for better UI feedback
        result.initialAgent = initialAgent;
        
        if (result.coordination) {
          result.workingMessages = [
            'Analyzing request and selecting best AI...',
            'Multi-AI coordination required...',
            'Coordinating between AIs...',
            'Finalizing...'
          ];
        } else {
          result.workingMessages = [
            `${result.role} is thinking...`,
            'Formulating response...'
          ];
        }
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(result));
      } catch (error) {
        console.error(`❌ Task failed: ${error.message}`);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: error.message, stack: error.stack }));
      }
    } else {
      console.log(`⚠️  No task provided in request`);
      res.writeHead(400, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'No task provided' }));
    }
  }

  async handleStatusRequest(res) {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      sessions: Array.from(this.continuum.sessions.entries()),
      costs: this.continuum.costs,
      costDetails: this.continuum.costTracker.getDetailedReport(),
      uptime: process.uptime()
    }));
  }
}

module.exports = HttpServer;