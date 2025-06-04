#!/usr/bin/env node
const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:5555');
ws.on('open', () => {
    console.log('🔄 Sending browser reload command...');
    ws.send(JSON.stringify({
        type: 'userMessage', 
        message: '[CMD:WEB_RELOAD]'
    }));
    setTimeout(() => {
        ws.close();
        process.exit(0);
    }, 1000);
});