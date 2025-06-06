// Capture all console errors and warnings
console.log("🚨 CAPTURING ALL ERRORS AND WARNINGS:");

// Override console to capture errors
const originalError = console.error;
const originalWarn = console.warn;
const capturedErrors = [];
const capturedWarnings = [];

console.error = function(...args) {
    capturedErrors.push(args.join(' '));
    originalError.apply(console, args);
};

console.warn = function(...args) {
    capturedWarnings.push(args.join(' '));
    originalWarn.apply(console, args);
};

// Wait a moment then report what we captured
setTimeout(() => {
    console.log("📊 CAPTURED ERRORS:", capturedErrors.length);
    capturedErrors.forEach((err, i) => console.log(`  ${i+1}. ${err}`));
    
    console.log("⚠️ CAPTURED WARNINGS:", capturedWarnings.length);
    capturedWarnings.forEach((warn, i) => console.log(`  ${i+1}. ${warn}`));
    
    // Also check for any existing console entries
    console.log("🔍 CHECKING FOR EXISTING CONSOLE ERRORS...");
    
    // Restore original console methods
    console.error = originalError;
    console.warn = originalWarn;
}, 2000);