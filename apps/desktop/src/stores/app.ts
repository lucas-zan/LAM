import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type { ThemeMode } from '../lib/theme';
import type { HealthCheck, TerminalTarget } from '../lib/types';
import type { Route } from '../routes/types';
import * as api from '../lib/api';

type Modal =
  | 'account'
  | 'renameAccount'
  | 'updatePatSession'
  | 'handoff'
  | 'sync'
  | 'provider'
  | 'attachProvider'
  | 'sessionDetail'
  | null;

interface AppState {
  route: Route;
  themeMode: ThemeMode;
  health: HealthCheck | null;
  status: string;
  error: string;
  appReady: boolean;
  modal: Modal;
  hideDockIcon: boolean;
  terminalTargets: TerminalTarget[];
  terminalTargetId: string;
  compactButtons: boolean;

  setRoute: (route: Route) => void;
  setThemeMode: (mode: ThemeMode) => void;
  setHealth: (health: HealthCheck) => void;
  setStatus: (status: string) => void;
  setError: (error: string) => void;
  clearError: () => void;
  setAppReady: () => void;
  openModal: (modal: NonNullable<Modal>) => void;
  closeModal: () => void;
  setHideDockIcon: (hide: boolean) => Promise<void>;
  setTerminalTargetId: (targetId: string) => Promise<void>;
  setCompactButtons: (compact: boolean) => void;
  loadSettings: () => Promise<void>;
}

export const useAppStore = create<AppState>()(
  subscribeWithSelector((set) => ({
    route: 'overview',
    themeMode: (() => {
      const saved = localStorage.getItem('lam-theme');
      if (saved === 'system' || saved === 'light' || saved === 'dark') return saved;
      return 'system' as ThemeMode;
    })(),
    health: null,
    status: 'Ready',
    error: '',
    appReady: false,
    modal: null,
    hideDockIcon: false,
    terminalTargets: [],
    terminalTargetId: 'terminal',
    compactButtons: (() => {
      const saved = localStorage.getItem('lam-compact-buttons');
      return saved === null ? true : saved === 'true';
    })(),

    setRoute: (route) => set({ route }),
    setThemeMode: (themeMode) => {
      localStorage.setItem('lam-theme', themeMode);
      set({ themeMode });
    },
    setHealth: (health) => set({ health }),
    setStatus: (status) => set({ status }),
    setError: (error) => set({ error }),
    clearError: () => set({ error: '' }),
    setAppReady: () => set({ appReady: true }),
    openModal: (modal) => set({ modal }),
    closeModal: () => set({ modal: null }),
    setHideDockIcon: async (hide) => {
      try {
        await api.setHideDockIcon(hide);
        set({ hideDockIcon: hide });
      } catch (err) {
        set({ error: err instanceof Error ? err.message : 'Failed to set Dock icon visibility' });
      }
    },
    setTerminalTargetId: async (targetId) => {
      try {
        await api.setSelectedTerminalTarget(targetId);
        set({ terminalTargetId: targetId });
      } catch (err) {
        set({ error: err instanceof Error ? err.message : 'Failed to set handoff terminal' });
      }
    },
    setCompactButtons: (compact) => {
      localStorage.setItem('lam-compact-buttons', String(compact));
      set({ compactButtons: compact });
    },
    loadSettings: async () => {
      try {
        const [hide, terminalTargets, terminalTargetId] = await Promise.all([
          api.getHideDockIcon(),
          api.listTerminalTargets(),
          api.getSelectedTerminalTarget(),
        ]);
        set({ hideDockIcon: hide, terminalTargets, terminalTargetId });
      } catch (err) {
        console.error('Failed to load settings:', err);
      }
    },
  })),
);

export type { Modal };
