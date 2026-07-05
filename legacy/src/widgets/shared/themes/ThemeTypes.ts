/**
 * Theme System Types - Dynamic theme discovery and loading
 */

export interface ThemeManifest {
  name: string;
  displayName: string;
  description: string;
  category: string;
  author: string;
  version: string;
  files: string[];
  tags: string[];
  preview: {
    primaryColor: string;
    backgroundColor: string;
    textColor: string;
  };
}

export interface ThemeDiscoveryResult {
  themes: ThemeManifest[];
  categories: string[];
  tags: string[];
}

export interface ThemeLoadResult {
  success: boolean;
  themeName: string;
  cssContent: string;
  error?: string;
}

export class ThemeRegistry {
  private static themes: Map<string, ThemeManifest> = new Map();
  private static categories: Set<string> = new Set();
  private static tags: Set<string> = new Set();

  static registerTheme(manifest: ThemeManifest): void {
    this.themes.set(manifest.name, manifest);
    this.categories.add(manifest.category);
    manifest.tags.forEach(tag => this.tags.add(tag));
  }

  static getAllThemes(): ThemeManifest[] {
    return Array.from(this.themes.values()).sort((a, b) => {
      // Sort base theme first, then alphabetically
      if (a.name === 'base') return -1;
      if (b.name === 'base') return 1;
      return a.displayName.localeCompare(b.displayName);
    });
  }

  static getTheme(name: string): ThemeManifest | undefined {
    return this.themes.get(name);
  }

  static getThemesByCategory(category: string): ThemeManifest[] {
    return Array.from(this.themes.values()).filter(theme => theme.category === category);
  }

  static getThemesByTag(tag: string): ThemeManifest[] {
    return Array.from(this.themes.values()).filter(theme => theme.tags.includes(tag));
  }

  static getCategories(): string[] {
    return Array.from(this.categories).sort();
  }

  static getTags(): string[] {
    return Array.from(this.tags).sort();
  }

  static clear(): void {
    this.themes.clear();
    this.categories.clear();
    this.tags.clear();
  }
}