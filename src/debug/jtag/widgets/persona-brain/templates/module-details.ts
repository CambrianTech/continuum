/**
 * Module Details Template
 */

export interface ModuleDetailsData {
  module: string;
  status: string;
  logType: string;
  stats: { label: string; value: string; statusClass?: string }[];
}

export function renderModuleDetails(data: ModuleDetailsData | null): string {
  if (!data) return '';

  const statsHtml = data.stats.map(s =>
    '<div class="stat-row">' +
      '<span class="stat-label">' + s.label + '</span>' +
      '<span class="stat-value ' + (s.statusClass || '') + '">' + s.value + '</span>' +
    '</div>'
  ).join('');

  return '<div class="module-details">' +
    '<div class="module-detail-view">' +
      '<div class="detail-header">' +
        '<h3>' + data.module.toUpperCase() + '</h3>' +
        '<button class="btn btn-secondary btn-small" data-action="back">Back to Overview</button>' +
      '</div>' +
      '<div class="detail-content">' +
        '<div class="stat-row">' +
          '<span class="stat-label">Status</span>' +
          '<span class="stat-value status-' + data.status + '">' + data.status + '</span>' +
        '</div>' +
        statsHtml +
      '</div>' +
      '<div class="detail-actions">' +
        '<button class="btn btn-primary" data-action="view-log" data-log="' + data.logType + '">' +
          'View ' + data.logType + '.log' +
        '</button>' +
        '<button class="btn btn-secondary" data-action="inspect">' +
          'Inspect State' +
        '</button>' +
      '</div>' +
    '</div>' +
  '</div>';
}
