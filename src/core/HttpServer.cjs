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
    } else if (req.method === 'GET' && (url.pathname === '/status' || url.pathname === '/api/status')) {
      await this.handleStatusRequest(res);
    } else if (url.pathname.startsWith('/api/personas')) {
      await this.handlePersonaRequest(req, res, url, body);
    } else if (url.pathname.startsWith('/api/projects')) {
      await this.handleProjectRequest(req, res, url, body);
    } else if (url.pathname.startsWith('/api/academy')) {
      await this.handleAcademyRequest(req, res, url, body);
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
        
        // 🤖 Protocol Sheriff: Validate response before sending to user
        const validation = await this.continuum.protocolSheriff.validateResponse(
          result.result, 
          task, 
          result.role
        );
        
        if (!validation.isValid && validation.correctedResponse) {
          console.log(`🚨 Protocol Sheriff: Using corrected response`);
          result.result = validation.correctedResponse;
        }
        
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
    const statusData = {
      sessions: Array.from(this.continuum.sessions.entries()),
      costs: this.continuum.costs,
      costDetails: this.continuum.costTracker.getDetailedReport(),
      uptime: process.uptime()
    };
    
    // Add version information via version manager
    const statusWithVersion = this.continuum.versionManager.addVersionToStatus(statusData);
    
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(statusWithVersion));
  }

  async handlePersonaRequest(req, res, url, body) {
    try {
      const Persona = require('./Persona.cjs');
      
      if (req.method === 'GET' && url.pathname === '/api/personas') {
        // List all personas
        const personas = await Persona.listAll();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(personas));
        
      } else if (req.method === 'POST' && url.pathname.match(/^\/api\/personas\/([^\/]+)\/share$/)) {
        // Share persona
        const personaId = url.pathname.match(/^\/api\/personas\/([^\/]+)\/share$/)[1];
        const data = JSON.parse(body);
        
        const persona = await Persona.load(personaId);
        const shareResult = await persona.share(data.toScope);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          personaName: persona.name,
          shareResult
        }));
        
      } else if (req.method === 'POST' && url.pathname.match(/^\/api\/personas\/([^\/]+)\/deploy$/)) {
        // Deploy persona
        const personaId = url.pathname.match(/^\/api\/personas\/([^\/]+)\/deploy$/)[1];
        const data = JSON.parse(body);
        
        const persona = await Persona.load(personaId);
        const deployment = persona.deploy(data);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          deployment
        }));
        
      } else if (req.method === 'DELETE' && url.pathname.match(/^\/api\/personas\/([^\/]+)$/)) {
        // Delete persona (this would need implementation in PersonaRegistry)
        const personaId = url.pathname.match(/^\/api\/personas\/([^\/]+)$/)[1];
        
        // For now, just return success - actual deletion would require PersonaRegistry method
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          personaName: personaId,
          message: 'Delete functionality not yet implemented'
        }));
        
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Persona API endpoint not found' }));
      }
      
    } catch (error) {
      console.error('Persona API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        success: false,
        error: error.message 
      }));
    }
  }

  async handleProjectRequest(req, res, url, body) {
    try {
      if (req.method === 'POST' && url.pathname === '/api/projects/register') {
        // Register a new project
        const projectInfo = JSON.parse(body);
        const result = this.continuum.registerProject(projectInfo);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          ...result
        }));
        
      } else if (req.method === 'GET' && url.pathname === '/api/projects') {
        // List all registered projects
        const projects = this.continuum.getRegisteredProjects();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(projects));
        
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Project API endpoint not found' }));
      }
      
    } catch (error) {
      console.error('Project API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        success: false,
        error: error.message 
      }));
    }
  }

  async handleAcademyRequest(req, res, url, body) {
    try {
      if (req.method === 'POST' && url.pathname === '/api/academy/train') {
        // Start Academy training
        const trainingParams = JSON.parse(body);
        
        // Get the AcademyWebInterface from continuum
        if (this.continuum.uiGenerator?.academyInterface) {
          const trainingSession = await this.continuum.uiGenerator.academyInterface.startAcademyTraining(
            trainingParams.personaName,
            trainingParams.specialization,
            {
              rounds: trainingParams.trainingRounds || 10,
              passingScore: trainingParams.passingScore || 0.85
            }
          );
          
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            success: true,
            trainingSession
          }));
        } else {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            success: false,
            error: 'Academy interface not available'
          }));
        }
        
      } else if (req.method === 'GET' && url.pathname === '/api/academy/status') {
        // Get Academy status
        const academyInterface = this.continuum.uiGenerator?.academyInterface;
        if (academyInterface) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            activeTraining: Array.from(academyInterface.trainingPersonas.entries()),
            completedPersonas: Array.from(academyInterface.completedPersonas.entries())
          }));
        } else {
          res.writeHead(404, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Academy not available' }));
        }
        
      } else {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Academy API endpoint not found' }));
      }
      
    } catch (error) {
      console.error('Academy API error:', error);
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ 
        success: false,
        error: error.message 
      }));
    }
  }
}

module.exports = HttpServer;