// Full System Health Check - Deep Space Probe Diagnostics
console.log('🛰️ Starting deep space probe system diagnostics...');

const diagnostics = {
    errors: [],
    warnings: [],
    info: [],
    fixes: []
};

// 1. Check WebSocket Connection
try {
    if (typeof ws !== 'undefined' && ws) {
        diagnostics.info.push(`WebSocket state: ${ws.readyState} (${ws.readyState === 1 ? 'OPEN' : 'CLOSED/CONNECTING'})`);
        if (ws.readyState !== 1) {
            diagnostics.errors.push('WebSocket not in OPEN state');
            diagnostics.fixes.push('Restart WebSocket connection');
        }
    } else {
        diagnostics.errors.push('WebSocket not initialized');
        diagnostics.fixes.push('Reinitialize WebSocket');
    }
} catch (e) {
    diagnostics.errors.push(`WebSocket check failed: ${e.message}`);
}

// 2. Check for JavaScript Errors
try {
    // Check for common undefined variables
    const checkVars = ['roomMessages', 'selectedAgent', 'currentRoom'];
    checkVars.forEach(varName => {
        if (typeof window[varName] === 'undefined') {
            diagnostics.warnings.push(`Global variable ${varName} is undefined`);
        }
    });
    
    // Check for error event listeners
    if (!window.onerror) {
        diagnostics.warnings.push('No global error handler registered');
        diagnostics.fixes.push('Install global error handler');
    }
} catch (e) {
    diagnostics.errors.push(`Variable check failed: ${e.message}`);
}

// 3. Check DOM Elements
try {
    const criticalElements = ['#chat', '#connection-status', '.agent-list'];
    criticalElements.forEach(selector => {
        if (!document.querySelector(selector)) {
            diagnostics.errors.push(`Critical element missing: ${selector}`);
            diagnostics.fixes.push(`Regenerate UI component: ${selector}`);
        }
    });
} catch (e) {
    diagnostics.errors.push(`DOM check failed: ${e.message}`);
}

// 4. Check Console for Previous Errors
try {
    // Intercept console.error to catch new errors
    const originalError = console.error;
    let errorCount = 0;
    console.error = function(...args) {
        errorCount++;
        diagnostics.errors.push(`Console Error ${errorCount}: ${args.join(' ')}`);
        originalError.apply(console, args);
    };
    
    diagnostics.info.push('Error interceptor installed');
} catch (e) {
    diagnostics.errors.push(`Error interceptor failed: ${e.message}`);
}

// 5. Memory and Performance Check
try {
    if (performance.memory) {
        const mem = performance.memory;
        const memUsed = (mem.usedJSHeapSize / 1048576).toFixed(2);
        const memLimit = (mem.jsHeapSizeLimit / 1048576).toFixed(2);
        
        diagnostics.info.push(`Memory: ${memUsed}MB / ${memLimit}MB`);
        
        if (mem.usedJSHeapSize / mem.jsHeapSizeLimit > 0.8) {
            diagnostics.warnings.push('High memory usage detected');
            diagnostics.fixes.push('Reload page to clear memory');
        }
    }
} catch (e) {
    diagnostics.warnings.push(`Memory check unavailable: ${e.message}`);
}

// 6. Check for Stale Event Listeners
try {
    const listenerCount = Object.keys(window).filter(key => key.startsWith('on')).length;
    diagnostics.info.push(`Event listeners registered: ${listenerCount}`);
} catch (e) {
    diagnostics.warnings.push(`Event listener check failed: ${e.message}`);
}

// Report Results
console.log('🔍 DIAGNOSTIC RESULTS:');
console.log('=====================');

if (diagnostics.errors.length > 0) {
    console.error('❌ ERRORS FOUND:');
    diagnostics.errors.forEach((error, i) => console.error(`  ${i+1}. ${error}`));
}

if (diagnostics.warnings.length > 0) {
    console.warn('⚠️ WARNINGS:');
    diagnostics.warnings.forEach((warning, i) => console.warn(`  ${i+1}. ${warning}`));
}

if (diagnostics.info.length > 0) {
    console.log('ℹ️ SYSTEM INFO:');
    diagnostics.info.forEach((info, i) => console.log(`  ${i+1}. ${info}`));
}

if (diagnostics.fixes.length > 0) {
    console.log('🔧 RECOMMENDED FIXES:');
    diagnostics.fixes.forEach((fix, i) => console.log(`  ${i+1}. ${fix}`));
}

// Auto-fix capability flag
window.diagnosticsResult = {
    status: diagnostics.errors.length === 0 ? 'healthy' : 'needs_repair',
    errorCount: diagnostics.errors.length,
    warningCount: diagnostics.warnings.length,
    fixes: diagnostics.fixes,
    canAutoFix: diagnostics.fixes.length > 0
};

console.log(`🛰️ Probe status: ${window.diagnosticsResult.status.toUpperCase()}`);

'diagnostics_complete'