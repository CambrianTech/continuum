import { JTAGClient } from './system/core/client/shared/JTAGClient';

async function testSessionCreate() {
  console.log('Testing session/create command directly...');
  
  try {
    const jtag = await JTAGClient.connect({ target: 'server' });
    console.log('JTAGClient connected');
    
    const result = await jtag.commands['session/create']({
      context: { uuid: 'test-session', environment: 'server' },
      sessionId: 'test-session',
      category: 'user' as const,
      displayName: 'Test Session',
      isShared: true
    });
    
    console.log('Session create result:', JSON.stringify(result, null, 2));
    
  } catch (error) {
    console.error('Session create test failed:', error);
  }
}

testSessionCreate();