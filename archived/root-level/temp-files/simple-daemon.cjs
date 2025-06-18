#!/usr/bin/env node
/**
 * Simple, honest daemon that doesn't lie about its status
 * No spaghetti, no bullshit, just works
 */

const http = require('http');
const path = require('path');
const fs = require('fs');

// Simple logger that doesn't crash on EPIPE
function safeLog(msg) {
  try {
    console.log(msg);
  } catch (e) {
    // Silently ignore broken pipes
  }
}

// Create simple HTTP server
const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/html' });
  res.end(`
    <!DOCTYPE html>
    <html>
      <head><title>Continuum - Working Daemon</title></head>
      <body>
        <h1>🚀 Continuum Daemon is ACTUALLY running</h1>
        <p>This daemon tells the truth about its status.</p>
        <p>Time: ${new Date().toISOString()}</p>
      </body>
    </html>
  `);
});

// Start server and ONLY report success when it's actually listening
server.listen(9000, 'localhost', () => {
  safeLog('✅ HTTP server is ACTUALLY listening on http://localhost:9000');
  safeLog('🎯 Daemon is GENUINELY working - no lies!');
});

// Handle errors honestly
server.on('error', (err) => {
  safeLog(`❌ Server error: ${err.message}`);
  process.exit(1);
});

// Graceful shutdown
process.on('SIGTERM', () => {
  safeLog('📴 Shutting down...');
  server.close(() => {
    safeLog('✅ Server closed cleanly');
    process.exit(0);
  });
});

process.on('SIGINT', () => {
  safeLog('📴 Ctrl+C pressed, shutting down...');
  server.close(() => {
    safeLog('✅ Server closed cleanly');
    process.exit(0);
  });
});