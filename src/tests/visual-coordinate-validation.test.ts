#!/usr/bin/env npx tsx
/**
 * Visual Coordinate Validation Test
 * 
 * Takes new screenshots with corrected coordinate calculation
 * to validate that text cutoff issues are resolved.
 */

import { jtag } from '../';

async function visualCoordinateValidation() {
  console.log('📐 VISUAL COORDINATE VALIDATION TEST');
  console.log('===================================');
  console.log('🎯 Validating corrected coordinate calculation fixes text cutoff');
  
  let client: any = null;
  
  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');
    
    // Test 1: Chat widget with corrected coordinates
    console.log('📸 Test 1: Chat widget with corrected coordinate calculation...');
    const chatResult = await client.commands.screenshot({
      querySelector: 'chat-widget',
      filename: `chat-widget-coordinate-fixed-${Date.now()}.png`,
      scale: 1.0, // Standard scale to see if text fits
      destination: 'file'
    });
    
    if (chatResult.success && chatResult.commandResult.commandResult?.success) {
      console.log(`✅ Chat widget screenshot: ${chatResult.commandResult.commandResult.filename}`);
      console.log('🔍 Check this screenshot for text cutoff - should be FIXED now');
    } else {
      console.log('❌ Chat widget screenshot failed');
    }
    
    // Test 2: High DPI version for detailed analysis
    console.log('📸 Test 2: High DPI chat widget for detailed coordinate analysis...');
    const highDpiResult = await client.commands.screenshot({
      querySelector: 'chat-widget',
      filename: `chat-widget-high-dpi-${Date.now()}.png`,
      scale: 2.0, // High DPI to see fine details
      destination: 'file'
    });
    
    if (highDpiResult.success && highDpiResult.commandResult.commandResult?.success) {
      console.log(`✅ High DPI screenshot: ${highDpiResult.commandResult.commandResult.filename}`);
      console.log('🔍 Check this screenshot for text completeness at 2x scale');
    } else {
      console.log('❌ High DPI screenshot failed');
    }
    
    // Test 3: Full body for comparison
    console.log('📸 Test 3: Full body screenshot for visual comparison...');
    const bodyResult = await client.commands.screenshot({
      querySelector: 'body',
      filename: `full-body-comparison-${Date.now()}.png`,
      scale: 1.0,
      destination: 'file'
    });
    
    if (bodyResult.success && bodyResult.commandResult.commandResult?.success) {
      console.log(`✅ Full body screenshot: ${bodyResult.commandResult.commandResult.filename}`);
      console.log('🔍 Use this to compare chat widget position vs full page');
    } else {
      console.log('❌ Full body screenshot failed');
    }
    
    console.log('');
    console.log('🎯 VISUAL VALIDATION COMPLETE');
    console.log('🔍 Check screenshots in: examples/test-bench/.continuum/jtag/currentUser/screenshots/');
    console.log('📐 Look for: complete text, no cutoff, proper widget boundaries');
    
    return true;
    
  } catch (error) {
    console.error('❌ Visual validation failed:', error);
    return false;
  } finally {
    if (client?.disconnect) {
      console.log('🔌 Disconnecting...');
      await client.disconnect();
    }
  }
}

// Execute if run directly
if (require.main === module) {
  visualCoordinateValidation().then(success => {
    process.exit(success ? 0 : 1);
  });
}

export { visualCoordinateValidation };