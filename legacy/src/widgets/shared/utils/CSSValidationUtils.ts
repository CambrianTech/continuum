/**
 * CSS Custom Property Validation Utilities
 * 
 * Provides validation for CSS custom properties (CSS variables) to ensure
 * themes properly define all properties used by widgets.
 */

export interface CSSValidationResult {
  usedProperties: string[];
  definedProperties: string[];
  missingProperties: string[];
  validationPassed: boolean;
  summary: {
    totalUsed: number;
    totalDefined: number;
    totalMissing: number;
  };
}

export interface CSSValidationOptions {
  enableWarnings?: boolean;
  enableDebugging?: boolean;
  widgetName?: string;
  themeName?: string;
}

export class CSSCustomPropertyValidator {
  
  /**
   * Validate CSS custom properties between widget CSS and theme CSS
   */
  static async validateProperties(
    widgetCSS: string, 
    themeCSS: string, 
    options: CSSValidationOptions = {}
  ): Promise<CSSValidationResult> {
    const {
      enableWarnings = true,
      enableDebugging = false,
      widgetName = 'Unknown Widget',
      themeName = 'Unknown Theme'
    } = options;

    try {
      if (enableDebugging) {
        console.log(`🔍 ${widgetName}: Starting CSS custom property validation...`);
      }

      // Extract properties from both CSS sources
      const usedProperties = this.extractCustomProperties(widgetCSS);
      const definedProperties = this.extractCustomProperties(themeCSS);
      
      // Find missing properties
      const missingProperties = usedProperties.filter(prop => !definedProperties.includes(prop));
      
      const result: CSSValidationResult = {
        usedProperties,
        definedProperties,
        missingProperties,
        validationPassed: missingProperties.length === 0,
        summary: {
          totalUsed: usedProperties.length,
          totalDefined: definedProperties.length,
          totalMissing: missingProperties.length
        }
      };

      // Log warnings if enabled
      if (enableWarnings && missingProperties.length > 0) {
        this.logMissingPropertiesWarning(widgetName, themeName, result);
      }

      // Log success if enabled
      if (enableDebugging && missingProperties.length === 0) {
        console.log(`✅ ${widgetName}: All CSS custom properties are properly defined (${usedProperties.length} properties)`);
      }

      return result;

    } catch (error) {
      console.error(`❌ ${widgetName}: CSS validation failed:`, error);
      
      // Return error result
      return {
        usedProperties: [],
        definedProperties: [],
        missingProperties: [],
        validationPassed: false,
        summary: { totalUsed: 0, totalDefined: 0, totalMissing: 0 }
      };
    }
  }

  /**
   * Extract CSS custom property names from CSS content
   */
  static extractCustomProperties(cssContent: string): string[] {
    const properties = new Set<string>();
    
    // Regex patterns for different CSS custom property usages
    const patterns = [
      // var() usages: var(--property-name)
      /var\(\s*(--[\w-]+)/g,
      // Direct definitions: --property-name:
      /(--[\w-]+)\s*:/g,
      // CSS property value assignments: property: var(--custom-prop)
      /:\s*var\(\s*(--[\w-]+)/g
    ];

    patterns.forEach(regex => {
      let match;
      while ((match = regex.exec(cssContent)) !== null) {
        const propName = match[1].trim();
        if (propName.startsWith('--')) {
          properties.add(propName);
        }
      }
    });
    
    return Array.from(properties).sort();
  }

  /**
   * Check if a specific CSS custom property is defined in the current document
   */
  static isPropertyDefined(propertyName: string): boolean {
    try {
      const computedStyle = getComputedStyle(document.documentElement);
      const value = computedStyle.getPropertyValue(propertyName);
      return value.trim() !== '';
    } catch {
      return false;
    }
  }

  /**
   * Get all currently defined CSS custom properties from the document
   */
  static getDefinedProperties(): string[] {
    try {
      const computedStyle = getComputedStyle(document.documentElement);
      const properties: string[] = [];
      
      // Iterate through all CSS properties
      for (let i = 0; i < computedStyle.length; i++) {
        const propName = computedStyle.item(i);
        if (propName.startsWith('--')) {
          properties.push(propName);
        }
      }
      
      return properties.sort();
    } catch {
      return [];
    }
  }

  /**
   * Log detailed warning about missing CSS custom properties
   */
  private static logMissingPropertiesWarning(
    widgetName: string, 
    themeName: string, 
    result: CSSValidationResult
  ): void {
    console.warn(`⚠️ ${widgetName}: Missing CSS custom properties in ${themeName} theme:`, {
      missing: result.missingProperties,
      widget: widgetName,
      theme: themeName,
      totalUsed: result.summary.totalUsed,
      totalDefined: result.summary.totalDefined
    });

    // Show detailed warning for each missing property
    result.missingProperties.forEach(prop => {
      console.warn(`   🚨 Missing: ${prop} (used by widget but not defined in theme)`);
    });

    // Provide helpful debugging advice
    console.warn(`   💡 Debugging tips:`);
    console.warn(`   - Check DevTools → Elements → :root → Computed to see CSS custom properties`);
    console.warn(`   - Look for properties showing as 'undefined' or empty values`);
    console.warn(`   - Add missing properties to themes/${themeName}.css`);
    
    // Show properties that ARE defined for reference
    if (result.definedProperties.length > 0) {
      console.log(`   ✅ Properties correctly defined: ${result.definedProperties.slice(0, 5).join(', ')}${result.definedProperties.length > 5 ? ` (+${result.definedProperties.length - 5} more)` : ''}`);
    }
  }
}