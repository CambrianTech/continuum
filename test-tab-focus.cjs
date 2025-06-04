#!/usr/bin/env node

/**
 * Test Tab Focus Functionality
 * Simulates tab registration and tests focus behavior
 */

const WebSocket = require('ws');

async function testTabFocus() {
    console.log('🧪 Testing Tab Focus Functionality');
    console.log('=' .repeat(50));
    
    // Connect and register a tab
    const ws = new WebSocket('ws://localhost:5555');
    
    ws.on('open', () => {
        console.log('✅ Connected to Continuum');
        
        // Register tab
        const tabData = {
            type: 'tabRegister',
            tabId: 'test-focus-tab-' + Date.now(),
            version: '0.2.1877',
            url: 'http://localhost:5555',
            timestamp: new Date().toISOString()
        };
        
        console.log('📱 Registering tab:', tabData.tabId);
        ws.send(JSON.stringify(tabData));
        
        // Wait a moment, then simulate restart
        setTimeout(() => {
            console.log('\n🔄 Now try: continuum --restart');
            console.log('   It should focus this tab instead of opening new one');
            
            setTimeout(() => {
                ws.close();
                process.exit(0);
            }, 5000);
        }, 2000);
    });
    
    ws.on('message', (data) => {
        try {
            const message = JSON.parse(data);
            if (message.type === 'tabFocus') {
                console.log('🎯 Received focus command!');
            }
        } catch (e) {
            // Ignore non-JSON messages
        }
    });
}

testTabFocus();