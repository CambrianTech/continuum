import { describe, it, expect } from 'vitest';
import { calculateProductCost } from './ProductCostCalculator';

describe('calculateProductCost', () => {
  describe('volume tiers', () => {
    it('assigns standard tier for quantity 1', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('standard');
      expect(result.discount).toBe(0);
    });

    it('assigns standard tier for quantity 9', () => {
      const result = calculateProductCost({ quantity: 9, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('standard');
      expect(result.discount).toBe(0);
    });

    it('assigns bulk tier (5% off) for quantity 10', () => {
      const result = calculateProductCost({ quantity: 10, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('bulk');
      // subtotal = 10 * 10 = 100, discount = 5
      expect(result.subtotal).toBe(100);
      expect(result.discount).toBe(5);
    });

    it('assigns bulk tier for quantity 49', () => {
      const result = calculateProductCost({ quantity: 49, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('bulk');
    });

    it('assigns wholesale tier (15% off) for quantity 50', () => {
      const result = calculateProductCost({ quantity: 50, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('wholesale');
      // subtotal = 50 * 10 = 500, discount = 75
      expect(result.subtotal).toBe(500);
      expect(result.discount).toBe(75);
    });

    it('assigns wholesale tier for quantity 199', () => {
      const result = calculateProductCost({ quantity: 199, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('wholesale');
    });

    it('assigns enterprise tier (25% off) for quantity 200', () => {
      const result = calculateProductCost({ quantity: 200, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('enterprise');
      // subtotal = 200 * 10 = 2000, discount = 500
      expect(result.subtotal).toBe(2000);
      expect(result.discount).toBe(500);
    });

    it('assigns enterprise tier for quantity above 200', () => {
      const result = calculateProductCost({ quantity: 500, productType: 'basic', location: 'TX' });
      expect(result.tier).toBe('enterprise');
    });
  });

  describe('product types', () => {
    it('prices basic at $10/unit', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'TX' });
      expect(result.unitPrice).toBe(10);
    });

    it('prices pro at $25/unit', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'pro', location: 'TX' });
      expect(result.unitPrice).toBe(25);
    });

    it('prices enterprise at $50/unit', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'enterprise', location: 'TX' });
      expect(result.unitPrice).toBe(50);
    });

    it('uses $15 default for unknown product type', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'unknown-type', location: 'TX' });
      expect(result.unitPrice).toBe(15);
    });
  });

  describe('location tax rates', () => {
    it('applies CA tax at 9.5%', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'CA' });
      // unitPrice=10, no discount, tax = 10 * 0.095 = 0.95
      expect(result.tax).toBe(0.95);
      expect(result.total).toBe(10.95);
    });

    it('applies NY tax at 8.875%', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'NY' });
      expect(result.tax).toBe(0.89);
      expect(result.total).toBe(10.89);
    });

    it('applies TX tax at 8.25%', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'TX' });
      expect(result.tax).toBe(0.83);
      expect(result.total).toBe(10.83);
    });

    it('applies WA tax at 10.25%', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'WA' });
      expect(result.tax).toBe(1.02);
      expect(result.total).toBe(11.02);
    });

    it('applies default 7% tax for unknown location', () => {
      const result = calculateProductCost({ quantity: 1, productType: 'basic', location: 'ZZ' });
      expect(result.tax).toBe(0.70);
      expect(result.total).toBe(10.70);
    });
  });

  describe('monetary rounding', () => {
    it('rounds all values to 2 decimal places', () => {
      const result = calculateProductCost({ quantity: 3, productType: 'pro', location: 'NY' });
      // subtotal = 75, discount = 0, tax = 75 * 0.08875 = 6.65625 → 6.66
      expect(result.subtotal).toBe(75);
      expect(result.discount).toBe(0);
      expect(result.tax).toBe(6.66);
      expect(result.total).toBe(81.66);
      expect(Number.isInteger(result.tax * 100)).toBe(true);
      expect(Number.isInteger(result.total * 100)).toBe(true);
    });
  });

  describe('full calculation correctness', () => {
    it('computes a bulk-tier pro purchase in CA correctly', () => {
      // quantity=10, pro=$25, bulk=5% off, CA=9.5%
      // subtotal = 250, discount = 12.50, taxable = 237.50, tax = 22.56, total = 260.06
      const result = calculateProductCost({ quantity: 10, productType: 'pro', location: 'CA' });
      expect(result.unitPrice).toBe(25);
      expect(result.tier).toBe('bulk');
      expect(result.subtotal).toBe(250);
      expect(result.discount).toBe(12.50);
      expect(result.tax).toBe(22.56);
      expect(result.total).toBe(260.06);
    });

    it('computes an enterprise-tier enterprise purchase in WA correctly', () => {
      // quantity=200, enterprise=$50, enterprise=25% off, WA=10.25%
      // subtotal = 10000, discount = 2500, taxable = 7500, tax = 768.75, total = 8268.75
      const result = calculateProductCost({ quantity: 200, productType: 'enterprise', location: 'WA' });
      expect(result.unitPrice).toBe(50);
      expect(result.tier).toBe('enterprise');
      expect(result.subtotal).toBe(10000);
      expect(result.discount).toBe(2500);
      expect(result.tax).toBe(768.75);
      expect(result.total).toBe(8268.75);
    });
  });
});
