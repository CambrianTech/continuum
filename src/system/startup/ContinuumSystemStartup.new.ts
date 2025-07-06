#!/usr/bin/env tsx
/**
 * Continuum System - Daemon auto-discovery architecture
 * No manual imports, no hardcoded daemon lists
 */

import { EventEmitter } from 'events';
import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonDiscovery } from '../daemon-discovery/DaemonDiscovery';
import { DaemonInfo } from '../../types/DaemonTypes';

export class ContinuumSystem extends EventEmitter {
  private daemons = new Map<string, BaseDaemon>();
  private readyDaemons = new Set<string>();
  private daemonDiscovery: DaemonDiscovery;
  
  constructor() {
    super();
    this.daemonDiscovery = new DaemonDiscovery();
  }
  
  async start(): Promise<void> {
    const startTime = new Date().toISOString();
    const pkg = this.getPackageInfo();
    
    console.log('╔══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗');
    console.log('║ 🌟 CONTINUUM SYSTEM STARTUP (Auto-Discovery)                                                                    ║');
    console.log(`║ Version: ${pkg.version.padEnd(20)} Start Time: ${startTime}       Process: ${process.pid.toString().padEnd(10)} ║`);
    console.log('╠══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣');
    
    // STEP 1: Discover all daemons
    console.log('║ 🔍 Discovering daemons...                                                                                        ║');
    const discoveredDaemons = await this.daemonDiscovery.discoverDaemons();
    console.log(`║ ✅ Found ${discoveredDaemons.length} daemons                                                                    ║`);
    
    // STEP 2: Sort by priority
    const sortedDaemons = discoveredDaemons.sort((a, b) => {
      const aPriority = a.packageJson.continuum?.priority || 50;
      const bPriority = b.packageJson.continuum?.priority || 50;
      return aPriority - bPriority;
    });
    
    // Display launch sequence
    const sequence = sortedDaemons.map((d, i) => `${i+1}. ${d.name}`).join(' → ');
    console.log(`║ 📋 Launch Sequence: ${sequence.padEnd(85)} ║`);
    console.log('╚══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝');
    
    // STEP 3: Load and start daemons
    console.log('🚀 Starting daemons...');
    
    for (let i = 0; i < sortedDaemons.length; i++) {
      const daemonInfo = sortedDaemons[i];
      const daemonStartTime = new Date().toISOString();
      
      console.log(`[${daemonStartTime}] 🚀 Starting ${daemonInfo.name} daemon...`);
      
      try {
        // Load the daemon
        const daemon = await this.loadDaemon(daemonInfo);
        if (!daemon) {
          throw new Error(`Failed to load daemon class from ${daemonInfo.path}`);
        }
        
        // Register daemon
        this.daemons.set(daemonInfo.name, daemon);
        
        // Setup event listeners
        this.setupDaemonListeners(daemonInfo.name, daemon);
        
        // Start the daemon
        await daemon.start();
        
        const daemonReadyTime = new Date().toISOString();
        console.log(`[${daemonReadyTime}] ✅ DAEMON READY: ${daemonInfo.name} (${i+1}/${sortedDaemons.length})`);
        console.log(`[${daemonReadyTime}] ✅ ${daemonInfo.name} daemon ready`);
        
      } catch (error) {
        const errorTime = new Date().toISOString();
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`[${errorTime}] 💥 ${daemonInfo.name} daemon FAILED:`, errorMessage);
        throw error;
      }
    }
    
    // STEP 4: Setup inter-daemon communication
    const waitTime = new Date().toISOString();
    console.log(`[${waitTime}] ⏳ Waiting for all daemons to be ready...`);
    await this.waitForSystemReady();
    
    await this.setupInterDaemonCommunication();
    
    // System is ready
    console.log('');
    console.log('✅ System ready and operational');
    console.log('🌐 Browser interface: http://localhost:9000');
    console.log('🔌 WebSocket API: ws://localhost:9000');
    console.log('');
  }
  
  private async loadDaemon(daemonInfo: DaemonInfo): Promise<BaseDaemon | null> {
    try {
      // Determine the main file
      const mainFile = daemonInfo.packageJson.main || `${daemonInfo.className}.ts`;
      const modulePath = `${daemonInfo.path}/${mainFile.replace('.js', '.ts')}`;
      
      // Dynamic import with file:// URL for absolute paths
      const fileUrl = `file://${modulePath}`;
      const module = await import(fileUrl);
      
      // Find the daemon class
      const DaemonClass = module.default || module[daemonInfo.className];
      
      if (!DaemonClass) {
        console.error(`No daemon class found in ${modulePath}`);
        return null;
      }
      
      // Instantiate
      return new DaemonClass();
      
    } catch (error) {
      console.error(`Error loading daemon ${daemonInfo.name}:`, error);
      return null;
    }
  }
  
  private setupDaemonListeners(name: string, daemon: BaseDaemon): void {
    daemon.on('stopped', () => this.handleDaemonStopped(name));
    daemon.on('error', (error) => this.handleDaemonError(name, error));
    daemon.on('ready', () => {
      this.readyDaemons.add(name);
      if (this.readyDaemons.size === this.daemons.size) {
        console.log('🎉 ALL DAEMONS READY - System operational!');
      }
    });
  }
  
  private async setupInterDaemonCommunication(): Promise<void> {
    console.log('🔌 Setting up inter-daemon communication...');
    
    // Get WebSocket daemon for routing
    const webSocketDaemon = this.daemons.get('websocket');
    if (!webSocketDaemon) {
      throw new Error('WebSocket daemon not found - required for inter-daemon communication');
    }
    
    // Register all daemons with WebSocket daemon
    for (const [name, daemon] of this.daemons) {
      if (name !== 'websocket') {
        (webSocketDaemon as any).registerDaemon(daemon);
      }
    }
    
    // Special registrations for daemons that need it
    const staticFileDaemon = this.daemons.get('static-file');
    if (staticFileDaemon && typeof (staticFileDaemon as any).registerWithWebSocketDaemon === 'function') {
      (staticFileDaemon as any).registerWithWebSocketDaemon(webSocketDaemon);
    }
    
    // Register route handlers
    (webSocketDaemon as any).registerRouteHandler('/api/commands/*', 'command-processor', 'handle_api');
    (webSocketDaemon as any).registerRouteHandler('*', 'renderer', 'http_request');
    
    console.log('✅ Inter-daemon communication established');
  }
  
  private async waitForSystemReady(): Promise<void> {
    // Simple ready check - all daemons started
    return Promise.resolve();
  }
  
  private handleDaemonStopped(name: string): void {
    const stopTime = new Date().toISOString();
    console.log(`[${stopTime}] 🛑 DAEMON STOPPED: ${name}`);
    this.readyDaemons.delete(name);
  }
  
  private handleDaemonError(name: string, error: any): void {
    const errorTime = new Date().toISOString();
    console.error(`[${errorTime}] 🚨 DAEMON ERROR: ${name} - ${error.message}`);
    console.error(`[${errorTime}] 📋 Stack trace:`, error.stack);
  }
  
  private getPackageInfo(): { version: string; name: string } {
    try {
      const fs = require('fs');
      const pkg = JSON.parse(fs.readFileSync('package.json', 'utf-8'));
      return { version: pkg.version, name: pkg.name };
    } catch (error) {
      return { version: 'unknown', name: 'continuum' };
    }
  }
  
  async stop(): Promise<void> {
    console.log('🛑 Shutting down Continuum system...');
    
    // Stop daemons in reverse order
    const daemonArray = Array.from(this.daemons.entries()).reverse();
    
    for (const [name, daemon] of daemonArray) {
      try {
        console.log(`Stopping ${name}...`);
        await daemon.stop();
      } catch (error) {
        console.error(`Error stopping ${name}:`, error);
      }
    }
    
    console.log('✅ Continuum system shutdown complete');
  }
  
  async getCurrentSessionInfo(): Promise<any> {
    // Optional: implement session info retrieval
    return null;
  }
}