/**
 * Continuum Multi-Agent AI System Tests
 * Tests the real AI integration and web interface
 */

const request = require('supertest');
const { spawn } = require('child_process');
const path = require('path');

describe('Continuum Multi-Agent AI System', () => {
  let continuumProcess;
  let baseURL = 'http://localhost:5555';

  beforeAll(async () => {
    // Start the continuum server
    console.log('🚀 Starting Continuum server for tests...');
    continuumProcess = spawn('node', ['continuum.cjs'], {
      cwd: __dirname,
      stdio: 'pipe'
    });

    // Wait for server to start
    await new Promise((resolve) => {
      continuumProcess.stdout.on('data', (data) => {
        if (data.toString().includes('Continuum ready')) {
          console.log('✅ Continuum server started');
          resolve();
        }
      });
    });

    // Give it a moment to fully initialize
    await new Promise(resolve => setTimeout(resolve, 2000));
  }, 30000);

  afterAll(async () => {
    if (continuumProcess) {
      console.log('🛑 Stopping Continuum server...');
      continuumProcess.kill();
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  });

  describe('Web Interface', () => {
    test('should serve the main web interface', async () => {
      const response = await fetch(baseURL);
      expect(response.status).toBe(200);
      
      const html = await response.text();
      expect(html).toContain('Continuum - Real Claude Pool');
      expect(html).toContain('GeneralAI');
      expect(html).toContain('CodeAI');
      expect(html).toContain('PlannerAI');
    });

    test('should serve status endpoint', async () => {
      const response = await fetch(`${baseURL}/status`);
      expect(response.status).toBe(200);
      
      const status = await response.json();
      expect(status).toHaveProperty('sessions');
      expect(status).toHaveProperty('costs');
      expect(status).toHaveProperty('uptime');
    });
  });

  describe('AI Instance Management', () => {
    test('should handle requests without pre-created instances', async () => {
      const response = await fetch(`${baseURL}/ask?role=TestAI&task=hello`);
      expect(response.status).toBe(200);
      
      const result = await response.json();
      expect(result).toHaveProperty('role', 'TestAI');
      expect(result).toHaveProperty('task', 'hello');
      expect(result).toHaveProperty('result');
      expect(result).toHaveProperty('costs');
    });
  });

  describe('Claude AI Integration (Anthropic)', () => {
    test('should get response from GeneralAI (Claude)', async () => {
      const response = await fetch(`${baseURL}/ask?role=GeneralAI&task=What is 2+2?`);
      expect(response.status).toBe(200);
      
      const result = await response.json();
      expect(result.role).toBe('GeneralAI');
      expect(result.task).toBe('What is 2+2?');
      expect(result.result).toContain('4');
      expect(result.costs.total).toBeGreaterThan(0);
      expect(result.costs.requests).toBeGreaterThan(0);
    });

    test('should get code response from CodeAI (Claude)', async () => {
      const response = await fetch(`${baseURL}/ask?role=CodeAI&task=Write a simple function that adds two numbers`);
      expect(response.status).toBe(200);
      
      const result = await response.json();
      expect(result.role).toBe('CodeAI');
      expect(result.result).toMatch(/function|def|const/i); // Should contain code keywords
      expect(result.costs.total).toBeGreaterThan(0);
    }, 15000);
  });

  describe('OpenAI Integration', () => {
    test('should get planning response from PlannerAI (OpenAI)', async () => {
      const response = await fetch(`${baseURL}/ask?role=PlannerAI&task=Plan a simple API with 3 endpoints`);
      expect(response.status).toBe(200);
      
      const result = await response.json();
      expect(result.role).toBe('PlannerAI');
      expect(result.result).toMatch(/API|endpoint|GET|POST/i); // Should contain API planning terms
      expect(result.costs.total).toBeGreaterThan(0);
    }, 15000);
  });

  describe('Cost Tracking', () => {
    test('should track costs accurately across requests', async () => {
      // Get initial costs
      const statusBefore = await fetch(`${baseURL}/status`).then(r => r.json());
      const initialCost = statusBefore.costs.total;
      const initialRequests = statusBefore.costs.requests;

      // Make a request
      const response = await fetch(`${baseURL}/ask?role=GeneralAI&task=Hello world`);
      const result = await response.json();

      // Check costs increased
      expect(result.costs.total).toBeGreaterThan(initialCost);
      expect(result.costs.requests).toBeGreaterThan(initialRequests);

      // Verify via status endpoint
      const statusAfter = await fetch(`${baseURL}/status`).then(r => r.json());
      expect(statusAfter.costs.total).toBeGreaterThan(initialCost);
      expect(statusAfter.costs.requests).toBeGreaterThan(initialRequests);
    });
  });

  describe('Error Handling', () => {
    test('should handle missing task parameter', async () => {
      const response = await fetch(`${baseURL}/ask?role=GeneralAI`);
      expect(response.status).toBe(400);
      
      const result = await response.json();
      expect(result).toHaveProperty('error');
      expect(result.error).toContain('No task provided');
    });

    test('should handle invalid endpoints', async () => {
      const response = await fetch(`${baseURL}/invalid-endpoint`);
      expect(response.status).toBe(404);
    });
  });

  describe('Multi-Agent Coordination', () => {
    test('should route different tasks to appropriate AI models', async () => {
      // Test code task goes to CodeAI (Claude)
      const codeResponse = await fetch(`${baseURL}/ask?role=CodeAI&task=Write a function`);
      const codeResult = await codeResponse.json();
      expect(codeResult.role).toBe('CodeAI');

      // Test planning task goes to PlannerAI (OpenAI)  
      const planResponse = await fetch(`${baseURL}/ask?role=PlannerAI&task=Plan an app`);
      const planResult = await planResponse.json();
      expect(planResult.role).toBe('PlannerAI');

      // Test general task goes to GeneralAI (Claude)
      const generalResponse = await fetch(`${baseURL}/ask?role=GeneralAI&task=What is AI?`);
      const generalResult = await generalResponse.json();
      expect(generalResult.role).toBe('GeneralAI');
    }, 20000);
  });

  describe('Performance', () => {
    test('should respond within reasonable time limits', async () => {
      const startTime = Date.now();
      const response = await fetch(`${baseURL}/ask?role=GeneralAI&task=Quick test`);
      const endTime = Date.now();
      
      expect(response.status).toBe(200);
      expect(endTime - startTime).toBeLessThan(30000); // 30 second max
    });

    test('should handle concurrent requests', async () => {
      const requests = [
        fetch(`${baseURL}/ask?role=GeneralAI&task=Test 1`),
        fetch(`${baseURL}/ask?role=CodeAI&task=Test 2`),
        fetch(`${baseURL}/ask?role=PlannerAI&task=Test 3`)
      ];

      const responses = await Promise.all(requests);
      
      responses.forEach(response => {
        expect(response.status).toBe(200);
      });

      const results = await Promise.all(responses.map(r => r.json()));
      expect(results).toHaveLength(3);
      results.forEach(result => {
        expect(result).toHaveProperty('result');
        expect(result).toHaveProperty('costs');
      });
    }, 30000);
  });
});