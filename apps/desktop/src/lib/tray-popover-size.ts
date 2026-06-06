import { LogicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { inTauri } from './api';

export const TRAY_POPOVER_WIDTH = 328;
export const TRAY_POPOVER_MIN_HEIGHT = 248;
export const TRAY_POPOVER_MAX_HEIGHT = 720;
const TRAY_POPOVER_HEIGHT_PADDING = 10;

export function measureTrayPopoverHeight(panel: HTMLElement): number {
  const children = Array.from(panel.children) as HTMLElement[];
  const contentHeight = children.reduce((total, child) => {
    const style = window.getComputedStyle(child);
    const rect = child.getBoundingClientRect();
    return total + rect.height + parseFloat(style.marginTop) + parseFloat(style.marginBottom);
  }, 0);
  return Math.ceil(
    Math.max(
      contentHeight,
      panel.scrollHeight,
      panel.getBoundingClientRect().height,
      document.body.scrollHeight,
      document.documentElement.scrollHeight,
    ) + TRAY_POPOVER_HEIGHT_PADDING,
  );
}

export async function syncTrayPopoverWindowSize(panel: HTMLElement | null): Promise<void> {
  if (!inTauri() || !panel) return;
  const height = measureTrayPopoverHeight(panel);
  const availableHeight = Math.max(220, window.screen.availHeight - 48);
  const next = Math.min(
    TRAY_POPOVER_MAX_HEIGHT,
    availableHeight,
    Math.max(TRAY_POPOVER_MIN_HEIGHT, height),
  );
  try {
    await getCurrentWindow().setSize(new LogicalSize(TRAY_POPOVER_WIDTH, next));
  } catch (err) {
    console.warn('LAM: tray popover resize failed', err);
  }
}

export function scheduleTrayPopoverWindowSize(panel: HTMLElement | null): void {
  if (!panel) return;
  const run = () => {
    void syncTrayPopoverWindowSize(panel);
  };
  run();
  requestAnimationFrame(() => {
    run();
    requestAnimationFrame(run);
  });
  window.setTimeout(run, 60);
  window.setTimeout(run, 180);
}
