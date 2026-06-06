# LocalAgentManager (Lam) — local desktop launcher
#
# Requires Node/npm and Rust/Cargo. `make start` launches the Tauri desktop app
# in the foreground; Ctrl+C stops the dev runtime.

.DEFAULT_GOAL := start

NPM          ?= npm
APP_DIR      := apps/desktop
TAURI_DIR    := apps/desktop/src-tauri
LAM_HOME     ?=
LAM_ENV      := $(if $(LAM_HOME),LAM_HOME=$(LAM_HOME),)

.PHONY: help app desktop start stop status accounts check test build install node-check cargo-check tauri-info clean

help:
	@echo "LocalAgentManager (Lam) — local development"
	@echo ""
	@echo "  make start       Start the Tauri desktop app in foreground"
	@echo "  make app         Same as make start"
	@echo "  make desktop     Same as make start"
	@echo "  make stop        Stop Tauri/Vite dev runtimes started from this repo"
	@echo "  make status      Show local tool/runtime status"
	@echo "  make accounts    Print detected local Codex accounts"
	@echo "  make check       Run frontend build, UI smoke, Rust fmt check, Rust tests"
	@echo "  make test        Same as make check"
	@echo "  make build       Build frontend and Tauri bundle"
	@echo "  make install     npm install in apps/desktop"
	@echo ""
	@echo "Optional:"
	@echo "  LAM_HOME=.fake-home make start"

app desktop start: install node-check cargo-check
	@echo "Starting LocalAgentManager native Tauri desktop app..."
	@echo "Note: Vite is only the renderer dev server loaded by the desktop window."
	cd $(APP_DIR) && $(LAM_ENV) $(NPM) run tauri:dev

install:
	@if [ ! -d "$(APP_DIR)/node_modules" ]; then \
		echo "node_modules missing — running npm install in $(APP_DIR)"; \
		cd $(APP_DIR) && $(NPM) install; \
	fi

node-check:
	@command -v node >/dev/null 2>&1 || (echo "node not found" >&2; exit 1)
	@command -v $(NPM) >/dev/null 2>&1 || (echo "npm not found" >&2; exit 1)

cargo-check:
	@command -v cargo >/dev/null 2>&1 || (echo "cargo not found" >&2; exit 1)

stop:
	-@pkill -f "$(APP_DIR).*tauri" 2>/dev/null || true
	-@pkill -f "$(APP_DIR).*vite" 2>/dev/null || true
	-@pkill -f "target/debug/localagentmanager" 2>/dev/null || true
	@echo "Stopped Lam dev runtimes if they were running."

status: install node-check cargo-check
	@echo "Node: $$(node --version)"
	@echo "npm:  $$(npm --version)"
	@echo "Rust: $$(rustc --version)"
	cd $(APP_DIR) && $(NPM) run tauri -- info

accounts: cargo-check
	cd $(TAURI_DIR) && $(LAM_ENV) cargo run --bin lam-core

check test: install node-check cargo-check
	cd $(APP_DIR) && $(NPM) run build
	cd $(APP_DIR) && $(NPM) run test:ui
	cd $(TAURI_DIR) && cargo fmt -- --check
	cd $(TAURI_DIR) && cargo clippy -- -D warnings
	cd $(TAURI_DIR) && cargo test

tauri-info: status

build: install node-check cargo-check
	cd $(APP_DIR) && $(NPM) run tauri:build

clean:
	-@rm -rf $(APP_DIR)/dist
	-@rm -rf $(TAURI_DIR)/target
