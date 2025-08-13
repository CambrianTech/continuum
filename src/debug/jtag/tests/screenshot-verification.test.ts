#!/usr/bin/env npx tsx

/**
 * 🎯 CRITICAL SCREENSHOT VERIFICATION TEST
 * 
 * This test MUST fail if screenshots are not properly saved.
 * User requirement: "make sure npm test works end to end and saves all screenshots successfully, otherwise it should fail this test"
 */

import * as fs from 'fs';
import * as path from 'path';

interface ScreenshotVerificationResult {
  success: boolean;
  screenshotsSaved: number;
  expectedScreenshots: number;
  missingScreenshots: string[];
  fileSizeErrors: string[];
  details: string[];
}

async function verifyScreenshotEndToEnd(): Promise<ScreenshotVerificationResult> {
  console.log('🎯 CRITICAL VERIFICATION: Screenshot end-to-end test with failure detection');
  
  const result: ScreenshotVerificationResult = {
    success: false,
    screenshotsSaved: 0,
    expectedScreenshots: 1, // At minimum, we expect the browser-demo-test.png
    missingScreenshots: [],
    fileSizeErrors: [],
    details: []
  };
  
  try {
    // 1. Find current user session directory
    const currentUserPath = 'examples/test-bench/.continuum/jtag/currentUser';
    
    let sessionPath: string;
    try {
      const stats = fs.lstatSync(currentUserPath);
      if (stats.isSymbolicLink()) {
        sessionPath = fs.readlinkSync(currentUserPath);
        // Convert relative path to absolute
        if (!path.isAbsolute(sessionPath)) {
          sessionPath = path.resolve(path.dirname(currentUserPath), sessionPath);
        }
        result.details.push(`✅ Found current session via symlink: ${sessionPath}`);
      } else {
        throw new Error('currentUser is not a symlink');
      }
    } catch (error) {
      // Fallback: find latest session
      const sessionsDir = 'examples/test-bench/.continuum/jtag/sessions/user';
      const sessions = fs.readdirSync(sessionsDir);
      if (sessions.length === 0) {
        throw new Error('No user sessions found');
      }
      // Get the most recent session
      sessionPath = path.join(sessionsDir, sessions[sessions.length - 1]);
      result.details.push(`⚠️ Using latest session (fallback): ${sessionPath}`);
    }
    
    // 2. Check screenshots directory
    const screenshotsPath = path.join(sessionPath, 'screenshots');
    
    if (!fs.existsSync(screenshotsPath)) {
      result.missingScreenshots.push('screenshots directory does not exist');
      result.details.push(`❌ CRITICAL: Screenshots directory missing: ${screenshotsPath}`);
      return result;
    }
    
    result.details.push(`✅ Screenshots directory found: ${screenshotsPath}`);
    
    // 3. List and verify all screenshots
    const screenshots = fs.readdirSync(screenshotsPath).filter(f => f.endsWith('.png'));
    result.screenshotsSaved = screenshots.length;
    
    result.details.push(`📊 Found ${screenshots.length} screenshot files`);
    
    for (const screenshot of screenshots) {
      const screenshotPath = path.join(screenshotsPath, screenshot);
      const stats = fs.statSync(screenshotPath);
      
      result.details.push(`📸 ${screenshot}: ${stats.size} bytes`);
      
      // Verify file size (screenshots should be at least 1KB for real captures)
      if (stats.size < 1024) {
        result.fileSizeErrors.push(`${screenshot} (${stats.size} bytes) - too small, likely failed`);
        result.details.push(`❌ ${screenshot}: File too small (${stats.size} bytes) - likely capture failed`);
      } else {
        result.details.push(`✅ ${screenshot}: Valid size (${stats.size} bytes)`);
      }
    }
    
    // 4. Expected screenshots check
    const expectedScreenshots = [
      'browser-demo-test.png' // From the browser integration test
    ];
    
    for (const expected of expectedScreenshots) {
      if (!screenshots.includes(expected)) {
        result.missingScreenshots.push(expected);
        result.details.push(`❌ MISSING: Expected screenshot ${expected} not found`);
      }
    }
    
    // 5. Determine overall success
    const hasMinimumScreenshots = result.screenshotsSaved >= result.expectedScreenshots;
    const noMissingCritical = result.missingScreenshots.length === 0;
    const noSizeErrors = result.fileSizeErrors.length === 0;
    
    result.success = hasMinimumScreenshots && noMissingCritical && noSizeErrors;
    
    if (result.success) {
      result.details.push(`🎉 SUCCESS: All ${result.screenshotsSaved} screenshots saved successfully`);
    } else {
      result.details.push(`❌ FAILURE: Screenshots not saved properly`);
      if (!hasMinimumScreenshots) {
        result.details.push(`❌ Expected at least ${result.expectedScreenshots} screenshots, got ${result.screenshotsSaved}`);
      }
      if (result.missingScreenshots.length > 0) {
        result.details.push(`❌ Missing screenshots: ${result.missingScreenshots.join(', ')}`);
      }
      if (result.fileSizeErrors.length > 0) {
        result.details.push(`❌ File size errors: ${result.fileSizeErrors.join(', ')}`);
      }
    }
    
  } catch (error) {
    result.details.push(`💥 CRITICAL ERROR: ${error.message}`);
    result.success = false;
  }
  
  return result;
}

async function main() {
  console.log('🧪 SCREENSHOT VERIFICATION TEST - CRITICAL FOR npm test SUCCESS');
  console.log('📋 This test ensures screenshots are actually saved to disk');
  console.log('');
  
  const result = await verifyScreenshotEndToEnd();
  
  // Print detailed results
  for (const detail of result.details) {
    console.log(detail);
  }
  
  console.log('');
  console.log('📊 VERIFICATION SUMMARY:');
  console.log(`   Screenshots Saved: ${result.screenshotsSaved}/${result.expectedScreenshots}`);
  console.log(`   Missing Screenshots: ${result.missingScreenshots.length}`);
  console.log(`   File Size Errors: ${result.fileSizeErrors.length}`);
  console.log(`   Overall Success: ${result.success ? '✅ PASS' : '❌ FAIL'}`);
  
  if (result.success) {
    console.log('');
    console.log('🎉 ✅ SCREENSHOT VERIFICATION PASSED');
    console.log('📸 All screenshots saved successfully to disk');
    process.exit(0);
  } else {
    console.log('');
    console.log('💥 ❌ SCREENSHOT VERIFICATION FAILED');
    console.log('🚨 npm test MUST FAIL - screenshots not properly saved');
    console.log('');
    console.log('FAILURE DETAILS:');
    if (result.missingScreenshots.length > 0) {
      console.log(`   Missing: ${result.missingScreenshots.join(', ')}`);
    }
    if (result.fileSizeErrors.length > 0) {
      console.log(`   Size Errors: ${result.fileSizeErrors.join(', ')}`);
    }
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(error => {
    console.error('💥 Screenshot verification test crashed:', error);
    process.exit(1);
  });
}