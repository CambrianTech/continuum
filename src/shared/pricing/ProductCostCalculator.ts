export type VolumeTier = 'standard' | 'bulk' | 'wholesale' | 'enterprise';

export interface ProductCostParams {
  quantity: number;
  productType: string;
  location: string;
}

export interface ProductCostResult {
  unitPrice: number;
  subtotal: number;
  discount: number;
  tax: number;
  total: number;
  tier: VolumeTier;
}

const BASE_PRICES: Readonly<Record<string, number>> = {
  basic: 10,
  pro: 25,
  enterprise: 50,
};

const DEFAULT_PRICE = 15;

const TAX_RATES: Readonly<Record<string, number>> = {
  CA: 0.095,
  NY: 0.08875,
  TX: 0.0825,
  WA: 0.1025,
};

const DEFAULT_TAX_RATE = 0.07;

const TIER_DISCOUNTS: Readonly<Record<VolumeTier, number>> = {
  standard: 0,
  bulk: 0.05,
  wholesale: 0.15,
  enterprise: 0.25,
};

function resolveTier(quantity: number): VolumeTier {
  if (quantity >= 200) return 'enterprise';
  if (quantity >= 50) return 'wholesale';
  if (quantity >= 10) return 'bulk';
  return 'standard';
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

export function calculateProductCost(params: ProductCostParams): ProductCostResult {
  const { quantity, productType, location } = params;

  const unitPrice = BASE_PRICES[productType] ?? DEFAULT_PRICE;
  const tier = resolveTier(quantity);
  const discountRate = TIER_DISCOUNTS[tier];
  const taxRate = TAX_RATES[location] ?? DEFAULT_TAX_RATE;

  const subtotal = round2(unitPrice * quantity);
  const discount = round2(subtotal * discountRate);
  const discountedSubtotal = subtotal - discount;
  const tax = round2(discountedSubtotal * taxRate);
  const total = round2(discountedSubtotal + tax);

  return { unitPrice, subtotal, discount, tax, total, tier };
}
