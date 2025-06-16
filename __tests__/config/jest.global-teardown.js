/**
 * Jest Global Teardown
 * Runs after all tests
 */

module.exports = async () => {
  console.log('🧹 Cleaning up Jest test environment...');
  
  // Add any global cleanup logic here
  // For example, stopping test servers, cleaning up test data, etc.
  
  console.log('✅ Jest global teardown complete');
};