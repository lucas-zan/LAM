import { beforeEach, vi } from 'vitest';

// Mock localStorage globally for all tests
const localStorageMock = {
  store: {} as Record<string, string>,
  getItem(key: string) {
    return this.store[key] || null;
  },
  setItem(key: string, value: string) {
    this.store[key] = value;
  },
  removeItem(key: string) {
    delete this.store[key];
  },
  clear() {
    this.store = {};
  },
  get length() {
    return Object.keys(this.store).length;
  },
  key(index: number) {
    const keys = Object.keys(this.store);
    return keys[index] || null;
  },
};

// Install mock immediately at module load time (before any imports)
vi.stubGlobal('localStorage', localStorageMock);

// Clear storage before each test
beforeEach(() => {
  localStorageMock.clear();
});
