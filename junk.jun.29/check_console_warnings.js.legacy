/**
 * Check Console Warnings
 * Simple script to monitor console output for continuum initialization
 */

console.log('🔍 Checking for continuum initialization warnings...');

// Check if we see any of our warning messages
const warningMessages = [
    '🔧 CRITICAL: continuum-api.js starting initialization process...',
    '🚀 CRITICAL: initializeContinuum() called - browser API starting...',
    '✅ CRITICAL: window.continuum initialization completed successfully!',
    '✅ CRITICAL: continuum-api.js loaded and initialization scheduled'
];

// Monitor console for a few seconds
const originalWarn = console.warn;
const capturedWarnings = [];

console.warn = function(...args) {
    const message = args.join(' ');
    capturedWarnings.push(message);
    originalWarn.apply(console, arguments);
};

// Check current state
console.log('📊 Current state:');
console.log('  - window.continuum exists:', typeof window.continuum !== 'undefined');
console.log('  - initializeContinuum exists:', typeof initializeContinuum === 'function');
console.log('  - ScreenshotUtils exists:', typeof window.ScreenshotUtils !== 'undefined');

// Force a script reload to trigger initialization
console.log('🔄 Forcing script reload...');
const script = document.createElement('script');
script.src = '/src/ui/continuum-api.js?test=' + Date.now();
script.onload = function() {
    console.log('✅ Script loaded successfully');
    setTimeout(() => {
        console.log('📋 Captured warnings:', capturedWarnings.length);
        capturedWarnings.forEach(w => console.log('  WARNING:', w));
        
        console.log('📊 Final state:');
        console.log('  - window.continuum exists:', typeof window.continuum !== 'undefined');
        console.log('  - initializeContinuum exists:', typeof initializeContinuum === 'function');
        
        // Restore console
        console.warn = originalWarn;
    }, 1000);
};
script.onerror = function(error) {
    console.error('❌ Script load failed:', error);
    console.warn = originalWarn;
};

document.head.appendChild(script);