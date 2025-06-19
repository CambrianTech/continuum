/**
 * ContinuonStatus - Unified continuon (AI entity) status and emotion management
 * The continuon is the AI object/avatar, continuum is the app
 * Controls CLI title, favicon, and all UI status indicators
 */

class ContinuonStatus {
  constructor(continuum) {
    this.continuum = continuum;
    
    // Status priority: RED (critical) > YELLOW (warning) > GREEN (healthy) > EMOTION
    this.currentStatus = 'disconnected';
    this.currentEmotion = null;
    this.statusEmoji = '🔴';
    
    console.log('🎯 ContinuonStatus: Initialized unified status system');
  }

  /**
   * Update system status (highest priority)
   * @param {string} status - 'connected', 'connecting', 'disconnected', 'error'
   */
  updateStatus(status) {
    this.currentStatus = status;
    this.statusEmoji = this.getStatusEmoji(status);
    
    console.log(`🎯 ContinuonStatus: Status updated to ${status} (${this.statusEmoji})`);
    this.broadcastUpdate();
  }

  /**
   * Update AI emotion (lower priority - only shown when healthy)
   * @param {string} emotion - 'excited', 'thinking', 'happy', etc.
   * @param {string} emoji - Custom emoji override
   */
  updateEmotion(emotion, emoji = null) {
    this.currentEmotion = emotion;
    
    // Only show emotion if system is healthy
    if (this.currentStatus === 'connected') {
      this.statusEmoji = emoji || this.getEmotionEmoji(emotion);
      console.log(`🎯 ContinuonStatus: Emotion updated to ${emotion} (${this.statusEmoji})`);
      this.broadcastUpdate();
    } else {
      console.log(`🎯 ContinuonStatus: Emotion ${emotion} cached (status priority: ${this.currentStatus})`);
    }
  }

  /**
   * Get status emoji based on connection state
   */
  getStatusEmoji(status) {
    switch (status) {
      case 'connected': 
        return this.currentEmotion ? this.getEmotionEmoji(this.currentEmotion) : '🟢';
      case 'connecting': return '🟡';
      case 'disconnected': return '🔴';
      case 'error': return '🔴';
      default: return '🔴';
    }
  }

  /**
   * Get emotion emoji (when system is healthy)
   */
  getEmotionEmoji(emotion) {
    const emotionMap = {
      'excited': '🚀',
      'thinking': '🤔', 
      'happy': '😊',
      'working': '⚡',
      'processing': '🧠',
      'sleeping': '😴',
      'alert': '👀',
      'focused': '🎯'
    };
    return emotionMap[emotion] || '🟢';
  }

  /**
   * Broadcast status update to all connected clients and CLI
   */
  broadcastUpdate() {
    // Update CLI process title (for process managers)
    process.title = `${this.statusEmoji} continuum`;
    
    // TODO: Terminal tab title updates - disabled for now to prevent server exit
    // May interfere with server stdout, needs investigation
    
    // Broadcast to all WebSocket clients
    if (this.continuum.webSocketServer) {
      this.continuum.webSocketServer.broadcast({
        type: 'continuon_status_update',
        emoji: this.statusEmoji,
        status: this.currentStatus,
        emotion: this.currentEmotion
      });
    }
    
    console.log(`🎯 ContinuonStatus: Broadcasted ${this.statusEmoji} to CLI and ${this.getClientCount()} clients`);
  }

  /**
   * Get current display emoji
   */
  getCurrentEmoji() {
    return this.statusEmoji;
  }

  /**
   * Get current status info
   */
  getStatus() {
    return {
      status: this.currentStatus,
      emotion: this.currentEmotion,
      emoji: this.statusEmoji
    };
  }

  /**
   * Get connected client count
   */
  getClientCount() {
    return this.continuum.webSocketServer?.wss?.clients?.size || 0;
  }
}

module.exports = ContinuonStatus;