import { describe, it, expect, beforeEach } from "vitest";
import { useAppStore } from "./app";

beforeEach(() => {
  useAppStore.setState({
    route: "overview",
    status: "Ready",
    error: "",
    appReady: false,
    modal: null,
  });
});

describe("useAppStore", () => {
  it("sets route", () => {
    useAppStore.getState().setRoute("sessions");
    expect(useAppStore.getState().route).toBe("sessions");
  });

  it("manages error state", () => {
    useAppStore.getState().setError("something broke");
    expect(useAppStore.getState().error).toBe("something broke");
    useAppStore.getState().clearError();
    expect(useAppStore.getState().error).toBe("");
  });

  it("manages modal lifecycle", () => {
    expect(useAppStore.getState().modal).toBeNull();
    useAppStore.getState().openModal("account");
    expect(useAppStore.getState().modal).toBe("account");
    useAppStore.getState().closeModal();
    expect(useAppStore.getState().modal).toBeNull();
  });

  it("tracks appReady", () => {
    expect(useAppStore.getState().appReady).toBe(false);
    useAppStore.getState().setAppReady();
    expect(useAppStore.getState().appReady).toBe(true);
  });
});
