#!/usr/bin/env node
/**
 * Quick Cursor Position Test
 */

const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:5555');

ws.on('open', () => {
  console.log('🔌 Connected to Continuum');
  
  // First, let's just test a simple message to see the interface
  ws.send(JSON.stringify({
    type: 'userMessage',
    message: 'Hello! I am testing the cursor position. Please take a screenshot so I can see where I am on the screen.'
  }));
  
  setTimeout(() => {
    // Take a screenshot to see the current state
    ws.send(JSON.stringify({
      type: 'userMessage', 
      message: '[CMD:SCREENSHOT] low 800x600'
    }));
  }, 2000);
  
  setTimeout(() => {
    // Now activate the AI cursor
    ws.send(JSON.stringify({
      type: 'userMessage',
      message: '[CMD:ACTIVATE_CURSOR]'
    }));
  }, 4000);
  
  setTimeout(() => {
    // Move the cursor to a visible position
    ws.send(JSON.stringify({
      type: 'userMessage',
      message: '[CMD:MOVE] 400 300 smooth'
    }));
  }, 6000);
  
  setTimeout(() => {
    // Take another screenshot to see the AI cursor
    ws.send(JSON.stringify({
      type: 'userMessage',
      message: '[CMD:SCREENSHOT] low 800x600'
    }));
  }, 8000);
  
  setTimeout(() => {
    process.exit(0);
  }, 10000);
});

ws.on('message', (data) => {
  try {
    const message = JSON.parse(data);
    console.log(`📨 ${message.type}: ${JSON.stringify(message.data || message.message).substring(0, 100)}`);
  } catch (error) {
    console.log(`📨 Raw: ${data.toString().substring(0, 100)}`);
  }
});

console.log('🤖 Quick Cursor Position Test Starting...');