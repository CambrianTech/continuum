#!/usr/bin/env node

const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:5555');

ws.on('open', () => {
    console.log('✅ Connected to WebSocket');
    
    // Send tab registration
    const tabData = {
        type: 'tabRegister',
        tabId: 'test-tab-' + Date.now(),
        version: '0.2.1869', // Old version to trigger update
        url: 'http://localhost:5555',
        timestamp: new Date().toISOString()
    };
    
    console.log('📤 Sending tab registration:', tabData);
    ws.send(JSON.stringify(tabData));
});

ws.on('message', (data) => {
    try {
        const message = JSON.parse(data);
        console.log('📨 Received:', message);
        
        if (message.type === 'versionUpdate') {
            console.log('🔄 VERSION UPDATE DETECTED!');
            console.log(`   Old: ${message.oldVersion || 'unknown'}`);
            console.log(`   New: ${message.version}`);
        }
    } catch (e) {
        console.log('📨 Raw message:', data.toString());
    }
});

ws.on('error', (error) => {
    console.error('❌ WebSocket error:', error);
});

// Keep connection open for 10 seconds
setTimeout(() => {
    console.log('🔌 Closing connection');
    ws.close();
    process.exit(0);
}, 10000);