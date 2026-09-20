/**
 * DOM CSS Validation - Browser-Only Operations
 * 
 * Handles browser-specific CSS property validation using DOM APIs.
 * This module should NEVER be imported by shared/ or server/ code.
 */

export interface CSSPropertyValidationResult {
  isDefined: boolean;
  value: string;
  property: string;
}

export interface DocumentCSSPropertiesResult {
  properties: string[];
  totalCount: number;
}

export class DOMCSSValidator {
  
  /**
   * Check if a specific CSS custom property is defined in the current document
   */
  static validatePropertyInDocument(propertyName: string): CSSPropertyValidationResult {
    try {
      const computedStyle = getComputedStyle(document.documentElement);
      const value = computedStyle.getPropertyValue(propertyName);
      
      return {
        isDefined: value.trim() !== '',
        value: value.trim(),
        property: propertyName
      };
      
    } catch (error) {
      return {
        isDefined: false,
        value: '',
        property: propertyName
      };
    }
  }

  /**
   * Get all currently defined CSS custom properties from the document
   */
  static getAllDefinedProperties(): DocumentCSSPropertiesResult {
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
      
      return {
        properties: properties.sort(),
        totalCount: properties.length
      };
      
    } catch (error) {
      return {
        properties: [],
        totalCount: 0
      };
    }
  }

  /**
   * Validate multiple CSS properties at once
   */
  static validateMultipleProperties(propertyNames: string[]): CSSPropertyValidationResult[] {
    return propertyNames.map(propertyName => 
      this.validatePropertyInDocument(propertyName)
    );
  }

  /**
   * Get CSS property validation summary for debugging
   */
  static getValidationSummary(propertyNames: string[]): {
    totalChecked: number;
    defined: number;
    undefined: number;
    definedProperties: string[];
    undefinedProperties: string[];
  } {
    const results = this.validateMultipleProperties(propertyNames);
    const definedProperties = results.filter(r => r.isDefined).map(r => r.property);
    const undefinedProperties = results.filter(r => !r.isDefined).map(r => r.property);
    
    return {
      totalChecked: results.length,
      defined: definedProperties.length,
      undefined: undefinedProperties.length,
      definedProperties,
      undefinedProperties
    };
  }
}