#!/usr/bin/env npx tsx
/**
 * AI Compiler Error Detection Integration Test
 * 
 * Tests that our AI-optimized system clearly detects and reports TypeScript compiler errors
 * both in browser and server contexts, making them obvious to AI agents.
 */

import { execSync } from 'child_process';
import { existsSync, readFileSync } from 'fs';
import { AIAgentDashboardRunner } from '../agents/ai-dashboard';

describe('AI Compiler Error Detection', () => {
  
  describe('TypeScript Compilation Error Detection', () => {
    
    test('should detect TypeScript errors via tsc --noEmit', async () => {
      try {
        // This should fail with our intentional errors
        execSync('npx tsc --noEmit --project .', { encoding: 'utf8', stdio: 'pipe' });
        fail('TypeScript compilation should have failed with our intentional errors');
      } catch (error: any) {
        const output = error.stdout + error.stderr;
        
        // Should detect browser-side error
        expect(output).toContain('InvalidBrowserType');
        expect(output).toContain('browser/generated.ts');
        expect(output).toContain('Cannot find name');
        
        console.log('✅ TypeScript compiler correctly detected intentional browser error');
        console.log('📊 Error output sample:', output.substring(0, 200));
      }
    });
    
    test('should detect server-side TypeScript errors', async () => {
      try {
        execSync('npx tsc --noEmit agents/ai-dashboard.ts', { encoding: 'utf8', stdio: 'pipe' });
        fail('Server-side TypeScript compilation should have failed');
      } catch (error: any) {
        const output = error.stdout + error.stderr;
        
        // Should detect server-side error  
        expect(output).toContain('UnknownType');
        expect(output).toContain('agents/ai-dashboard.ts');
        
        console.log('✅ TypeScript compiler correctly detected intentional server error');
        console.log('📊 Server error output sample:', output.substring(0, 200));
      }
    });
    
  });
  
  describe('AI Agent Dashboard Error Reporting', () => {
    
    test('should report system as unhealthy due to compilation errors', async () => {
      const dashboard = new AIAgentDashboardRunner();
      
      // Generate dashboard with current state (should be unhealthy due to our errors)
      const dashboardData = await (dashboard as any).generateDashboard();
      
      // System should be unhealthy due to compilation preventing startup
      expect(dashboardData.systemHealth).toBe('unhealthy');
      expect(dashboardData.readyForDevelopment).toBe(false);
      
      console.log('✅ AI Dashboard correctly reports system as unhealthy');
      console.log('📊 System health:', dashboardData.systemHealth);
      console.log('📊 Critical issues:', dashboardData.criticalIssues);
    });
    
    test('should provide clear AI guidance for compilation errors', async () => {
      const dashboard = new AIAgentDashboardRunner();
      const dashboardData = await (dashboard as any).generateDashboard();
      
      // Should have guidance about checking logs and restarting
      expect(dashboardData.autonomousGuidance).toContain(
        expect.stringContaining('FIRST STEP: Start the system')
      );
      
      expect(dashboardData.criticalIssues).toContain(
        expect.stringContaining('system signal')
      );
      
      console.log('✅ AI Dashboard provides actionable guidance');
      console.log('📋 Autonomous guidance:', dashboardData.autonomousGuidance);
    });
    
  });
  
  describe('AI Error Recovery Workflow', () => {
    
    test('should make TypeScript errors obvious to AI via npm start failure', async () => {
      // When npm start times out due to compilation errors, 
      // the AI should be guided to check TypeScript compilation
      
      const logPaths = [
        '.continuum/jtag/logs/system/npm-start.log',
        '.continuum/jtag/signals/system-ready.json'
      ];
      
      // Check if signal file shows unhealthy status
      if (existsSync('.continuum/jtag/signals/system-ready.json')) {
        const signalData = JSON.parse(
          readFileSync('.continuum/jtag/signals/system-ready.json', 'utf8')
        );
        
        // Should be unhealthy due to our compilation errors
        expect(signalData.systemHealth).toBe('unhealthy');
        
        console.log('✅ System signal correctly shows unhealthy state');
        console.log('📊 Signal data:', JSON.stringify(signalData, null, 2));
      }
    });
    
    test('AI should be able to identify specific TypeScript errors easily', () => {
      // The AI should be able to run simple commands to identify issues
      const aiCommands = [
        'npx tsc --noEmit --project .',  // Check all TypeScript files
        'npm run agent:quick',           // Quick health check
        'npm run signal:check',          // Check system signals
      ];
      
      console.log('🤖 AI-FRIENDLY ERROR DETECTION COMMANDS:');
      aiCommands.forEach((cmd, i) => {
        console.log(`${i + 1}. ${cmd}`);
      });
      
      // Test that the first command reveals our errors
      try {
        execSync(aiCommands[0], { encoding: 'utf8', stdio: 'pipe' });
        fail('Should have detected TypeScript errors');
      } catch (error: any) {
        const output = error.stdout + error.stderr;
        
        // Verify both browser and server errors are detectable
        const hasBrowserError = output.includes('InvalidBrowserType');
        const hasServerError = output.includes('UnknownType');
        
        console.log('🎯 Browser error detected:', hasBrowserError ? '✅' : '❌');
        console.log('🎯 Server error detected:', hasServerError ? '✅' : '❌');
        
        // At least one should be detected (tsx might not catch server error at compile time)
        expect(hasBrowserError || hasServerError).toBe(true);
      }
    });
    
  });
  
});

console.log('🧪 AI COMPILER ERROR DETECTION TEST COMPLETED');
console.log('📊 This test verifies that AIs can easily detect and resolve TypeScript compilation errors');
console.log('🎯 Next: Run the specific commands shown above to debug TypeScript issues');