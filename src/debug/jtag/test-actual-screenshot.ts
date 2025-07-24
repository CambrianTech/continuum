#!/usr/bin/env tsx
/**
 * Actually take a screenshot and verify the file is created
 */

import puppeteer from 'puppeteer';
import { existsSync, statSync } from 'fs';

async function testActualScreenshot() {
  console.log('📸 Testing ACTUAL screenshot creation...');
  
  let browser: any;
  
  try {
    browser = await puppeteer.launch({ 
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    const page = await browser.newPage();
    await page.goto('http://localhost:9002', { 
      waitUntil: 'domcontentloaded',
      timeout: 15000 
    });
    
    // Wait for demo to be ready
    await page.waitForFunction(() => {
      const statusElements = document.querySelectorAll('.status.connected');
      return statusElements.length > 0;
    }, { timeout: 10000 });
    
    console.log('✅ Demo page loaded');
    
    // Check screenshots directory before
    const screenshotDir = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/screenshots';
    const beforeFiles = existsSync(screenshotDir) ? require('fs').readdirSync(screenshotDir) : [];
    console.log('📁 Screenshots before:', beforeFiles.length, 'files');
    
    // Click the screenshot button and wait for it to complete
    console.log('🖱️ Clicking screenshot button...');
    await page.click('button[onclick="testBrowserScreenshot()"]');
    
    // Wait longer for screenshot to be processed
    console.log('⏳ Waiting for screenshot to be created...');
    await page.waitForTimeout(5000);
    
    // Check if screenshot was created
    const afterFiles = existsSync(screenshotDir) ? require('fs').readdirSync(screenshotDir) : [];
    console.log('📁 Screenshots after:', afterFiles.length, 'files');
    
    if (afterFiles.length > beforeFiles.length) {
      const newFiles = afterFiles.filter((f: string) => !beforeFiles.includes(f));
      console.log('✅ New screenshot files created:', newFiles);
      
      // Show file details
      for (const file of newFiles) {
        const filePath = `${screenshotDir}/${file}`;
        const stats = statSync(filePath);
        console.log(`   📄 ${file}: ${stats.size} bytes, created ${stats.mtime}`);
      }
      
    } else {
      console.log('❌ No new screenshot files created');
      
      // Let's check server logs for any screenshot activity
      console.log('🔍 Checking server logs for screenshot activity...');
      const serverLogPath = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/logs/server-console-log.log';
      if (existsSync(serverLogPath)) {
        const logs = require('fs').readFileSync(serverLogPath, 'utf8');
        const screenshotLogs = logs.split('\n').filter((line: string) => 
          line.includes('screenshot') || line.includes('Screenshot') || line.includes('📸')
        );
        console.log('📋 Screenshot-related logs:', screenshotLogs.slice(-5));
      }
    }
    
  } catch (error: unknown) {
    console.error('❌ Test failed:', error instanceof Error ? error.message : String(error));
  } finally {
    if (browser) {
      await browser.close();
    }
  }
}

testActualScreenshot();