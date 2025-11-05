// Auto-Repair System - Fix common JS issues automatically
console.log('🔧 Starting auto-repair sequence...');

const repairs = {
    applied: [],
    failed: []
};

// 1. Fix WebSocket Connection
try {
    if (typeof ws === 'undefined' || !ws || ws.readyState !== 1) {
        console.log('🔌 Repairing WebSocket connection...');
        
        // Reinitialize WebSocket if function exists
        if (typeof initWebSocket === 'function') {
            initWebSocket();
            repairs.applied.push('WebSocket reinitialized');
        } else {
            // Create new WebSocket connection
            const port = window.location.port || '9000';
            ws = new WebSocket(`ws://localhost:${port}`);
            
            ws.onopen = function() {
                console.log('✅ WebSocket repaired and connected');
            };
            
            ws.onmessage = function(event) {
                const data = JSON.parse(event.data);
                if (typeof handleWebSocketMessage === 'function') {
                    handleWebSocketMessage(data);
                }
            };
            
            repairs.applied.push('WebSocket recreated');
        }
    }
} catch (e) {
    repairs.failed.push(`WebSocket repair failed: ${e.message}`);
}

// 2. Fix Missing Global Variables
try {
    if (typeof roomMessages === 'undefined') {
        window.roomMessages = { general: [], academy: [] };
        repairs.applied.push('roomMessages initialized');
    }
    
    if (typeof selectedAgent === 'undefined') {
        window.selectedAgent = 'Auto Route';
        repairs.applied.push('selectedAgent initialized');
    }
    
    if (typeof currentRoom === 'undefined') {
        window.currentRoom = 'general';
        repairs.applied.push('currentRoom initialized');
    }
} catch (e) {
    repairs.failed.push(`Global variable repair failed: ${e.message}`);
}

// 3. Install Global Error Handler
try {
    if (!window.onerror) {
        window.onerror = function(message, source, lineno, colno, error) {
            console.error('🚨 Global JS Error:', {
                message, source, lineno, colno, error
            });
            
            // Auto-trigger repair if too many errors
            if (!window.errorCount) window.errorCount = 0;
            window.errorCount++;
            
            if (window.errorCount > 5) {
                console.log('🔄 Too many errors, triggering page reload...');
                setTimeout(() => location.reload(), 2000);
            }
            
            return true; // Prevent default error handling
        };
        
        repairs.applied.push('Global error handler installed');
    }
} catch (e) {
    repairs.failed.push(`Error handler repair failed: ${e.message}`);
}

// 4. Fix Missing DOM Elements
try {
    // Check and recreate critical elements if missing
    if (!document.getElementById('connection-status')) {
        const statusEl = document.createElement('div');
        statusEl.id = 'connection-status';
        statusEl.className = 'connection-status connected';
        statusEl.textContent = '🟢';
        document.body.appendChild(statusEl);
        repairs.applied.push('Connection status indicator recreated');
    }
    
    if (!document.getElementById('chat')) {
        const chatEl = document.createElement('div');
        chatEl.id = 'chat';
        chatEl.style.cssText = 'height: 400px; overflow-y: auto; border: 1px solid #333; padding: 10px;';
        document.body.appendChild(chatEl);
        repairs.applied.push('Chat container recreated');
    }
} catch (e) {
    repairs.failed.push(`DOM repair failed: ${e.message}`);
}

// 5. Memory Cleanup
try {
    // Clear old event listeners that might be causing leaks
    if (window.oldEventListeners) {
        window.oldEventListeners.forEach(listener => {
            try {
                document.removeEventListener(listener.type, listener.handler);
            } catch (e) {
                // Ignore cleanup errors
            }
        });
        window.oldEventListeners = [];
        repairs.applied.push('Event listeners cleaned up');
    }
    
    // Force garbage collection if available
    if (window.gc) {
        window.gc();
        repairs.applied.push('Manual garbage collection triggered');
    }
} catch (e) {
    repairs.failed.push(`Memory cleanup failed: ${e.message}`);
}

// 6. Fix CSS Issues
try {
    // Add critical styles if missing
    if (!document.getElementById('emergency-styles')) {
        const style = document.createElement('style');
        style.id = 'emergency-styles';
        style.textContent = `
            .connection-status {
                position: fixed;
                top: 10px;
                right: 10px;
                width: 20px;
                height: 20px;
                border-radius: 50%;
                background: #4CAF50;
                z-index: 9999;
            }
            .connection-status.disconnected {
                background: #f44336;
            }
            #chat {
                color: #e0e6ed;
                background: rgba(15, 20, 25, 0.8);
            }
        `;
        document.head.appendChild(style);
        repairs.applied.push('Emergency styles applied');
    }
} catch (e) {
    repairs.failed.push(`CSS repair failed: ${e.message}`);
}

// Report Results
console.log('🔧 AUTO-REPAIR RESULTS:');
console.log('========================');

if (repairs.applied.length > 0) {
    console.log('✅ REPAIRS APPLIED:');
    repairs.applied.forEach((repair, i) => console.log(`  ${i+1}. ${repair}`));
}

if (repairs.failed.length > 0) {
    console.error('❌ REPAIRS FAILED:');
    repairs.failed.forEach((failure, i) => console.error(`  ${i+1}. ${failure}`));
}

// Set repair status
window.repairResult = {
    status: repairs.failed.length === 0 ? 'success' : 'partial',
    appliedCount: repairs.applied.length,
    failedCount: repairs.failed.length,
    needsReload: repairs.failed.length > 2
};

if (window.repairResult.needsReload) {
    console.log('🔄 Major issues detected - page reload recommended');
    console.log('⏰ Automatic reload in 5 seconds... (cancel with clearTimeout(window.reloadTimer))');
    
    window.reloadTimer = setTimeout(() => {
        console.log('🔄 Auto-reloading page for complete repair...');
        location.reload();
    }, 5000);
}

console.log(`🛰️ Repair status: ${window.repairResult.status.toUpperCase()}`);

'auto_repair_complete'