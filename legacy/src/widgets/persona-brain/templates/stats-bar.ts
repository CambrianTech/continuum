/**
 * Stats Bar Template
 */

export interface StatsBarData {
  ltmSize: string;
  toolsAvailable: number;
  connections: number;
  issueCount: number;
}

export function renderStatsBar(data: StatsBarData): string {
  let html = '<div class="stats-bar">' +
    '<div class="stat-item clickable" data-action="open-log" data-log="hippocampus" title="View memory log">' +
      '<span class="stat-icon">💾</span>' +
      '<span class="stat-text">' + data.ltmSize + '</span>' +
    '</div>' +
    '<div class="stat-item clickable" data-action="open-log" data-log="tools" title="View tools log">' +
      '<span class="stat-icon">⚡</span>' +
      '<span class="stat-text">' + data.toolsAvailable + ' tools</span>' +
    '</div>' +
    '<div class="stat-item clickable" data-action="open-log" data-log="cns" title="View CNS log">' +
      '<span class="stat-icon">🔗</span>' +
      '<span class="stat-text">' + data.connections + ' conn</span>' +
    '</div>';

  if (data.issueCount > 0) {
    html += '<div class="stat-item issue-indicator" data-action="show-issues">' +
      '<span class="stat-icon">!</span>' +
      '<span class="stat-text issue-count">' + data.issueCount + ' issues</span>' +
    '</div>';
  }

  html += '</div>';
  return html;
}
