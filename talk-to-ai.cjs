#!/usr/bin/env node
/**
 * TALK TO AI
 * 
 * Actually have a conversation with the Simple AI right now
 */
const WebSocket = require('ws');
const readline = require('readline');

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout
});

let ws;

function connectAndTalk() {
  console.log('🤖 Connecting to Simple AI at localhost:5559...');
  
  ws = new WebSocket('ws://localhost:5559');
  
  ws.on('open', () => {
    console.log('✅ Connected! You can now talk to the AI.');
    console.log('Available personas: QuestionerClaude, PlannerClaude, ImplementerClaude');
    console.log('Type "quit" to exit\n');
    
    askForMessage();
  });
  
  ws.on('message', (data) => {
    try {
      const response = JSON.parse(data);
      console.log(`\n🎭 ${response.persona}: ${response.response}\n`);
      askForMessage();
    } catch (error) {
      console.log('❌ Error parsing response:', error.message);
      askForMessage();
    }
  });
  
  ws.on('error', (error) => {
    console.log('❌ Connection error:', error.message);
    process.exit(1);
  });
  
  ws.on('close', () => {
    console.log('🔌 Connection closed');
    process.exit(0);
  });
}

function askForMessage() {
  rl.question('👤 You: ', (message) => {
    if (message.toLowerCase() === 'quit') {
      ws.close();
      return;
    }
    
    rl.question('🎭 Persona (Q/P/I or QuestionerClaude/PlannerClaude/ImplementerClaude): ', (persona) => {
      let selectedPersona = 'QuestionerClaude';
      
      if (persona.toLowerCase().startsWith('p')) {
        selectedPersona = 'PlannerClaude';
      } else if (persona.toLowerCase().startsWith('i')) {
        selectedPersona = 'ImplementerClaude';
      }
      
      console.log(`📨 Sending to ${selectedPersona}...`);
      
      ws.send(JSON.stringify({
        message: message,
        persona: selectedPersona
      }));
    });
  });
}

connectAndTalk();