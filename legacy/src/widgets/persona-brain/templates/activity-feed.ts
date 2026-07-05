/**
 * Activity Feed Template
 */

export interface ActivityEvent {
  type: string;
  timestamp: string;
  module: string;
  message: string;
  severity: 'info' | 'warn' | 'error';
  icon: string;
}

export interface ActivityFeedData {
  events: ActivityEvent[];
  totalCount: number;
}

export function renderActivityFeed(data: ActivityFeedData): string {
  if (data.events.length === 0) {
    return '<div class="activity-feed"><div class="activity-empty">No recent activity</div></div>';
  }

  const eventsHtml = data.events.map(e => 
    '<div class="activity-item severity-' + e.severity + '">' +
      '<span class="activity-icon">' + e.icon + '</span>' +
      '<span class="activity-time">' + e.timestamp + '</span>' +
      '<span class="activity-module">' + e.module + '</span>' +
      '<span class="activity-message">' + e.message + '</span>' +
    '</div>'
  ).join('');

  return '<div class="activity-feed">' +
    '<div class="activity-header">' +
      '<span class="activity-title">ACTIVITY</span>' +
      '<span class="activity-count">' + data.totalCount + ' events</span>' +
    '</div>' +
    '<div class="activity-list">' + eventsHtml + '</div>' +
  '</div>';
}
