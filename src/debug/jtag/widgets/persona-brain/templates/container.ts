/**
 * Brain Container Template
 */

export interface ContainerData {
  loggingEnabled: boolean;
  personaStatus: string;
  brainSvg: string;
  activityFeed: string;
  issuesPanel: string;
  moduleDetails: string;
  statsBar: string;
}

export function renderContainer(data: ContainerData): string {
  const logIcon = data.loggingEnabled ? 'L' : '-';
  const logTitle = data.loggingEnabled ? 'Logging ON - Click to disable' : 'Logging OFF - Click to enable';
  const logClass = data.loggingEnabled ? 'log-toggle enabled' : 'log-toggle';

  return '<div class="brain-container">' +
    '<div class="brain-header">' +
      '<button class="' + logClass + '" data-action="toggle-logging" title="' + logTitle + '">' +
        logIcon +
      '</button>' +
      '<div class="persona-status status-' + data.personaStatus + '">' + data.personaStatus + '</div>' +
    '</div>' +
    '<div class="brain-main">' +
      '<div class="brain-visualization">' +
        data.brainSvg +
      '</div>' +
      '<div class="brain-sidebar">' +
        data.activityFeed +
        data.issuesPanel +
      '</div>' +
    '</div>' +
    data.moduleDetails +
    '<div class="brain-stats">' +
      data.statsBar +
    '</div>' +
  '</div>';
}
