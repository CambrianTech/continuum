/**
 * Jest Global Setup
 * Runs before all tests
 */

module.exports = async () => {
  console.log('🔧 Setting up Jest test environment...');
  
  // Set test environment variables
  process.env.NODE_ENV = 'test';
  process.env.CONTINUUM_TEST_MODE = 'true';
  
  // Add any global setup logic here
  // For example, starting test servers, database setup, etc.
  
  console.log('✅ Jest global setup complete');
};