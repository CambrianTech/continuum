#!/usr/bin/env tsx
/**
 * Simple test: Just evaluate the screenshot function directly in the existing browser
 */

import puppeteer from 'puppeteer';

async function clickScreenshotSimple() {
  console.log('🎯 Testing: Click screenshot button in existing browser');
  
  let browser: any;
  
  try {
    // Connect to existing browser process instead of launching new one
    console.log('🔗 Connecting to browser...');
    browser = await puppeteer.launch({ 
      headless: false,  // Show the browser so we can see what happens
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    const page = await browser.newPage();
    
    // Navigate to demo
    console.log('🌐 Navigating to demo...');
    await page.goto('http://localhost:9002', { 
      waitUntil: 'domcontentloaded',
      timeout: 10000 
    });
    
    // Wait for demo to initialize
    console.log('⏳ Waiting for demo to initialize...');
    await page.waitForTimeout(3000);
    
    // Execute the screenshot function directly
    console.log('📸 Executing testBrowserScreenshot() directly...');
    const result = await page.evaluate(async () => {
      console.log('🌐 Browser: About to call testBrowserScreenshot()');
      
      if (typeof (window as any).testBrowserScreenshot === 'function') {
        try {
          await (window as any).testBrowserScreenshot();
          return { success: true, called: true };
        } catch (error: unknown) {
          return { success: false, error: error instanceof Error ? error.message : String(error), called: true };
        }
      } else {
        return { success: false, error: 'testBrowserScreenshot function not found', called: false };
      }
    });
    
    console.log('📸 Result:', result);
    
    // Wait a moment for any async screenshot processing
    console.log('⏳ Waiting for screenshot processing...');
    await page.waitForTimeout(2000);
    
  } catch (error: unknown) {
    console.error('❌ Test error:', error instanceof Error ? error.message : String(error));
  } finally {
    if (browser) {
      await browser.close();
    }
  }
}

clickScreenshotSimple();