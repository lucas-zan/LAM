import { create } from "zustand";
import * as api from "../lib/api";
import type { AttachProviderRequest, CreateProviderRequest, ProviderProfile } from "../lib/types";
import { useAppStore } from "./app";
import { useAccountStore } from "./accounts";
import { formatError } from "../lib/format";

interface ProviderState {
  providers: ProviderProfile[];

  setProviders: (providers: ProviderProfile[]) => void;
  testProvider: (providerId: string) => Promise<void>;
  removeProvider: (providerId: string) => Promise<void>;
  createFromModal: (req: CreateProviderRequest) => Promise<void>;
  attachToProfile: (req: AttachProviderRequest) => Promise<void>;
}

export const useProviderStore = create<ProviderState>()((set) => ({
  providers: [],

  setProviders: (providers) => set({ providers }),

  testProvider: async (providerId) => {
    const updated = await api.testProvider(providerId);
    set((s) => ({ providers: s.providers.map((p) => (p.id === providerId ? updated : p)) }));
    useAppStore.getState().setStatus(`Provider ${providerId} health: ${updated.health}`);
  },

  removeProvider: async (providerId) => {
    const confirmed = window.confirm(
      `Delete provider "${providerId}"? Profiles using it will no longer resolve this provider.`,
    );
    if (!confirmed) return;
    try {
      await api.deleteProvider(providerId);
      set({ providers: await api.listProviders() });
      await useAccountStore.getState().refresh();
      useAppStore.getState().setStatus(`Provider ${providerId} deleted`);
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    }
  },

  createFromModal: async (req) => {
    const envKey = req.envKey?.trim() || null;
    await api.createProvider({
      ...req,
      envKey,
      secret: envKey ? { kind: "env", envKey } : { kind: "none" },
    });
    set({ providers: await api.listProviders() });
    useAppStore.getState().closeModal();
    useAppStore.getState().setStatus("Provider created");
  },

  attachToProfile: async (req) => {
    await api.attachProviderToProfile(req);
    useAppStore.getState().closeModal();
    await useAccountStore.getState().refresh();
    useAppStore.getState().setStatus(`Attached ${req.providerId} to ${req.profileId}`);
  },
}));
