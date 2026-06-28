type Rate = {
  pricingModel: string;
  estimated: boolean;
  input: number;
  cachedInput: number;
  output: number;
};

type CostRow = {
  model?: string | null;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  totalTokens: number;
};

const rates: Record<string, Rate> = {
  'gpt-5.5': { pricingModel: 'gpt-5.5', estimated: false, input: 125, cachedInput: 12.5, output: 750 },
  'gpt-5.4': { pricingModel: 'gpt-5.4', estimated: false, input: 62.5, cachedInput: 6.25, output: 375 },
  'gpt-5.4-mini': { pricingModel: 'gpt-5.4-mini', estimated: false, input: 18.75, cachedInput: 1.875, output: 113 },
  'gpt-5.3-codex': { pricingModel: 'gpt-5.3-codex', estimated: false, input: 43.75, cachedInput: 4.375, output: 350 },
  'gpt-5.2': { pricingModel: 'gpt-5.2', estimated: false, input: 43.75, cachedInput: 4.375, output: 350 },
  'gpt-5': { pricingModel: 'gpt-5', estimated: false, input: 43.75, cachedInput: 4.375, output: 350 },
};

function rateForModel(model?: string | null): Rate | null {
  const normalized = String(model || '').toLowerCase();
  if (normalized === 'codex-auto-review') {
    return { ...rates['gpt-5.3-codex'], estimated: true };
  }
  return rates[normalized] ?? null;
}

export function estimateUsageCost(row: CostRow) {
  const rate = rateForModel(row.model);
  if (!rate) {
    return {
      estimatedCostUsd: 0,
      pricedTokens: 0,
      unpricedTokens: Number(row.totalTokens || 0),
      unknownModels: row.model ? [row.model] : ['unknown'],
      pricingModel: null,
      pricingEstimated: false,
    };
  }
  const input = Number(row.inputTokens || 0);
  const cached = Number(row.cachedInputTokens || 0);
  const output = Number(row.outputTokens || 0);
  const uncached = Math.max(input - cached, 0);
  return {
    estimatedCostUsd: (uncached * rate.input + cached * rate.cachedInput + output * rate.output) / 1_000_000,
    pricedTokens: input + output,
    unpricedTokens: 0,
    unknownModels: [],
    pricingModel: rate.pricingModel,
    pricingEstimated: rate.estimated,
  };
}

export function formatCost(value: number | null | undefined): string {
  return `$${Number(value || 0).toFixed(2)}`;
}
