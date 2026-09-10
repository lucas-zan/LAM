import type {
  AntigravityModelQuota,
  AntigravityQuotaBucket,
  AntigravityQuotaGroup,
  AntigravityQuotaResponse,
} from './types';

export type AntigravityGroupedModels = {
  group: AntigravityQuotaGroup;
  models: AntigravityModelQuota[];
};

const GROUP_KEYWORDS: Array<{ pattern: RegExp; terms: string[] }> = [
  { pattern: /gemini/i, terms: ['gemini'] },
  { pattern: /claude|gpt/i, terms: ['claude', 'gpt', 'gpt-oss', 'gpt oss'] },
];

export function groupAntigravityModels(
  quota: Pick<AntigravityQuotaResponse, 'groups' | 'models'>,
): AntigravityGroupedModels[] {
  const groups = quota.groups ?? [];
  if (groups.length === 0) return [];

  const grouped = groups.map((group) => ({ group, models: [] as AntigravityModelQuota[] }));
  const unmatched: AntigravityModelQuota[] = [];

  for (const model of quota.models) {
    const index = grouped.findIndex(({ group }) => modelMatchesGroup(model, group));
    if (index >= 0) {
      grouped[index].models.push(model);
    } else {
      unmatched.push(model);
    }
  }

  if (unmatched.length > 0) {
    grouped.push({
      group: {
        displayName: 'Other models',
        description: null,
        buckets: firstBucketSet(groups),
      },
      models: unmatched,
    });
  }

  return grouped;
}

export function quotaBucketUsedPercent(bucket: AntigravityQuotaBucket): number | null {
  const remainingFraction = bucket.remainingFraction ?? null;
  if (remainingFraction === null) return null;
  return 100 - Math.round(remainingFraction * 100);
}

export function quotaBucketVariant(bucket: AntigravityQuotaBucket): 'session' | 'weekly' {
  return bucket.window === 'weekly' ? 'weekly' : 'session';
}

export function formatAntigravityBucketLabel(displayName: string): string {
  const lower = displayName.toLowerCase().trim();
  if (lower.includes('five') || lower === '5h' || lower.startsWith('5h')) {
    return '5h';
  }
  if (lower.includes('week')) {
    return 'weekly';
  }
  return displayName;
}

function modelMatchesGroup(model: AntigravityModelQuota, group: AntigravityQuotaGroup): boolean {
  const modelLabel = normalize(model.label);
  return groupTerms(group).some((term) => modelLabel.includes(term));
}

function groupTerms(group: AntigravityQuotaGroup): string[] {
  const source = `${group.displayName} ${group.description ?? ''}`;
  const terms = new Set<string>();

  for (const { pattern, terms: knownTerms } of GROUP_KEYWORDS) {
    if (pattern.test(source)) {
      knownTerms.forEach((term) => terms.add(normalize(term)));
    }
  }

  extractDescriptionModels(group.description).forEach((term) => terms.add(normalize(term)));
  return Array.from(terms).filter(Boolean);
}

function extractDescriptionModels(description?: string | null): string[] {
  if (!description) return [];
  const [, modelList = ''] = description.split(':');
  return modelList
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function firstBucketSet(groups: AntigravityQuotaGroup[]): AntigravityQuotaBucket[] {
  return groups.find((group) => group.buckets.length > 0)?.buckets ?? [];
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim();
}
