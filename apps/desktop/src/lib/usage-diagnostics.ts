import type { UsageDiagnostics } from './types';

export function summarizeUsageDiagnostics(diagnostics: UsageDiagnostics): string {
  const parts: string[] = [];
  const parser = Object.entries(diagnostics.parserDiagnostics || {})
    .filter(([, count]) => Number(count) > 0)
    .map(([key, count]) => `${key}: ${count}`);
  if (parser.length) parts.push(parser.join(', '));
  if (diagnostics.skippedEvents) parts.push(`skipped: ${diagnostics.skippedEvents}`);
  if (diagnostics.unknownModels.length) {
    parts.push(`unknown models: ${diagnostics.unknownModels.join(', ')}`);
  }
  if (diagnostics.lowCacheThreads.length) {
    parts.push(`low cache: ${diagnostics.lowCacheThreads.map((thread) => thread.threadLabel).join(', ')}`);
  }
  if (diagnostics.highContextCalls.length) {
    parts.push(`high context: ${diagnostics.highContextCalls.map((call) => call.recordId).join(', ')}`);
  }
  if (diagnostics.lastRefreshError) parts.push(`last error: ${diagnostics.lastRefreshError}`);
  return parts.join(' · ') || 'No aggregate diagnostics.';
}
