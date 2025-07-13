/**
 * Data Source Manager - Single Responsibility: Data Source Operations
 * =================================================================
 * Handles data source selection, fetching, and processing
 */

import { DataSourceType } from '../../../types/shared/WidgetServerTypes';

export interface DataSourceConfig {
  readonly type: DataSourceType;
  readonly displayName: string;
  readonly description: string;
  readonly icon: string;
  readonly refreshInterval: number;
}

export class DataSourceManager {
  private selectedDataSource: DataSourceType = 'health';
  private readonly dataSources: DataSourceConfig[] = [];

  constructor(dataSources: DataSourceConfig[]) {
    this.dataSources = [...dataSources];
  }

  getSelectedDataSource(): DataSourceType {
    return this.selectedDataSource;
  }

  setSelectedDataSource(dataSource: DataSourceType): boolean {
    if (this.isValidDataSource(dataSource)) {
      this.selectedDataSource = dataSource;
      return true;
    }
    return false;
  }

  getDataSources(): readonly DataSourceConfig[] {
    return this.dataSources;
  }

  getDataSourceConfig(type: DataSourceType): DataSourceConfig | undefined {
    return this.dataSources.find(ds => ds.type === type);
  }

  private isValidDataSource(dataSource: DataSourceType): boolean {
    return this.dataSources.some(ds => ds.type === dataSource);
  }

  getDefaultRefreshInterval(): number {
    const config = this.getDataSourceConfig(this.selectedDataSource);
    return config?.refreshInterval ?? 30000;
  }
}