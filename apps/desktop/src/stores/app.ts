import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type { ThemeMode } from '../lib/theme';
import type { HealthCheck } from '../lib/types';
import type { Route } from '../routes/types';

type Modal =
  | 'account'
  | 'renameAccount'
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

  setRoute: (route: Route) => void;
  setThemeMode: (mode: ThemeMode) => void;
  setHealth: (health: HealthCheck) => void;
  setStatus: (status: string) => void;
  setError: (error: string) => void;
  clearError: () => void;
  setAppReady: () => void;
  openModal: (modal: NonNullable<Modal>) => void;
  closeModal: () => void;
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
  })),
);

export type { Modal };
