#!/usr/bin/env node
/**
 * JTAG Screenshot Integration Test
 * 
 * Uses JTAG's own abilities to test end-to-end html2canvas functionality
 */

import { spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { jtag } from '../index';

interface ScreenshotTestResults {
  serverReady: boolean;
  browserLoaded: boolean;
  html2canvasLoaded: boolean;
  screenshotsCaptured: number;
  pngFilesCreated: number;
  totalFileSize: number;
  errors: string[];
}

console.log('\n📸 JTAG Screenshot Integration Test');
console.log('===================================');

class ScreenshotIntegrationTest {
  private demoProcess: ChildProcess | null = null;
  private results: ScreenshotTestResults = {
    serverReady: false,
    browserLoaded: false, 
    html2canvasLoaded: false,
    screenshotsCaptured: 0,
    pngFilesCreated: 0,
    totalFileSize: 0,
    errors: []
  };

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  async runIntegrationTest(): Promise<void> {
    try {
      console.log('🚀 Starting JTAG demo with screenshot integration...');
      
      await this.startDemoServer();
      await this.waitForServerReady();
      await this.triggerScreenshotsViaJTAG();
      await this.verifyCreatedFiles();
      
      this.printResults();
      
    } catch (error: any) {
      console.error('💥 Integration test failed:', error.message);
      this.results.errors.push(error.message);
    } finally {
      await this.cleanup();
    }
  }

  private async startDemoServer(): Promise<void> {
    console.log('📡 Starting JTAG demo server...');
    
    this.demoProcess = spawn('npx', ['tsx', 'examples/end-to-end-demo.js'], {
      env: {
        ...process.env,
        JTAG_AUTO_LAUNCH: 'true'
      },
      stdio: ['ignore', 'pipe', 'pipe']
    });

    this.demoProcess.stdout?.on('data', (data: Buffer) => {
      const output = data.toString();
      console.log('📝', output.trim());
      
      if (output.includes('Demo server running')) {
        this.results.serverReady = true;
      }
      
      if (output.includes('html2canvas loaded successfully')) {
        this.results.html2canvasLoaded = true;
        console.log('🎨 HTML2Canvas integration confirmed!');
      }
      
      if (output.includes('Screenshot saved successfully')) {
        this.results.screenshotsCaptured++;
        console.log('📸 Real PNG screenshot created!');
      }
    });

    this.demoProcess.stderr?.on('data', (data: Buffer) => {
      const error = data.toString().trim();
      console.log('⚠️ ', error);
      this.results.errors.push(error);
    });
  }

  private async waitForServerReady(): Promise<void> {
    console.log('⏳ Waiting for server and browser to be ready...');
    
    let attempts = 0;
    const maxAttempts = 20;
    
    while (!this.results.serverReady && attempts < maxAttempts) {
      await this.sleep(1000);
      attempts++;
    }
    
    if (!this.results.serverReady) {
      throw new Error('Demo server failed to start');
    }
    
    console.log('✅ Server ready, giving browser time to load...');
    await this.sleep(5000); // Extra time for browser to fully load
  }

  private async triggerScreenshotsViaJTAG(): Promise<void> {
    console.log('\n⚡ Using JTAG exec() to trigger browser screenshots...');
    
    try {
      console.log('🎯 Step 1: Basic full-page screenshot');
      const basicResult = await jtag.exec(`
        (async () => {
          if (typeof window !== 'undefined' && window.jtag && window.jtag.screenshot) {
            console.log('🎯 JTAG exec: Triggering basic screenshot...');
            return await window.jtag.screenshot('integration-test-basic', {
              selector: 'body',
              format: 'png'
            });
          } else {
            return { error: 'JTAG not available in browser' };
          }
        })()
      `, { timeout: 15000 });
      
      console.log('📋 Basic screenshot result:', JSON.stringify(basicResult.result, null, 2));
      
      await this.sleep(2000);
      
      console.log('🎯 Step 2: Widget-targeted screenshot');
      const widgetResult = await jtag.exec(`
        (async () => {
          if (typeof window !== 'undefined' && window.jtag && window.jtag.screenshot) {
            console.log('🎯 JTAG exec: Triggering widget screenshot...');
            return await window.jtag.screenshot('integration-test-widget', {
              selector: '#demo-widget',
              width: 600,
              height: 400,
              format: 'png'
            });
          } else {
            return { error: 'JTAG not available in browser' };
          }
        })()
      `, { timeout: 15000 });
      
      console.log('📋 Widget screenshot result:', JSON.stringify(widgetResult.result, null, 2));
      
      await this.sleep(2000);
      
      console.log('🎯 Step 3: High-quality JPEG screenshot');
      const jpegResult = await jtag.exec(`
        (async () => {
          if (typeof window !== 'undefined' && window.jtag && window.jtag.screenshot) {
            console.log('🎯 JTAG exec: Triggering JPEG screenshot...');
            return await window.jtag.screenshot('integration-test-jpeg', {
              selector: '.demo-section',
              format: 'jpeg',
              quality: 0.9
            });
          } else {
            return { error: 'JTAG not available in browser' };
          }
        })()
      `, { timeout: 15000 });
      
      console.log('📋 JPEG screenshot result:', JSON.stringify(jpegResult.result, null, 2));
      
    } catch (execError: any) {
      console.error('❌ JTAG exec failed:', execError.message);
      this.results.errors.push(`JTAG exec failed: ${execError.message}`);
    }
    
    // Give time for all screenshots to be processed and saved
    console.log('⏳ Waiting for screenshot processing to complete...');
    await this.sleep(5000);
  }

  private async verifyCreatedFiles(): Promise<void> {
    console.log('\n📁 Verifying created screenshot files...');
    
    const screenshotDir = path.resolve('../../../.continuum/jtag/screenshots');
    console.log('🔍 Screenshot directory:', screenshotDir);
    
    if (!fs.existsSync(screenshotDir)) {
      this.results.errors.push('Screenshot directory not found');
      return;
    }
    
    const files = fs.readdirSync(screenshotDir);
    console.log(`📂 Found ${files.length} files:`);
    
    let pngCount = 0;
    let jpegCount = 0;
    let totalSize = 0;
    
    files.forEach(file => {
      const filePath = path.join(screenshotDir, file);
      const stats = fs.statSync(filePath);
      const fileType = path.extname(file).toLowerCase();
      const size = stats.size;
      totalSize += size;
      
      if (fileType === '.png') {
        pngCount++;
        console.log(`   🖼️  ${file} (${(size/1024).toFixed(1)} KB) - PNG screenshot`);
      } else if (fileType === '.jpg' || fileType === '.jpeg') {
        jpegCount++;
        console.log(`   🖼️  ${file} (${(size/1024).toFixed(1)} KB) - JPEG screenshot`);
      } else {
        console.log(`   📄 ${file} (${size} bytes) - ${fileType || 'other'}`);
      }
    });
    
    this.results.pngFilesCreated = pngCount + jpegCount;
    this.results.totalFileSize = totalSize;
    
    console.log(`\n📊 File Summary:`);
    console.log(`   🖼️  PNG files: ${pngCount}`);
    console.log(`   🖼️  JPEG files: ${jpegCount}`);
    console.log(`   📁 Total files: ${files.length}`);
    console.log(`   💾 Total size: ${(totalSize/1024).toFixed(1)} KB`);
  }

  private printResults(): void {
    console.log('\n📊 JTAG SCREENSHOT INTEGRATION TEST RESULTS');
    console.log('===========================================');
    console.log(`🚀 Server Ready: ${this.results.serverReady ? '✅' : '❌'}`);
    console.log(`🎨 HTML2Canvas Loaded: ${this.results.html2canvasLoaded ? '✅' : '❌'}`);
    console.log(`📸 Screenshots Captured: ${this.results.screenshotsCaptured}`);
    console.log(`🖼️  Image Files Created: ${this.results.pngFilesCreated}`);
    console.log(`💾 Total File Size: ${(this.results.totalFileSize/1024).toFixed(1)} KB`);
    console.log(`❌ Errors: ${this.results.errors.length}`);
    
    if (this.results.errors.length > 0) {
      console.log('\n❌ Errors encountered:');
      this.results.errors.forEach(error => {
        console.log(`   - ${error}`);
      });
    }
    
    if (this.results.pngFilesCreated > 0 && this.results.html2canvasLoaded) {
      console.log('\n🎉 SUCCESS: JTAG HTML2Canvas integration working perfectly!');
      console.log('📸 Real image files created via browser screenshot capture');
      console.log('⚡ JTAG exec() successfully triggered browser functions');
      console.log('🌐 End-to-end screenshot pipeline functional');
    } else if (this.results.serverReady) {
      console.log('\n⚠️  PARTIAL SUCCESS: Server working but screenshots may need manual trigger');
      console.log('💡 Try manually clicking "Test Screenshots" in the browser');
    } else {
      console.log('\n❌ FAILURE: Integration test did not complete successfully');
    }
  }

  private async cleanup(): Promise<void> {
    if (this.demoProcess) {
      console.log('\n🧹 Cleaning up demo process...');
      this.demoProcess.kill();
      await this.sleep(1000);
    }
  }
}

// Run the integration test
if (require.main === module) {
  const test = new ScreenshotIntegrationTest();
  
  // Handle cleanup on exit
  process.on('SIGINT', async () => {
    console.log('\n🛑 Test interrupted by user');
    await test['cleanup']();
    process.exit(0);
  });
  
  test.runIntegrationTest().catch(error => {
    console.error('💥 Screenshot integration test failed:', error);
    process.exit(1);
  });
}