#!/usr/bin/env node
/**
 * Trace WebSocket Screenshot - Deep dive into WebSocket message flow
 * ================================================================
 */

import { BrowserClientConnection } from './ClientConnection.js';

async function traceWebSocketScreenshot() {
    console.log("🔍 TRACING WEBSOCKET SCREENSHOT MESSAGE FLOW");
    console.log("=" * 50);
    
    const browser = new BrowserClientConnection();
    await browser.connect();
    
    // Set up comprehensive WebSocket message tracing
    const setupTracingJs = `
        console.log("🔧 Setting up WebSocket message tracing...");
        
        // Track all WebSocket activity
        window.websocketTrace = [];
        
        // Find the WebSocket connection
        const wsConnections = [];
        
        // Override WebSocket constructor to track new connections
        const OriginalWebSocket = window.WebSocket;
        window.WebSocket = function(url, protocols) {
            const ws = new OriginalWebSocket(url, protocols);
            wsConnections.push(ws);
            
            console.log("🔌 WebSocket connection tracked:", url);
            
            // Trace all message events
            ws.addEventListener('message', (event) => {
                const data = event.data;
                console.log("📥 WebSocket received:", data);
                
                window.websocketTrace.push({
                    type: 'received',
                    data: data,
                    timestamp: new Date().toISOString()
                });
                
                // Parse and check for screenshot commands
                try {
                    const parsed = JSON.parse(data);
                    if (parsed.task && parsed.task.includes('SCREENSHOT')) {
                        console.log("📸 SCREENSHOT COMMAND DETECTED!");
                        console.log("📋 Screenshot command details:", parsed);
                        
                        // This is where we should see the screenshot processing
                        window.websocketTrace.push({
                            type: 'screenshot_command',
                            command: parsed,
                            timestamp: new Date().toISOString()
                        });
                    }
                } catch (e) {
                    // Not JSON, could be plain text
                    if (data.includes('SCREENSHOT') || data.includes('screenshot')) {
                        console.log("📸 Screenshot command in plain text:", data);
                    }
                }
            });
            
            // Track outgoing messages too
            const originalSend = ws.send;
            ws.send = function(data) {
                console.log("📤 WebSocket sending:", data);
                window.websocketTrace.push({
                    type: 'sent',
                    data: data,
                    timestamp: new Date().toISOString()
                });
                return originalSend.call(this, data);
            };
            
            return ws;
        };
        
        // Also check if there are existing WebSocket connections
        if (window.ws || window.websocket || window.socket) {
            console.log("📡 Found existing WebSocket references");
        }
        
        console.log("✅ WebSocket tracing setup complete");
        "TRACING_READY";
    `;
    
    await browser.executeJs(setupTracingJs);
    
    console.log("\n📸 Now sending screenshot command and tracing the flow...");
    
    // Send screenshot command and trace what happens
    const screenshotTraceJs = `
        console.log("📸 Preparing for screenshot command trace...");
        
        // Clear the trace
        window.websocketTrace = [];
        
        // Wait for any incoming messages
        let messageCount = 0;
        const startTime = Date.now();
        
        const checkInterval = setInterval(() => {
            const elapsed = Date.now() - startTime;
            console.log(\`⏱️  Waiting for screenshot command... (\${elapsed}ms)\`);
            
            if (window.websocketTrace.length > messageCount) {
                console.log("📊 New WebSocket activity detected!");
                for (let i = messageCount; i < window.websocketTrace.length; i++) {
                    const trace = window.websocketTrace[i];
                    console.log(\`📋 [\${trace.type}] \${trace.data}\`);
                }
                messageCount = window.websocketTrace.length;
            }
            
            if (elapsed > 10000) { // 10 second timeout
                clearInterval(checkInterval);
                console.log("⏰ Trace timeout reached");
            }
        }, 500);
        
        "WAITING_FOR_SCREENSHOT_COMMAND";
    `;
    
    await browser.executeJs(screenshotTraceJs);
    
    // Now send the screenshot command via our WebSocket
    console.log("Sending screenshot command via WebSocket...");
    
    const taskMessage = {
        type: 'task',
        role: 'system',
        task: '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
    };
    
    browser.websocket.send(JSON.stringify(taskMessage));
    console.log("✅ Screenshot command sent");
    
    // Wait for trace results
    await new Promise(resolve => setTimeout(resolve, 5000));
    
    // Check trace results
    const getTraceResultsJs = `
        console.log("📊 Collecting WebSocket trace results...");
        
        const results = {
            totalTraces: window.websocketTrace ? window.websocketTrace.length : 0,
            traces: window.websocketTrace || [],
            screenshotCommandSeen: false,
            messageTypes: {}
        };
        
        if (window.websocketTrace) {
            window.websocketTrace.forEach(trace => {
                if (trace.data && trace.data.includes('SCREENSHOT')) {
                    results.screenshotCommandSeen = true;
                }
                
                results.messageTypes[trace.type] = (results.messageTypes[trace.type] || 0) + 1;
            });
        }
        
        console.log("📋 Trace analysis:", results);
        
        JSON.stringify(results);
    `;
    
    const traceResult = await browser.executeJs(getTraceResultsJs);
    
    if (traceResult.success) {
        try {
            const analysis = JSON.parse(traceResult.result);
            
            console.log("\n📊 WEBSOCKET TRACE ANALYSIS:");
            console.log(`Total traces: ${analysis.totalTraces}`);
            console.log(`Screenshot command seen: ${analysis.screenshotCommandSeen ? '✅' : '❌'}`);
            console.log(`Message types:`, analysis.messageTypes);
            
            if (analysis.totalTraces === 0) {
                console.log("\n❌ NO WEBSOCKET MESSAGES DETECTED");
                console.log("This indicates the WebSocket connection might not be working properly");
            } else if (!analysis.screenshotCommandSeen) {
                console.log("\n❌ SCREENSHOT COMMAND NOT RECEIVED BY BROWSER");
                console.log("The command is being sent but not reaching the browser WebSocket handler");
            } else {
                console.log("\n✅ SCREENSHOT COMMAND RECEIVED");
                console.log("The command is reaching the browser, issue is in processing");
            }
            
        } catch (e) {
            console.log("❌ Could not parse trace results");
        }
    }
    
    await browser.disconnect();
    
    console.log("\n🎯 NEXT STEP:");
    console.log("Based on this trace, we can determine exactly where the screenshot flow breaks");
}

traceWebSocketScreenshot().catch(console.error);