#!/usr/bin/env tsx
/**
 * Simple test: Open browser, click screenshot button, immediately check logs and files
 */

import puppeteer from 'puppeteer';

async function triggerAndCheck() {
  console.log('🔍 Testing: Click screenshot button and check results immediately');
  
  let browser: any;
  
  try {
    browser = await puppeteer.launch({ 
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    const page = await browser.newPage();
    await page.goto('http://localhost:9002', { waitUntil: 'domcontentloaded', timeout: 10000 });
    
    // Wait for JTAG to be ready
    await page.waitForFunction(() => {
      const statusElements = document.querySelectorAll('.status.connected');
      return statusElements.length > 0;
    }, { timeout: 5000 });
    
    console.log('✅ Page loaded and connected');
    
    // Click the browser screenshot button
    console.log('🖱️ Clicking browser screenshot button...');
    await page.click('button[onclick="testBrowserScreenshot()"]');
    
    // Wait a moment for the request to be sent
    await page.waitForTimeout(1000);
    
    console.log('📋 Checking logs immediately...');
    
  } catch (error: unknown) {
    console.error('❌ Test error:', error instanceof Error ? error.message : String(error));
  } finally {
    if (browser) {
      await browser.close();
    }
  }
  
  // Now check the logs after browser is closed
  console.log('\n🔍 Checking server logs for screenshot activity:');
  try {
    const fs = require('fs');
    const serverLogPath = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/logs/server-console-log.log';
    
    if (fs.existsSync(serverLogPath)) {
      const logs = fs.readFileSync(serverLogPath, 'utf8');
      const recentLogs = logs.split('\n').slice(-20);  // Last 20 lines
      const screenshotLogs = recentLogs.filter((line: string) => 
        line.includes('screenshot') || line.includes('Screenshot') || line.includes('📸') || 
        line.includes('commands') || line.includes('browser-demo-test')
      );
      
      if (screenshotLogs.length > 0) {
        console.log('✅ Found screenshot-related logs:');
        screenshotLogs.forEach((log: string) => console.log('   ' + log));
      } else {
        console.log('❌ No screenshot-related logs found');
        console.log('📋 Recent server logs:');
        recentLogs.slice(-5).forEach((log: string) => console.log('   ' + log));
      }
    }
  } catch (error: unknown) {
    console.error('❌ Error reading logs:', error instanceof Error ? error.message : String(error));
  }
  
  // Check browser logs too
  console.log('\n🔍 Checking browser logs:');
  try {
    const fs = require('fs');
    const browserLogPath = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/logs/browser-console-log.log';
    
    if (fs.existsSync(browserLogPath)) {
      const logs = fs.readFileSync(browserLogPath, 'utf8');
      const recentLogs = logs.split('\n').slice(-20);  // Last 20 lines
      const screenshotLogs = recentLogs.filter((line: string) => 
        line.includes('screenshot') || line.includes('Screenshot') || line.includes('📸') ||
        line.includes('browser-demo-test')
      );
      
      if (screenshotLogs.length > 0) {
        console.log('✅ Found screenshot-related browser logs:'); 
        screenshotLogs.forEach((log: string) => console.log('   ' + log));
      } else {
        console.log('❌ No screenshot-related browser logs found');
      }
    }
  } catch (error: unknown) {
    console.error('❌ Error reading browser logs:', error instanceof Error ? error.message : String(error));
  }
  
  // Check if any screenshot files were created
  console.log('\n📁 Checking for screenshot files:');
  try {
    const fs = require('fs');
    const screenshotDir = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/screenshots';
    
    if (fs.existsSync(screenshotDir)) {
      const files = fs.readdirSync(screenshotDir);
      if (files.length > 0) {
        console.log('✅ Screenshot files found:');
        files.forEach((file: string) => {
          const stats = fs.statSync(`${screenshotDir}/${file}`);
          console.log(`   📄 ${file}: ${stats.size} bytes`);
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

triggerAndCheck();