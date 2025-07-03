/**
 * Simple Tab Manager - Just use ps to manage tabs
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class SimpleTabManager {
  /**
   * Count localhost:9000 tabs using multiple methods
   */
  async countTabs(): Promise<number> {
    try {
      // Method 1: Look for browsers connected to port 9000 (not just ESTABLISHED)
      const { stdout: browserConnections } = await execAsync(
        `lsof -i :9000 | grep -v LISTEN | grep -v node | wc -l`
      );
      const browserCount = parseInt(browserConnections.trim()) || 0;
      
      // Method 2: Check netstat for all connections to 9000
      const { stdout: netstatConnections } = await execAsync(
        `netstat -an | grep "9000" | grep -v LISTEN | wc -l`
      );
      const netstatCount = parseInt(netstatConnections.trim()) || 0;
      
      // Method 3: Count established WebSocket connections
      const { stdout: wsConnections } = await execAsync(
        `lsof -i :9000 | grep ESTABLISHED | grep -v node | wc -l`
      );
      const wsCount = parseInt(wsConnections.trim()) || 0;
      
      // Take the maximum - any of these could indicate tabs
      const maxCount = Math.max(browserCount, netstatCount, wsCount);
      
      if (maxCount > 0) {
        console.log(`🔍 Tab detection: browser=${browserCount}, netstat=${netstatCount}, websocket=${wsCount}`);
      }
      
      return maxCount;
    } catch {
      return 0;
    }
  }

  /**
   * Ensure exactly one tab
   */
  async ensureOneTab(log: (msg: string) => void): Promise<void> {
    const count = await this.countTabs();
    
    if (count === 0) {
      log('🌐 No tabs found - opening one');
      await execAsync('open http://localhost:9000');
    } else if (count === 1) {
      log('✅ Exactly 1 tab - perfect');
    } else {
      log(`⚠️ ${count} tabs found - please close ${count - 1} manually`);
      // Can't reliably close specific tabs cross-platform
    }
  }

  /**
   * Check tabs without taking action
   */
  async checkTabs(): Promise<{ count: number; action: string }> {
    const count = await this.countTabs();
    
    if (count === 0) {
      return { count, action: 'would_open' };
    } else if (count === 1) {
      return { count, action: 'perfect' };
    } else {
      return { count, action: 'too_many' };
    }
  }
}