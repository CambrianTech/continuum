#!/usr/bin/env npx tsx

/**
 * Test cursor pagination logic to understand the bug
 */

// Simulate messages
const messages = [
  { id: 1, timestamp: '2025-10-08T22:26:00Z', text: 'Newest' },
  { id: 2, timestamp: '2025-10-08T22:25:40Z', text: 'Message 2' },
  { id: 3, timestamp: '2025-10-08T22:25:30Z', text: 'Message 3' },
  { id: 4, timestamp: '2025-10-08T22:25:20Z', text: 'Message 4' },
  { id: 5, timestamp: '2025-10-08T22:25:10Z', text: 'Message 5' },
  { id: 6, timestamp: '2025-10-08T22:25:00Z', text: 'Message 6' },
  { id: 7, timestamp: '2025-10-08T22:24:50Z', text: 'Message 7' },
  { id: 8, timestamp: '2025-10-08T22:24:40Z', text: 'Oldest' }
];

console.log('🔍 Testing cursor pagination logic\n');

// First query: Get first 3 messages (DESC order)
console.log('Query 1: No cursor, limit 3, sort DESC');
const query1 = [...messages].sort((a, b) => b.timestamp.localeCompare(a.timestamp)).slice(0, 3);
console.log('Results:', query1.map(m => `${m.id}:${m.timestamp}`));
const cursor1 = query1[query1.length - 1].timestamp;
console.log('Cursor for next query:', cursor1);
console.log('');

// Second query: Get next 3 messages using cursor
console.log('Query 2: Cursor from query 1, limit 3, sort DESC, direction="before"');
const sorted = [...messages].sort((a, b) => b.timestamp.localeCompare(a.timestamp));
console.log('After sorting DESC:', sorted.map(m => `${m.id}:${m.timestamp}`));

const cursorFiltered = sorted.filter(m => m.timestamp < cursor1);
console.log('After cursor filter (timestamp < cursor):', cursorFiltered.map(m => `${m.id}:${m.timestamp}`));

const query2 = cursorFiltered.slice(0, 3);
console.log('After limit 3:', query2.map(m => `${m.id}:${m.timestamp}`));
console.log('');

// Check if query2 overlaps with query1
const overlaps = query2.some(m => query1.find(q => q.id === m.id));
console.log('Does query2 overlap with query1?', overlaps ? '❌ YES (BUG!)' : '✅ NO (correct)');
