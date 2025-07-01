/**
 * DataDisplayWidget - Base class for widgets that display lists of data with selection
 * 
 * Handles the common pattern of:
 * 1. Load data from API/source
 * 2. Display as a list with search/filter
 * 3. Handle item selection
 * 4. Provide item actions (edit, delete, etc.)
 */

import { BaseWidget } from './BaseWidget.js';

export interface DataItem {
  id: string;
  [key: string]: any;
}

export interface DataDisplayConfig<T extends DataItem> {
  apiEndpoint?: string;
  searchFields: (keyof T)[];
  sortField?: keyof T;
  sortDirection?: 'asc' | 'desc';
  itemsPerPage?: number;
}

export interface ItemActionHandler<T extends DataItem> {
  (item: T, action: string): Promise<void>;
}

export abstract class DataDisplayWidget<T extends DataItem> extends BaseWidget {
  protected items: T[] = [];
  protected filteredItems: T[] = [];
  protected selectedItem: T | null = null;
  protected searchQuery: string = '';
  protected isLoading: boolean = false;
  protected config: DataDisplayConfig<T>;

  constructor(config: DataDisplayConfig<T>) {
    super();
    this.config = config;
  }

  // Abstract methods that subclasses must implement
  protected abstract loadData(): Promise<T[]>;
  protected abstract renderItem(item: T): string;
  protected abstract getItemActions(item: T): { label: string; action: string }[];
  protected abstract handleItemAction(item: T, action: string): Promise<void>;

  // Optional methods that subclasses can override
  protected getSearchPlaceholder(): string {
    return `Search ${this.widgetTitle.toLowerCase()}...`;
  }

  protected getEmptyStateMessage(): string {
    return `No ${this.widgetTitle.toLowerCase()} found`;
  }

  protected shouldShowItem(item: T, query: string): boolean {
    if (!query) return true;
    
    const searchLower = query.toLowerCase();
    return this.config.searchFields.some(field => {
      const value = item[field];
      return String(value).toLowerCase().includes(searchLower);
    });
  }

  // Common lifecycle
  protected async initializeWidget(): Promise<void> {
    await this.refreshData();
    this.setupSearchHandlers();
  }

  protected async refreshData(): Promise<void> {
    try {
      this.isLoading = true;
      this.render();
      
      this.items = await this.loadData();
      this.applyFilters();
      
    } catch (error) {
      console.error(`Error loading ${this.widgetName} data:`, error);
    } finally {
      this.isLoading = false;
      this.render();
    }
  }

  protected applyFilters(): void {
    this.filteredItems = this.items.filter(item => 
      this.shouldShowItem(item, this.searchQuery)
    );

    // Apply sorting if configured
    if (this.config.sortField) {
      this.filteredItems.sort((a, b) => {
        const aVal = a[this.config.sortField!];
        const bVal = b[this.config.sortField!];
        const direction = this.config.sortDirection === 'desc' ? -1 : 1;
        
        if (aVal < bVal) return -1 * direction;
        if (aVal > bVal) return 1 * direction;
        return 0;
      });
    }
  }

  protected setupSearchHandlers(): void {
    const searchInput = this.shadowRoot?.querySelector('.search-input') as HTMLInputElement;
    if (searchInput) {
      searchInput.addEventListener('input', (e) => {
        this.searchQuery = (e.target as HTMLInputElement).value;
        this.applyFilters();
        this.render();
      });
    }
  }

  setupEventListeners(): void {
    // Handle item selection
    this.shadowRoot?.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      
      // Item selection
      const itemElement = target.closest('.data-item');
      if (itemElement) {
        const itemId = itemElement.getAttribute('data-id');
        const item = this.items.find(i => i.id === itemId);
        if (item) {
          this.selectItem(item);
        }
      }

      // Item actions
      const actionElement = target.closest('.item-action');
      if (actionElement) {
        e.stopPropagation();
        const itemId = actionElement.getAttribute('data-item-id');
        const action = actionElement.getAttribute('data-action');
        const item = this.items.find(i => i.id === itemId);
        
        if (item && action) {
          this.handleItemAction(item, action);
        }
      }
    });
  }

  protected selectItem(item: T): void {
    this.selectedItem = item;
    this.render();
    
    // Dispatch selection event
    this.dispatchEvent(new CustomEvent('item-selected', {
      detail: { item },
      bubbles: true
    }));
  }

  // Standard template structure
  renderContent(): string {
    return `
      <div class="data-display-container">
        <div class="header">
          <div class="search-container">
            <input 
              type="text" 
              class="search-input" 
              placeholder="${this.getSearchPlaceholder()}"
              value="${this.searchQuery}"
            />
          </div>
          <div class="actions">
            ${this.renderHeaderActions()}
          </div>
        </div>
        
        <div class="content">
          ${this.isLoading ? this.renderLoadingState() : this.renderItemList()}
        </div>
      </div>
    `;
  }

  protected renderHeaderActions(): string {
    return `
      <button class="refresh-btn" onclick="this.refreshData()">
        🔄 Refresh
      </button>
    `;
  }

  protected renderLoadingState(): string {
    return `<div class="loading">Loading ${this.widgetTitle.toLowerCase()}...</div>`;
  }

  protected renderItemList(): string {
    if (this.filteredItems.length === 0) {
      return `<div class="empty-state">${this.getEmptyStateMessage()}</div>`;
    }

    return `
      <div class="item-list">
        ${this.filteredItems.map(item => `
          <div class="data-item ${this.selectedItem?.id === item.id ? 'selected' : ''}" 
               data-id="${item.id}">
            ${this.renderItem(item)}
            <div class="item-actions">
              ${this.getItemActions(item).map(action => `
                <button class="item-action" 
                        data-item-id="${item.id}" 
                        data-action="${action.action}">
                  ${action.label}
                </button>
              `).join('')}
            </div>
          </div>
        `).join('')}
      </div>
    `;
  }
}