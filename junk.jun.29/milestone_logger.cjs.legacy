/**
 * Centralized Milestone Logger for Continuum
 * 
 * This provides standardized progress milestones that can be:
 * - Displayed in UI widgets
 * - Tracked for performance monitoring  
 * - Used for progress indicators
 * - Emitted as real-time events
 */

class MilestoneLogger {
    constructor(context = 'SYSTEM') {
        this.context = context;
        this.milestones = [];
        this.startTime = Date.now();
    }

    /**
     * Log a major process milestone
     * @param {string} phase - The phase/category (BROWSER_LAUNCH, VERIFICATION, etc.)
     * @param {string} action - The specific action being performed
     * @param {string} details - Optional additional details
     * @param {string} level - Log level (INFO, WARN, ERROR, CRITICAL)
     */
    logMilestone(phase, action, details = '', level = 'INFO') {
        const timestamp = new Date().toLocaleTimeString();
        const elapsed = ((Date.now() - this.startTime) / 1000).toFixed(1);
        
        const milestone = {
            timestamp,
            elapsed: `${elapsed}s`,
            context: this.context,
            phase,
            action,
            details,
            level
        };
        
        this.milestones.push(milestone);
        
        // Always output critical browser/system events
        const icon = this.getIcon(phase, level);
        console.log(`${icon} MILESTONE [${timestamp}] ${this.context}/${phase}: ${action}`);
        if (details) {
            console.log(`   ℹ️  ${details}`);
        }
        
        // Emit event for real-time UI updates (if in browser environment)
        if (typeof window !== 'undefined' && window.ContinuumEvents) {
            window.ContinuumEvents.emit('milestone', milestone);
        }
    }

    getIcon(phase, level) {
        const icons = {
            BROWSER_LAUNCH: '🌐',
            VERIFICATION: '🔍', 
            SCREENSHOT: '📸',
            GIT_STAGING: '📦',
            CLEANUP: '🧹',
            ERROR: '❌',
            SUCCESS: '✅'
        };
        
        if (level === 'CRITICAL' || level === 'ERROR') return '🚨';
        return icons[phase] || '🎯';
    }

    /**
     * Get all milestones for progress tracking
     */
    getMilestones() {
        return this.milestones;
    }

    /**
     * Get current progress summary
     */
    getProgressSummary() {
        const totalTime = ((Date.now() - this.startTime) / 1000).toFixed(1);
        const phases = [...new Set(this.milestones.map(m => m.phase))];
        
        return {
            totalTime: `${totalTime}s`,
            totalMilestones: this.milestones.length,
            phases,
            lastMilestone: this.milestones[this.milestones.length - 1]
        };
    }
}

// Export for both Node.js and browser environments
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { MilestoneLogger };
} else if (typeof window !== 'undefined') {
    window.MilestoneLogger = MilestoneLogger;
}