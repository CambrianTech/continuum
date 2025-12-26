/**
 * Issues Panel Template
 */

export interface Issue {
  id: string;
  module: string;
  type: 'error' | 'warning';
  message: string;
  timestamp: string;
}

export function renderIssuesPanel(issues: Issue[]): string {
  if (issues.length === 0) {
    return '';
  }

  const issuesHtml = issues.map(i =>
    '<div class="issue-item issue-' + i.type + '" data-issue-id="' + i.id + '" data-module="' + i.module + '">' +
      '<span class="issue-icon">' + (i.type === 'error' ? '!' : '?') + '</span>' +
      '<span class="issue-module">' + i.module + '</span>' +
      '<span class="issue-message">' + i.message + '</span>' +
      '<span class="issue-time">' + i.timestamp + '</span>' +
    '</div>'
  ).join('');

  return '<div class="issues-panel">' +
    '<div class="issues-header">' +
      '<span class="issues-title">ISSUES</span>' +
      '<span class="issues-count">' + issues.length + '</span>' +
    '</div>' +
    '<div class="issues-list">' + issuesHtml + '</div>' +
  '</div>';
}
