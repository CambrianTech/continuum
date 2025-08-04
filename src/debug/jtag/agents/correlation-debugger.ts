#!/usr/bin/env npx tsx
/**
 * Correlation Debugger - Diagnose WebSocket response correlation issues
 * 
 * This script analyzes the correlation flow between external clients and server
 * to identify why responses aren't reaching external clients.
 */

import fs from 'fs';
import path from 'path';

interface CorrelationEvent {
  timestamp: string;
  type: 'request' | 'response' | 'error';
  correlationId: string;
  details: string;
  lineNumber: number;
}

class CorrelationDebugger {
  private serverLogPath = path.join(
    'examples/test-bench/.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/server-node-output.log'
  );

  async analyzeCorrelationFlow(): Promise<void> {
    console.log('🔍 JTAG Correlation Flow Analysis');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');

    if (!fs.existsSync(this.serverLogPath)) {
      console.log('❌ Server log file not found:', this.serverLogPath);
      return;
    }

    const logContent = fs.readFileSync(this.serverLogPath, 'utf8');
    const lines = logContent.split('\n').filter(line => line.trim());
    
    // Extract correlation events
    const events = this.extractCorrelationEvents(lines);
    
    // Group by correlation ID
    const correlationGroups = this.groupByCorrelation(events);
    
    // Analyze each correlation
    console.log(`📊 Found ${Object.keys(correlationGroups).length} correlation flows to analyze\n`);
    
    for (const [correlationId, correlationEvents] of Object.entries(correlationGroups)) {
      this.analyzeCorrelation(correlationId, correlationEvents);
    }
    
    // Summary
    this.printSummary(correlationGroups);
  }

  private extractCorrelationEvents(lines: string[]): CorrelationEvent[] {
    const events: CorrelationEvent[] = [];
    
    lines.forEach((line, index) => {
      // Look for correlation IDs in the format client_timestamp_randomid
      const correlationMatch = line.match(/client_(\d+)_([a-z0-9]+)/);
      if (!correlationMatch) return;
      
      const correlationId = correlationMatch[0];
      const timestamp = new Date(parseInt(correlationMatch[1])).toISOString();
      
      let type: 'request' | 'response' | 'error' = 'request';
      let details = line;
      
      if (line.includes('No pending request for')) {
        type = 'error';
        details = 'ResponseCorrelator: No pending request found';
      } else if (line.includes('Sending response for') || line.includes('Created response message')) {
        type = 'response';
        details = 'Server creating/sending response';
      } else if (line.includes('Processing message req:')) {
        type = 'request';
        details = 'Server processing request';
      }
      
      events.push({
        timestamp,
        type,
        correlationId,
        details,
        lineNumber: index + 1
      });
    });
    
    return events;
  }

  private groupByCorrelation(events: CorrelationEvent[]): Record<string, CorrelationEvent[]> {
    const groups: Record<string, CorrelationEvent[]> = {};
    
    events.forEach(event => {
      if (!groups[event.correlationId]) {
        groups[event.correlationId] = [];
      }
      groups[event.correlationId].push(event);
    });
    
    // Sort each group by line number
    Object.values(groups).forEach(group => {
      group.sort((a, b) => a.lineNumber - b.lineNumber);
    });
    
    return groups;
  }

  private analyzeCorrelation(correlationId: string, events: CorrelationEvent[]): void {
    console.log(`🎯 Correlation: ${correlationId}`);
    console.log(`   Timeline: ${events.length} events`);
    
    const hasRequest = events.some(e => e.type === 'request');
    const hasResponse = events.some(e => e.type === 'response');
    const hasError = events.some(e => e.type === 'error');
    
    console.log(`   📥 Request:  ${hasRequest ? '✅' : '❌'}`);
    console.log(`   📤 Response: ${hasResponse ? '✅' : '❌'}`);
    console.log(`   ⚠️  Error:    ${hasError ? '❌' : '✅'}`);
    
    if (hasError) {
      const errorEvent = events.find(e => e.type === 'error');
      console.log(`   🔍 Issue: ${errorEvent?.details}`);
      console.log(`   🎯 Problem: External client correlation not tracked by server ResponseCorrelator`);
    }
    
    // Show event sequence
    console.log('   📋 Event Sequence:');
    events.forEach((event, index) => {
      const icon = event.type === 'request' ? '📥' : event.type === 'response' ? '📤' : '⚠️';
      console.log(`      ${index + 1}. ${icon} ${event.type}: ${event.details.substring(0, 80)}...`);
    });
    
    console.log('');
  }

  private printSummary(correlationGroups: Record<string, CorrelationEvent[]>): void {
    const total = Object.keys(correlationGroups).length;
    const successful = Object.values(correlationGroups).filter(events => 
      events.some(e => e.type === 'request') && 
      events.some(e => e.type === 'response') && 
      !events.some(e => e.type === 'error')
    ).length;
    const failed = total - successful;
    
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 CORRELATION ANALYSIS SUMMARY');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log(`   Total Correlations: ${total}`);
    console.log(`   ✅ Successful: ${successful}`);
    console.log(`   ❌ Failed: ${failed}`);
    console.log(`   📈 Success Rate: ${total > 0 ? Math.round((successful / total) * 100) : 0}%`);
    
    if (failed > 0) {
      console.log('');
      console.log('🎯 ROOT CAUSE IDENTIFIED:');
      console.log('   External client correlation IDs are not being tracked by server ResponseCorrelator');
      console.log('   Server processes requests and creates responses, but cannot correlate them back');
      console.log('   This suggests the correlation tracking happens only within the same process context');
      console.log('');
      console.log('💡 SOLUTION NEEDED:');
      console.log('   - WebSocket transport layer needs to register correlations with server ResponseCorrelator');
      console.log('   - Or external client responses need different routing mechanism');
      console.log('   - Check JTAGClient handleTransportMessage implementation');
    }
  }
}

// Run the analysis
const analyzer = new CorrelationDebugger();
analyzer.analyzeCorrelationFlow().catch(console.error);