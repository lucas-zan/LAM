import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useAppStore } from './app';
import * as api from '../lib/api';

vi.mock('../lib/api', () => ({
  getHideDockIcon: vi.fn(),
  setHideDockIcon: vi.fn(),
  listTerminalTargets: vi.fn(),
  getSelectedTerminalTarget: vi.fn(),
  setSelectedTerminalTarget: vi.fn(),
  getGatewayFirstResponseTimeoutSeconds: vi.fn(),
  setGatewayFirstResponseTimeoutSeconds: vi.fn(),
  getAntigravityPort: vi.fn(),
  setAntigravityPort: vi.fn(),
}));

const TERMINAL_APP = {
  id: 'terminal',
  displayName: 'Terminal.app',
  kind: 'terminal',
  installed: true,
};
const CMUX = { id: 'cmux', displayName: 'cmux', kind: 'terminal', installed: true };

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.setState({
    route: 'overview',
    status: 'Ready',
    error: '',
    appReady: false,
    modal: null,
    terminalTargets: [],
  });
});

describe('useAppStore', () => {
  it('sets route', () => {
    useAppStore.getState().setRoute('sessions');
    expect(useAppStore.getState().route).toBe('sessions');
  });

  it('manages error state', () => {
    useAppStore.getState().setError('something broke');
    expect(useAppStore.getState().error).toBe('something broke');
    useAppStore.getState().clearError();
    expect(useAppStore.getState().error).toBe('');
  });

  it('manages modal lifecycle', () => {
    expect(useAppStore.getState().modal).toBeNull();
    useAppStore.getState().openModal('account');
    expect(useAppStore.getState().modal).toBe('account');
    useAppStore.getState().closeModal();
    expect(useAppStore.getState().modal).toBeNull();
  });

  it('opens the dedicated External API modal', () => {
    useAppStore.getState().openModal('externalApi');
    expect(useAppStore.getState().modal).toBe('externalApi');
  });

  it('tracks appReady', () => {
    expect(useAppStore.getState().appReady).toBe(false);
    useAppStore.getState().setAppReady();
    expect(useAppStore.getState().appReady).toBe(true);
  });

  it('re-probes terminal targets so newly installed terminals appear', async () => {
    useAppStore.setState({ terminalTargets: [TERMINAL_APP] });
    vi.mocked(api.listTerminalTargets).mockResolvedValue([TERMINAL_APP, CMUX]);

    await useAppStore.getState().refreshTerminalTargets();

    expect(api.listTerminalTargets).toHaveBeenCalledTimes(1);
    expect(useAppStore.getState().terminalTargets).toEqual([TERMINAL_APP, CMUX]);
  });

  it('keeps the previous terminal targets when re-probing fails', async () => {
    useAppStore.setState({ terminalTargets: [TERMINAL_APP] });
    vi.mocked(api.listTerminalTargets).mockRejectedValue(new Error('probe failed'));

    await useAppStore.getState().refreshTerminalTargets();

    expect(useAppStore.getState().terminalTargets).toEqual([TERMINAL_APP]);
  });
});
