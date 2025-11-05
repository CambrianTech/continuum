/**
 * Zombie Tab Killer - Detects and reports tabs that are connected to localhost:9000
 * but not actively communicating via WebSocket
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export interface TabInfo {
  pid: number;
  process: string;
  state: string;
}

export class ZombieTabKiller {
  /**
   * Find all processes that have localhost:9000 open
   */
  async findAllTabs(): Promise<TabInfo[]> {
    const tabs: TabInfo[] = [];
    
    try {
      // Get detailed info about connections to port 9000
      const { stdout } = await execAsync(
        `lsof -i :9000 | grep -v LISTEN | grep -v node`
      );
      
      const lines = stdout.trim().split('\n').filter(line => line.length > 0);
      
      for (const line of lines) {
        const parts = line.split(/\s+/);
        if (parts.length >= 9) {
          const process = parts[0];
          const pid = parseInt(parts[1]);
          const state = parts[7]; // TCP state
          
          if (!isNaN(pid)) {
            tabs.push({ pid, process, state });
          }
        }
      }
    } catch {
      // lsof might return error if no connections found
    }
    
    return tabs;
  }
  
  /**
   * Count zombie tabs (connected but not ESTABLISHED)
   */
  async countZombieTabs(): Promise<number> {
    const tabs = await this.findAllTabs();
    const zombies = tabs.filter(tab => 
      tab.state !== 'ESTABLISHED' && 
      tab.state !== '(ESTABLISHED)'
    );
    return zombies.length;
  }
  
  /**
   * Get detailed tab status
   */
  async getTabStatus(): Promise<{
    total: number;
    active: number;
    zombies: number;
    details: TabInfo[];
  }> {
    const tabs = await this.findAllTabs();
    const active = tabs.filter(tab => 
      tab.state === 'ESTABLISHED' || 
      tab.state === '(ESTABLISHED)'
    ).length;
    const zombies = tabs.length - active;
    
    return {
      total: tabs.length,
      active,
      zombies,
      details: tabs
    };
  }
  
  /**
   * Kill zombie tabs (non-established connections)
   */
  async killZombieTabs(log: (msg: string) => void): Promise<number> {
    const tabs = await this.findAllTabs();
    const zombies = tabs.filter(tab => 
      tab.state !== 'ESTABLISHED' && 
      tab.state !== '(ESTABLISHED)'
    );
    
    let killed = 0;
    for (const zombie of zombies) {
      try {
        log(`💀 Killing zombie tab: ${zombie.process} (PID: ${zombie.pid}, State: ${zombie.state})`);
        await execAsync(`kill -9 ${zombie.pid}`);
        killed++;
      } catch (error) {
        log(`⚠️ Failed to kill PID ${zombie.pid}: ${error}`);
      }
    }
    
    return killed;
  }
}