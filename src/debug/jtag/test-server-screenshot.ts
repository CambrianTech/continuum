#!/usr/bin/env tsx
/**
 * Direct server-side screenshot save test
 */

import * as fs from 'fs/promises';
import * as path from 'path';

async function testServerScreenshotSave() {
  console.log('📸 Testing server-side screenshot save functionality...');
  
  // Create a fake dataURL (represents what browser would send)
  const fakeDataUrl = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAI9jU77zgAAAABJRU5ErkJggg==';
  
  try {
    // Test save functionality
    const screenshotDir = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/screenshots';
    
    // Ensure directory exists
    await fs.mkdir(screenshotDir, { recursive: true });
    
    // Save file
    const filename = `server-test-${Date.now()}.png`;
    const filepath = path.join(screenshotDir, filename);
    
    // Convert dataURL to buffer
    const base64Data = fakeDataUrl.replace(/^data:image\/\w+;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    
    await fs.writeFile(filepath, buffer);
    
    // Verify file exists
    const stats = await fs.stat(filepath);
    console.log(`✅ Server screenshot save works: ${filename} (${stats.size} bytes)`);
    
    // Show file location
    console.log(`📁 File saved at: ${filepath}`);
    
    return { success: true, filepath, filename, size: stats.size };
    
  } catch (error: any) {
    console.error('❌ Server screenshot save failed:', error.message);
    return { success: false, error: error.message };
  }
}

testServerScreenshotSave();