#!/usr/bin/env node
async function testBrowser() {
  const jtagModule = await import('./server-index');
  const jtag = await jtagModule.jtag.connect();
  console.log('✅ Connected!');
  
  try {
    const result = await jtag.commands.screenshot('browser-test');
    console.log('📸 Screenshot result:', result?.success ? 'SUCCESS' : 'FAILED');
    
    const clickResult = await jtag.commands.click('button, a, input');
    console.log('🖱️ Click result:', clickResult?.success ? 'SUCCESS' : 'FAILED');
  } catch (error) {
    console.log('❌ Error:', error.message);
  }
  
  process.exit(0);
}

testBrowser();