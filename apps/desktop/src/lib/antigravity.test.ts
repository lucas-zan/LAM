import { describe, expect, it } from 'vitest';
import { groupAntigravityModels } from './antigravity';
import type { AntigravityQuotaResponse } from './types';

const quota: AntigravityQuotaResponse = {
  ok: true,
  models: [
    { label: 'Gemini Flash', remainingFraction: 0.8 },
    { label: 'Claude Sonnet', remainingFraction: 0.7 },
    { label: 'GPT-OSS', remainingFraction: 0.6 },
    { label: 'Unknown Preview', remainingFraction: 0.5 },
  ],
  groups: [
    {
      displayName: 'Gemini Models',
      description: 'Models within this group: Gemini Flash, Gemini Pro',
      buckets: [
        { bucketId: 'gemini-weekly', displayName: 'Weekly Limit', window: 'weekly' },
        { bucketId: 'gemini-5h', displayName: '5h', window: '5h' },
      ],
    },
    {
      displayName: 'Claude and GPT models',
      description: 'Models within this group: Claude Opus, Claude Sonnet, GPT-OSS',
      buckets: [
        { bucketId: '3p-weekly', displayName: 'Weekly Limit', window: 'weekly' },
        { bucketId: '3p-5h', displayName: 'Five Hour Limit', window: '5h' },
      ],
    },
  ],
};

describe('groupAntigravityModels', () => {
  it('groups flat model rows under quota summary groups', () => {
    const groups = groupAntigravityModels(quota);

    expect(groups[0].group.displayName).toBe('Gemini Models');
    expect(groups[0].models.map((model) => model.label)).toEqual(['Gemini Flash']);
    expect(groups[1].group.displayName).toBe('Claude and GPT models');
    expect(groups[1].models.map((model) => model.label)).toEqual(['Claude Sonnet', 'GPT-OSS']);
  });

  it('keeps unmatched model rows visible', () => {
    const groups = groupAntigravityModels(quota);
    const otherGroup = groups[groups.length - 1];

    expect(otherGroup.group.displayName).toBe('Other models');
    expect(otherGroup.models.map((model) => model.label)).toEqual(['Unknown Preview']);
  });
});
