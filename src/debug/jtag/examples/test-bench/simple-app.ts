#!/usr/bin/env npx tsx

// ONE LINE - that's it! Console auto-attached, full API available
import { jtag } from '../server-index';

// Your normal app code - console automatically logged
console.log('Hello from simple app!');
console.error('Test error message');

// Use JTAG API directly when needed  
jtag.critical('APP', 'Application started', { version: '1.0.0' });
jtag.log('STARTUP', 'Initializing components');

// Normal app logic
const startTime = Date.now();

function doWork() {
    console.log('Doing some work...');
    jtag.probe('PERFORMANCE', 'work_duration', Date.now() - startTime);
}

// Demonstrate screenshot (server-side)
jtag.screenshot('app-startup.png').then(result => {
    if (result.success) {
        jtag.log('SCREENSHOT', 'Startup screenshot saved', { filename: result.filename });
    } else {
        jtag.error('SCREENSHOT', 'Failed to take screenshot', { error: result.error });
    }
});

// Your app continues normally...
setTimeout(doWork, 2000);

console.log('Simple app running - all console output logged automatically!');