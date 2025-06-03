#!/usr/bin/env node

const WebSocket = require('ws');

console.log('[TEST CLIENT] Starting WebSocket test client...');

// Create WebSocket connection
const ws = new WebSocket('ws://localhost:5555');

let startTime = Date.now();
let hasReceived = false;

// Function to get timestamp
function getTimestamp() {
    return new Date().toISOString();
}

// Connection opened
ws.on('open', function() {
    console.log(`[${getTimestamp()}] Connected to WebSocket server`);
    
    // Wait longer for any initial messages and processing, then send our test task
    setTimeout(() => {
        const testMessage = {
            type: "task",
            role: "PlannerAI",
            task: "What is 10 minus 3?"
        };
        
        console.log(`[${getTimestamp()}] Sending test message:`, JSON.stringify(testMessage));
        ws.send(JSON.stringify(testMessage));
    }, 3000);
});

// Message received
ws.on('message', function(data) {
    hasReceived = true;
    const timestamp = getTimestamp();
    
    try {
        const message = JSON.parse(data.toString());
        console.log(`[${timestamp}] Received message:`, JSON.stringify(message, null, 2));
        
        // Check if this is the expected response
        if (message.type === 'result' && message.data && message.data.result) {
            const result = message.data.result;
            if (result.includes('7') || result.includes('seven')) {
                console.log(`[${timestamp}] ✅ SUCCESS: Received expected mathematical result containing "7": ${result}`);
            } else {
                console.log(`[${timestamp}] ⚠️  UNEXPECTED: Mathematical result does not contain "7": ${result}`);
            }
        } else if (message.type === 'working') {
            console.log(`[${timestamp}] 🔄 INFO: AI is working on the task`);
        } else if (message.type === 'status') {
            console.log(`[${timestamp}] 📋 INFO: Received status message`);
        } else {
            console.log(`[${timestamp}] 📝 INFO: Received ${message.type} message`);
        }
    } catch (error) {
        console.log(`[${timestamp}] Received raw message:`, data.toString());
    }
});

// Connection error
ws.on('error', function(error) {
    console.log(`[${getTimestamp()}] ❌ WebSocket error:`, error.message);
});

// Connection closed
ws.on('close', function(code, reason) {
    console.log(`[${getTimestamp()}] Connection closed with code ${code}, reason: ${reason}`);
    
    if (!hasReceived) {
        console.log(`[${getTimestamp()}] ⚠️  WARNING: No messages received during test`);
    }
    
    process.exit(hasReceived ? 0 : 1);
});

// Exit after 20 seconds to allow for longer processing
setTimeout(() => {
    console.log(`[${getTimestamp()}] Test timeout reached (20 seconds), closing connection`);
    
    if (!hasReceived) {
        console.log(`[${getTimestamp()}] ❌ FAILURE: No responses received within timeout period`);
    }
    
    ws.close();
}, 20000);

// Handle process termination
process.on('SIGINT', () => {
    console.log(`[${getTimestamp()}] Received SIGINT, closing connection`);
    ws.close();
});

process.on('SIGTERM', () => {
    console.log(`[${getTimestamp()}] Received SIGTERM, closing connection`);
    ws.close();
});