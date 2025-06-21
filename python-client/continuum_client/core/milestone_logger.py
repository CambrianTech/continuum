"""
Centralized Milestone Logger for Continuum Python Components

This provides standardized progress milestones that can be:
- Displayed in UI widgets
- Tracked for performance monitoring  
- Used for progress indicators
- Emitted as real-time events
"""

import time
import json
from datetime import datetime
from typing import List, Dict, Optional

class MilestoneLogger:
    def __init__(self, context: str = 'SYSTEM'):
        self.context = context
        self.milestones: List[Dict] = []
        self.start_time = time.time()

    def log_milestone(self, phase: str, action: str, details: str = '', level: str = 'INFO'):
        """
        Log a major process milestone
        
        Args:
            phase: The phase/category (BROWSER_LAUNCH, VERIFICATION, etc.)
            action: The specific action being performed  
            details: Optional additional details
            level: Log level (INFO, WARN, ERROR, CRITICAL)
        """
        timestamp = datetime.now().strftime("%H:%M:%S")
        elapsed = f"{(time.time() - self.start_time):.1f}s"
        
        milestone = {
            'timestamp': timestamp,
            'elapsed': elapsed,
            'context': self.context,
            'phase': phase,
            'action': action,
            'details': details,
            'level': level
        }
        
        self.milestones.append(milestone)
        
        # Always output critical browser/system events
        icon = self.get_icon(phase, level)
        print(f"{icon} MILESTONE [{timestamp}] {self.context}/{phase}: {action}")
        if details:
            print(f"   ℹ️  {details}")
        
        # Emit to Continuum core system for real-time UI updates
        self._emit_to_continuum(milestone)

    def get_icon(self, phase: str, level: str) -> str:
        """Get appropriate icon for milestone"""
        icons = {
            'BROWSER_LAUNCH': '🌐',
            'VERIFICATION': '🔍', 
            'SCREENSHOT': '📸',
            'GIT_STAGING': '📦',
            'CLEANUP': '🧹',
            'ERROR': '❌',
            'SUCCESS': '✅'
        }
        
        if level in ['CRITICAL', 'ERROR']:
            return '🚨'
        return icons.get(phase, '🎯')

    def get_milestones(self) -> List[Dict]:
        """Get all milestones for progress tracking"""
        return self.milestones

    def get_progress_summary(self) -> Dict:
        """Get current progress summary"""
        total_time = f"{(time.time() - self.start_time):.1f}s"
        phases = list(set(m['phase'] for m in self.milestones))
        
        return {
            'total_time': total_time,
            'total_milestones': len(self.milestones),
            'phases': phases,
            'last_milestone': self.milestones[-1] if self.milestones else None
        }

    def _emit_to_continuum(self, milestone: Dict):
        """Send milestone event to Continuum core system"""
        try:
            # Import here to avoid circular imports
            from .client import get_global_client
            
            client = get_global_client()
            if client and client.connected:
                # Send as milestone event to Continuum core
                import asyncio
                asyncio.create_task(client.send_milestone_event(milestone))
        except Exception as e:
            # Don't break logging if Continuum connection fails
            pass

    def export_for_ui(self) -> str:
        """Export milestones as JSON for UI consumption"""
        return json.dumps({
            'summary': self.get_progress_summary(),
            'milestones': self.milestones
        }, indent=2)

# Global milestone logger instances for different contexts
verification_logger = MilestoneLogger('VERIFICATION')
browser_logger = MilestoneLogger('BROWSER')
git_logger = MilestoneLogger('GIT')