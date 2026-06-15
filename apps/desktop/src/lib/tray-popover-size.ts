import { LogicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { inTauri } from './api';

export const TRAY_POPOVER_WIDTH = 328;
export const TRAY_POPOVER_MIN_HEIGHT = 248;
export const TRAY_POPOVER_MAX_HEIGHT = 620;
export function measureTrayPopoverHeight(panel: HTMLElement): number {
  const head = panel.querySelector<HTMLElement>(".trayPopoverFixedHead");
  const list = panel.querySelector<HTMLElement>(".trayAccountList");
  const foot = panel.querySelector<HTMLElement>(".trayPopoverFoot");

  const headHeight = head?.getBoundingClientRect().height ?? 0;
  const footHeight = foot?.getBoundingClientRect().height ?? 0;
  // Use scrollHeight to get the full height of all accounts.
  // The max-height of `.trayAccountList` in CSS is 416px, so we cap it there.
  const listHeight = list ? Math.min(list.scrollHeight, 416) : 0;

  const contentHeight = headHeight + footHeight + listHeight;
  return Math.ceil(contentHeight);
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
