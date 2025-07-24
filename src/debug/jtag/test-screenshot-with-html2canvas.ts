#!/usr/bin/env tsx
/**
 * Test screenshot with html2canvas properly loaded
 */

import puppeteer from 'puppeteer';

async function testScreenshotWithHtml2Canvas() {
  console.log('📸 Testing screenshot with html2canvas loaded...');
  
  let browser: any;
  
  try {
    browser = await puppeteer.launch({ 
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    const page = await browser.newPage();
    
    // Navigate to demo
    console.log('🌐 Loading demo page...');
    await page.goto('http://localhost:9002', { 
      waitUntil: 'networkidle0',  // Wait for all network requests to finish
      timeout: 15000 
    });
    
    // Wait for JTAG to initialize
    console.log('⏳ Waiting for JTAG to initialize...');
    await page.waitForFunction(() => {
      return typeof (window as any).testBrowserScreenshot === 'function' &&
             typeof (window as any).html2canvas !== 'undefined';
    }, { timeout: 10000 });
    
    console.log('✅ Both JTAG and html2canvas are loaded');
    
    // Execute screenshot and wait for completion
    console.log('📸 Executing screenshot...');
    const result = await page.evaluate(async () => {
      console.log('🎯 Browser: Starting screenshot test...');
      
      try {
        // Call the screenshot function and wait for it to complete
        await (window as any).testBrowserScreenshot();
        return { success: true };
      } catch (error) {
        console.error('❌ Browser: Screenshot failed:', error);
        return { success: false, error: error instanceof Error ? error.message : String(error) };
      }
    });
    
    console.log('📸 Screenshot execution result:', result);
    
    // Wait for screenshot processing
    console.log('⏳ Waiting for screenshot processing...');
    await page.waitForTimeout(3000);
    
  } catch (error: unknown) {
    console.error('❌ Test error:', error instanceof Error ? error.message : String(error));
  } finally {
    if (browser) {
      await browser.close();
    }
  }
  
  // Check for screenshot files
  console.log('\n📁 Checking for screenshot files...');
  try {
    const fs = require('fs');
    const screenshotDir = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/screenshots';
    
    if (fs.existsSync(screenshotDir)) {
      const files = fs.readdirSync(screenshotDir);
      if (files.length > 0) {
        console.log('✅ Screenshot files found:');
        files.forEach((file: string) => {
          const stats = fs.statSync(`${screenshotDir}/${file}`);
          console.log(`   📄 ${file}: ${stats.size} bytes, ${stats.mtime}`);
        });
      } else {
        console.log('❌ No screenshot files found');
      }
    } else {
      console.log('❌ Screenshots directory does not exist');
    }
  } catch (error: unknown) {
    console.error('❌ Error checking screenshots:', error instanceof Error ? error.message : String(error));
  }
}

testScreenshotWithHtml2Canvas();