/**
 * Debug IPC - Check if IPC channel exists
 */

import { spawn } from 'child_process';

const child = spawn('node', ['-e', `
  console.log('Child process started');
  console.log('Has IPC channel:', !!process.send);
  console.log('Connected:', process.connected);
  
  process.on('message', (msg) => {
    console.log('Received:', msg);
    process.send({ echo: msg });
  });
  
  setTimeout(() => {
    console.log('Child still alive after 5s');
  }, 5000);
`], {
  stdio: ['pipe', 'pipe', 'pipe', 'ipc']
});

child.stdout?.on('data', (data) => {
  console.log('Child stdout:', data.toString().trim());
});

child.stderr?.on('data', (data) => {
  console.log('Child stderr:', data.toString().trim());
});

child.on('message', (msg) => {
  console.log('Parent received:', msg);
  child.kill();
});

setTimeout(() => {
  console.log('Sending test message...');
  child.send?.({ test: 'hello' });
}, 2000);

setTimeout(() => {
  console.log('Timeout, killing child');
  child.kill();
}, 8000);