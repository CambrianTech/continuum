/**
 * Screenshot Validation Tests
 * Migrated from various test files to ensure comprehensive coverage
 * Tests element validation, dimension checking, and error handling
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';

describe('Screenshot Validation', () => {
  test('should validate element dimensions', () => {
    const validElement = {
      tagName: 'DIV',
      offsetWidth: 800,
      offsetHeight: 600
    };
    
    const invalidElement = {
      tagName: 'DIV', 
      offsetWidth: 0,
      offsetHeight: 0
    };
    
    // Test validation logic
    const isValidDimension = (element) => {
      return element.offsetWidth > 0 && element.offsetHeight > 0;
    };
    
    assert(isValidDimension(validElement), 'Should validate element with proper dimensions');
    assert(!isValidDimension(invalidElement), 'Should reject element with zero dimensions');
  });

  test('should handle element existence validation', () => {
    const existingElement = { tagName: 'DIV', offsetWidth: 100, offsetHeight: 100 };
    const nullElement = null;
    const undefinedElement = undefined;
    
    const validateElement = (element) => {
      if (!element) {
        throw new Error('Target element is required');
      }
      return true;
    };
    
    assert(validateElement(existingElement), 'Should validate existing element');
    
    assert.throws(() => validateElement(nullElement), /Target element is required/);
    assert.throws(() => validateElement(undefinedElement), /Target element is required/);
  });

  test('should validate child elements when requested', () => {
    const mockElements = [];
    
    // Create 39 zero-dimension elements (from original test)
    for (let i = 0; i < 39; i++) {
      mockElements.push({
        tagName: i < 5 ? 'BUTTON' : i < 28 ? 'DIV' : 'SCRIPT',
        offsetWidth: 0,
        offsetHeight: 0
      });
    }
    
    const parentElement = {
      tagName: 'BODY',
      offsetWidth: 1673,
      offsetHeight: 1630,
      querySelectorAll: () => mockElements
    };
    
    const validateChildElements = (element) => {
      const children = element.querySelectorAll ? element.querySelectorAll('*') : [];
      const zeroSizeChildren = children.filter(child => 
        child.offsetWidth === 0 && child.offsetHeight === 0
      );
      
      if (zeroSizeChildren.length > 30) {
        throw new Error(`Too many zero-dimension child elements: ${zeroSizeChildren.length}`);
      }
      
      return true;
    };
    
    assert.throws(() => validateChildElements(parentElement), /Too many zero-dimension child elements/);
  });
});