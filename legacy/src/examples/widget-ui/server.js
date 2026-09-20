#!/usr/bin/env node

/**
 * Widget UI Demo Server
 * Simple static file server for widget testing
 */

const http = require('http');
const fs = require('fs');
const path = require('path');

const PORT = 9003; // Different port to avoid conflicts
const ROOT_DIR = __dirname;
const DIST_DIR = path.join(__dirname, '../../dist');

function serveFile(req, res, filepath) {
  fs.readFile(filepath, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end('Not found');
      return;
    }
    
    // Set content type based on extension
    const ext = path.extname(filepath);
    const contentTypes = {
      '.html': 'text/html',
      '.js': 'application/javascript',
      '.css': 'text/css',
      '.json': 'application/json'
    };
    
    res.writeHead(200, {
      'Content-Type': contentTypes[ext] || 'text/plain',
      'Access-Control-Allow-Origin': '*'
    });
    res.end(data);
  });
}

const server = http.createServer((req, res) => {
  let filepath;
  
  if (req.url === '/' || req.url === '/index.html') {
    filepath = path.join(ROOT_DIR, 'public/demo.html');
  } else if (req.url.startsWith('/dist/')) {
    filepath = path.join(DIST_DIR, req.url.replace('/dist/', ''));
  } else {
    // Try public directory first, then root
    const publicPath = path.join(ROOT_DIR, 'public', req.url);
    const rootPath = path.join(ROOT_DIR, req.url);
    
    if (fs.existsSync(publicPath)) {
      filepath = publicPath;
    } else {
      filepath = rootPath;
    }
  }
  
  serveFile(req, res, filepath);
});

server.listen(PORT, () => {
  console.log(`🎭 Widget UI Demo Server running on http://localhost:${PORT}`);
  console.log(`📁 Serving files from: ${ROOT_DIR}`);
  console.log(`📦 JTAG dist files from: ${DIST_DIR}`);
});