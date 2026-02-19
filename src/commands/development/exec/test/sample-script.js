// Sample JavaScript for exec command testing
console.log('🚀 Sample script is running!');

// Test basic functionality
const result = {
  message: 'Hello from exec command!',
  timestamp: new Date().toISOString(),
  calculation: 2 + 2,
  environment: typeof window !== 'undefined' ? 'browser' : 'server'
};

console.log('📊 Script result:', result);

// Return the result (this should be captured by exec)
return result;