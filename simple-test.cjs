#!/usr/bin/env node
/**
 * SIMPLE TEST - Verify AIs can create new processes and coordinate
 */

const http = require('http');
const { spawn } = require('child_process');
const { Anthropic } = require('@anthropic-ai/sdk');
require('dotenv').config();

const anthropic = new Anthropic({
  apiKey: process.env.ANTHROPIC_API_KEY,
});

class SimpleTest {
  constructor() {
    this.port = 5558;
    this.processes = new Map();
    
    console.log('🧪 SIMPLE AI PROCESS TEST');
    console.log('=========================');
    
    this.start();
  }

  async callAI(prompt) {
    try {
      const completion = await anthropic.messages.create({
        model: "claude-3-haiku-20240307",
        max_tokens: 512,
        messages: [{ 
          role: "user", 
          content: `You are a test AI. Answer this simple question: ${prompt}` 
        }],
      });
      return completion.content[0].text;
    } catch (error) {
      return `Error: ${error.message}`;
    }
  }

  start() {
    const server = http.createServer(async (req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      
      if (req.method === 'OPTIONS') {
        res.writeHead(200);
        res.end();
        return;
      }

      const url = new URL(req.url, `http://localhost:${this.port}`);
      
      if (url.pathname === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(`<!DOCTYPE html>
<html>
<head><title>Simple AI Test</title></head>
<body style="font-family: monospace; background: #000; color: #0f0; padding: 20px;">
<h1>🧪 Simple AI Test</h1>
<p>Test basic AI functionality</p>
<input id="question" placeholder="Ask something simple..." style="width: 400px; padding: 10px; background: #222; color: #0f0; border: 1px solid #555;">
<button onclick="ask()" style="padding: 10px; background: #006600; color: white; border: none;">Ask AI</button>
<div id="result" style="margin-top: 20px; padding: 10px; background: #111; white-space: pre-wrap;"></div>
<script>
async function ask() {
  const question = document.getElementById('question').value;
  const result = document.getElementById('result');
  
  result.textContent = 'Thinking...';
  
  try {
    const response = await fetch('/ask?q=' + encodeURIComponent(question));
    const text = await response.text();
    result.textContent = 'AI: ' + text;
  } catch (error) {
    result.textContent = 'Error: ' + error.message;
  }
}
</script>
</body>
</html>`);
        
      } else if (url.pathname === '/ask') {
        const question = url.searchParams.get('q');
        if (!question) {
          res.writeHead(400);
          res.end('No question provided');
          return;
        }
        
        const answer = await this.callAI(question);
        res.writeHead(200, { 'Content-Type': 'text/plain' });
        res.end(answer);
        
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    server.listen(this.port, () => {
      console.log(`🌐 Simple test running: http://localhost:${this.port}`);
      console.log('🧪 Test basic AI functionality first');
    });
  }
}

new SimpleTest();