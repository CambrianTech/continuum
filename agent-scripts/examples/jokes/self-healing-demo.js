// Demonstrate full self-healing pipeline
console.log('🛰️ DEEP SPACE PROBE - Self-Healing Demo');
console.log('=========================================');

// 1. Send a joke
const jokes = [
  'Why do programmers prefer dark mode? Because light attracts bugs! 🐛',
  'How many programmers does it take to change a light bulb? None, that\'s a hardware problem! 💡',
  'Why don\'t programmers like nature? It has too many bugs! 🌿🐛'
];

const joke = jokes[Math.floor(Math.random() * jokes.length)];
console.log('😄 JOKE OF THE DAY:', joke);

// 2. Check system health
const health = {
  websocket: typeof ws !== 'undefined' && ws && ws.readyState === 1,
  dom: document.getElementById('chat') !== null,
  globals: typeof roomMessages !== 'undefined'
};

console.log('🔍 SYSTEM HEALTH:', health);

// 3. Auto-fix any issues found
let fixes = [];
if (!health.websocket) fixes.push('WebSocket needs repair');
if (!health.dom) fixes.push('DOM elements missing');
if (!health.globals) fixes.push('Global variables missing');

if (fixes.length > 0) {
  console.log('🔧 ISSUES DETECTED:', fixes);
  console.log('🛰️ Auto-healing would activate here');
} else {
  console.log('✅ ALL SYSTEMS OPERATIONAL');
}

// 4. Demonstrate probe telemetry
console.log('📡 PROBE TELEMETRY:');
console.log('  - Location:', window.location.href);
console.log('  - User Agent:', navigator.userAgent.substring(0, 50) + '...');
console.log('  - Memory:', performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) + 'MB' : 'Unknown');
console.log('  - Timestamp:', new Date().toISOString());

'self_healing_demo_complete'