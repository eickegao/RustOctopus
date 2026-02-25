# RustOctopus Phase 3: Tauri GUI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a cross-platform desktop control console (macOS/Windows/Linux) that embeds the RustOctopus gateway as a system tray application.

**Architecture:** Tauri 2 app with React+TypeScript frontend. The Rust backend holds `AppState` containing Config, AgentLoop, ChannelManager, CronService, and MessageBus. Frontend communicates via Tauri IPC commands. Window close hides to tray; only "Quit" exits the process.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Tailwind CSS, `rustoctopus-core` workspace dep

**Design doc:** `docs/plans/2026-02-24-tauri-gui-design.md`

---

## Task 1: Scaffold Tauri Project

**Files:**
- Create: `crates/rustoctopus-app/Cargo.toml`
- Create: `crates/rustoctopus-app/src/main.rs`
- Create: `crates/rustoctopus-app/tauri.conf.json`
- Create: `crates/rustoctopus-app/build.rs`
- Create: `crates/rustoctopus-app/capabilities/default.json`
- Modify: `Cargo.toml` (workspace root — add member)

**Step 1: Add app crate to workspace**

In workspace root `Cargo.toml`, add `"crates/rustoctopus-app"` to `members`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/rustoctopus-core",
    "crates/rustoctopus-cli",
    "crates/rustoctopus-app",
]
```

**Step 2: Create `crates/rustoctopus-app/Cargo.toml`**

```toml
[package]
name = "rustoctopus-app"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
rustoctopus-core = { path = "../rustoctopus-core" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
```

**Step 3: Create `crates/rustoctopus-app/build.rs`**

```rust
fn main() {
    tauri_build::build()
}
```

**Step 4: Create minimal `crates/rustoctopus-app/src/main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 5: Create `crates/rustoctopus-app/tauri.conf.json`**

```json
{
  "$schema": "https://raw.githubusercontent.com/nicedoc/schemas/main/v2/tauri.conf.json",
  "productName": "RustOctopus",
  "version": "0.1.0",
  "identifier": "com.rustoctopus.app",
  "build": {
    "frontendDist": "../ui/dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "cd ui && npm run dev",
    "beforeBuildCommand": "cd ui && npm run build"
  },
  "app": {
    "windows": [
      {
        "title": "RustOctopus",
        "width": 1024,
        "height": 680,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "trayIcon": {
      "iconPath": "icons/icon.png",
      "iconAsTemplate": true
    },
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

**Step 6: Create `crates/rustoctopus-app/capabilities/default.json`**

```json
{
  "$schema": "https://raw.githubusercontent.com/nicedoc/schemas/main/v2/capability.json",
  "identifier": "default",
  "description": "Default capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-open"
  ]
}
```

**Step 7: Generate icons from `assets/icon.png`**

Run: `cd crates/rustoctopus-app && cargo tauri icon ../../assets/icon.png`

This generates all required icon sizes into `crates/rustoctopus-app/icons/`.

**Step 8: Verify Rust backend compiles**

Run: `cargo build -p rustoctopus-app`

Expected: compiles successfully (no frontend yet, will warn about missing `ui/dist`)

**Step 9: Commit**

```bash
git add crates/rustoctopus-app/ Cargo.toml
git commit -m "feat: scaffold Tauri app crate with tray icon support"
```

---

## Task 2: Scaffold React Frontend

**Files:**
- Create: `crates/rustoctopus-app/ui/package.json`
- Create: `crates/rustoctopus-app/ui/tsconfig.json`
- Create: `crates/rustoctopus-app/ui/vite.config.ts`
- Create: `crates/rustoctopus-app/ui/tailwind.config.js`
- Create: `crates/rustoctopus-app/ui/postcss.config.js`
- Create: `crates/rustoctopus-app/ui/index.html`
- Create: `crates/rustoctopus-app/ui/src/main.tsx`
- Create: `crates/rustoctopus-app/ui/src/App.tsx`
- Create: `crates/rustoctopus-app/ui/src/index.css`

**Step 1: Initialize npm project**

Run from `crates/rustoctopus-app/ui`:

```bash
npm init -y
npm install react react-dom react-router-dom
npm install -D typescript @types/react @types/react-dom \
  vite @vitejs/plugin-react \
  tailwindcss @tailwindcss/vite \
  @tauri-apps/api @tauri-apps/plugin-shell
```

**Step 2: Create `vite.config.ts`**

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
```

**Step 3: Create `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2021",
    "lib": ["ES2021", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true
  },
  "include": ["src"]
}
```

**Step 4: Create `index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>RustOctopus</title>
  </head>
  <body class="bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

**Step 5: Create `src/index.css`**

```css
@import "tailwindcss";
```

**Step 6: Create `src/main.tsx`**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

**Step 7: Create `src/App.tsx`** (placeholder with sidebar skeleton)

```tsx
import { BrowserRouter, Routes, Route, NavLink } from "react-router-dom";

function Placeholder({ name }: { name: string }) {
  return <div className="p-6 text-xl font-semibold">{name}</div>;
}

const navItems = [
  { to: "/", label: "Dashboard" },
  { to: "/config", label: "Config" },
  { to: "/channels", label: "Channels" },
  { to: "/cron", label: "Cron" },
];

export default function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen">
        {/* Sidebar */}
        <nav className="w-56 shrink-0 border-r border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 flex flex-col">
          <div className="p-4 text-lg font-bold tracking-tight">
            RustOctopus
          </div>
          <div className="flex-1 px-2 space-y-1">
            {navItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  `block px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                    isActive
                      ? "bg-indigo-50 dark:bg-indigo-900/30 text-indigo-700 dark:text-indigo-300"
                      : "text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700"
                  }`
                }
              >
                {item.label}
              </NavLink>
            ))}
          </div>
        </nav>

        {/* Content */}
        <main className="flex-1 overflow-y-auto">
          <Routes>
            <Route path="/" element={<Placeholder name="Dashboard" />} />
            <Route path="/config" element={<Placeholder name="Config" />} />
            <Route path="/channels" element={<Placeholder name="Channels" />} />
            <Route path="/cron" element={<Placeholder name="Cron" />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}
```

**Step 8: Verify frontend builds**

Run: `cd crates/rustoctopus-app/ui && npm run build`

(Add `"scripts": {"dev": "vite", "build": "vite build"}` to package.json if not present.)

Expected: `dist/` directory created with bundled output.

**Step 9: Verify full Tauri dev**

Run: `cd crates/rustoctopus-app && cargo tauri dev`

Expected: window opens showing sidebar with 4 nav items and placeholder content.

**Step 10: Commit**

```bash
git add crates/rustoctopus-app/ui/
git commit -m "feat: scaffold React frontend with sidebar navigation"
```

---

## Task 3: System Tray + Window Management

**Files:**
- Modify: `crates/rustoctopus-app/src/main.rs`

**Step 1: Implement tray and close-to-tray behavior**

Replace `src/main.rs` with:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Build tray menu
            let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            // Build tray icon
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide to tray on close instead of quitting
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 2: Verify tray behavior**

Run: `cd crates/rustoctopus-app && cargo tauri dev`

Test:
1. Window opens normally
2. Click window close → window hides, tray icon visible
3. Click tray icon → window reappears
4. Right-click tray → "Show Window" / "Quit" menu
5. Click "Quit" → app exits

**Step 3: Commit**

```bash
git add crates/rustoctopus-app/src/main.rs
git commit -m "feat: system tray with close-to-tray behavior"
```

---

## Task 4: AppState + Gateway Startup

**Files:**
- Create: `crates/rustoctopus-app/src/state.rs`
- Modify: `crates/rustoctopus-app/src/main.rs`

**Step 1: Create `src/state.rs`**

This manages all the core services. The gateway starts when the app launches.

```rust
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;
use tracing::info;

use rustoctopus_core::agent::agent_loop::AgentLoop;
use rustoctopus_core::bus::queue::MessageBus;
use rustoctopus_core::channels::{ChannelManager, FeishuChannel, TelegramChannel};
use rustoctopus_core::config::factory::{create_provider, resolve_workspace_path};
use rustoctopus_core::config::loader::{default_config_path, load_config, save_config};
use rustoctopus_core::config::schema::Config;
use rustoctopus_core::cron::CronService;

pub struct AppState {
    pub config: Mutex<Config>,
    pub cron: Mutex<CronService>,
    pub channel_names: Mutex<Vec<String>>,
    pub started_at: Instant,
}

impl AppState {
    /// Boot the gateway: load config, start agent + channels + cron.
    pub async fn boot() -> anyhow::Result<Arc<Self>> {
        let config = load_config(None)?;

        // Create bus
        let (bus, inbound_rx, outbound_rx) = MessageBus::new();

        // Create provider
        let provider = create_provider(&config)?;

        // Create agent
        let mut agent = AgentLoop::from_config(config.clone(), bus.clone(), provider, inbound_rx);

        // Create channel manager
        let mut channel_mgr = ChannelManager::new(bus.clone(), outbound_rx);

        if config.channels.telegram.enabled {
            let telegram = TelegramChannel::new(config.channels.telegram.clone(), bus.clone());
            channel_mgr.add_channel(Box::new(telegram));
            info!("Telegram channel registered");
        }

        if config.channels.feishu.enabled {
            let feishu = FeishuChannel::new(config.channels.feishu.clone(), bus.clone());
            channel_mgr.add_channel(Box::new(feishu));
            info!("Feishu channel registered");
        }

        #[cfg(feature = "whatsapp")]
        if config.channels.whatsapp.enabled {
            let whatsapp = rustoctopus_core::channels::WhatsAppChannel::new(
                config.channels.whatsapp.clone(),
                bus.clone(),
            );
            channel_mgr.add_channel(Box::new(whatsapp));
            info!("WhatsApp channel registered");
        }

        let names = channel_mgr.channel_names();
        channel_mgr.start_all().await?;

        // Cron
        let workspace = resolve_workspace_path(&config.agents.defaults.workspace);
        let cron_path = workspace.join("cron_jobs.json");
        let mut cron_service = CronService::new(cron_path);
        let _ = cron_service.start();

        info!(
            model = %config.agents.defaults.model,
            channels = ?names,
            cron_jobs = cron_service.status().job_count,
            "Gateway started"
        );

        // Spawn background tasks
        tokio::spawn(async move { agent.run().await });
        tokio::spawn(async move { channel_mgr.run_dispatch().await });

        Ok(Arc::new(Self {
            config: Mutex::new(config),
            cron: Mutex::new(cron_service),
            channel_names: Mutex::new(names),
            started_at: Instant::now(),
        }))
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}
```

**Step 2: Update `main.rs` to boot gateway and manage state**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    // Build tokio runtime for async gateway boot
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    let app_state = rt.block_on(async {
        state::AppState::boot().await.expect("failed to boot gateway")
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 3: Verify app starts with gateway**

Run: `cd crates/rustoctopus-app && cargo tauri dev`

Expected: logs show "Gateway started" with model/channels/cron info, window opens.

**Step 4: Commit**

```bash
git add crates/rustoctopus-app/src/
git commit -m "feat: boot embedded gateway on app startup with AppState"
```

---

## Task 5: IPC Commands — Config

**Files:**
- Create: `crates/rustoctopus-app/src/commands/mod.rs`
- Create: `crates/rustoctopus-app/src/commands/config.rs`
- Modify: `crates/rustoctopus-app/src/main.rs` (register commands)

**Step 1: Create `src/commands/mod.rs`**

```rust
pub mod config;
```

**Step 2: Create `src/commands/config.rs`**

```rust
use std::sync::Arc;

use tauri::State;

use rustoctopus_core::config::loader::save_config;
use rustoctopus_core::config::schema::Config;

use crate::state::AppState;

#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<Config, String> {
    let config = state.config.lock().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config_cmd(
    state: State<'_, Arc<AppState>>,
    config: Config,
) -> Result<(), String> {
    save_config(&config, None).map_err(|e| e.to_string())?;
    let mut current = state.config.lock().await;
    *current = config;
    Ok(())
}
```

**Step 3: Register commands in `main.rs`**

Add `mod commands;` at the top. Add `.invoke_handler(tauri::generate_handler![...])` to the builder:

```rust
mod commands;
mod state;

// ... in Builder:
.invoke_handler(tauri::generate_handler![
    commands::config::get_config,
    commands::config::save_config_cmd,
])
```

**Step 4: Verify compiles**

Run: `cargo build -p rustoctopus-app`

**Step 5: Commit**

```bash
git add crates/rustoctopus-app/src/commands/
git commit -m "feat: add get_config and save_config IPC commands"
```

---

## Task 6: IPC Commands — Dashboard + Channels + Cron

**Files:**
- Create: `crates/rustoctopus-app/src/commands/dashboard.rs`
- Create: `crates/rustoctopus-app/src/commands/channels.rs`
- Create: `crates/rustoctopus-app/src/commands/cron.rs`
- Modify: `crates/rustoctopus-app/src/commands/mod.rs`
- Modify: `crates/rustoctopus-app/src/main.rs` (register commands)

**Step 1: Create `src/commands/dashboard.rs`**

```rust
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub model: String,
    pub uptime_secs: u64,
    pub channels: Vec<String>,
    pub cron_job_count: usize,
    pub cron_enabled_count: usize,
    pub cron_next_fire_ms: Option<i64>,
}

#[tauri::command]
pub async fn get_status(state: State<'_, Arc<AppState>>) -> Result<StatusInfo, String> {
    let config = state.config.lock().await;
    let names = state.channel_names.lock().await;
    let cron = state.cron.lock().await;
    let cron_status = cron.status();

    Ok(StatusInfo {
        model: config.agents.defaults.model.clone(),
        uptime_secs: state.uptime_secs(),
        channels: names.clone(),
        cron_job_count: cron_status.job_count,
        cron_enabled_count: cron_status.enabled_count,
        cron_next_fire_ms: cron_status.next_fire_at_ms,
    })
}
```

**Step 2: Create `src/commands/channels.rs`**

```rust
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn get_channel_status(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ChannelInfo>, String> {
    let config = state.config.lock().await;
    let active_names = state.channel_names.lock().await;

    let mut channels = Vec::new();

    channels.push(ChannelInfo {
        name: "telegram".to_string(),
        enabled: active_names.contains(&"telegram".to_string()),
    });
    channels.push(ChannelInfo {
        name: "feishu".to_string(),
        enabled: active_names.contains(&"feishu".to_string()),
    });
    channels.push(ChannelInfo {
        name: "whatsapp".to_string(),
        enabled: active_names.contains(&"whatsapp".to_string()),
    });

    // Suppress unused-variable warning
    let _ = &config;

    Ok(channels)
}
```

**Step 3: Create `src/commands/cron.rs`**

```rust
use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use rustoctopus_core::cron::{CronJob, CronSchedule, ScheduleKind};

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddJobRequest {
    pub name: String,
    pub message: String,
    pub schedule_kind: String,   // "every" | "cron"
    pub every_ms: Option<i64>,
    pub cron_expr: Option<String>,
}

#[tauri::command]
pub async fn list_cron_jobs(state: State<'_, Arc<AppState>>) -> Result<Vec<CronJob>, String> {
    let cron = state.cron.lock().await;
    Ok(cron.list_jobs(true))
}

#[tauri::command]
pub async fn add_cron_job(
    state: State<'_, Arc<AppState>>,
    req: AddJobRequest,
) -> Result<CronJob, String> {
    let schedule = match req.schedule_kind.as_str() {
        "every" => {
            let ms = req.every_ms.ok_or("every_ms required for 'every' schedule")?;
            CronSchedule::every(ms)
        }
        "cron" => {
            let expr = req.cron_expr.as_deref().ok_or("cron_expr required for 'cron' schedule")?;
            CronSchedule::cron_expr(expr, None)
        }
        other => return Err(format!("unknown schedule kind: {}", other)),
    };

    let mut cron = state.cron.lock().await;
    cron.add_job(&req.name, schedule, &req.message, true, None, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_cron_job(
    state: State<'_, Arc<AppState>>,
    job_id: String,
) -> Result<bool, String> {
    let mut cron = state.cron.lock().await;
    Ok(cron.remove_job(&job_id))
}

#[tauri::command]
pub async fn toggle_cron_job(
    state: State<'_, Arc<AppState>>,
    job_id: String,
) -> Result<bool, String> {
    let mut cron = state.cron.lock().await;
    let jobs = cron.list_jobs(true);
    let job = jobs.iter().find(|j| j.id == job_id);
    match job {
        Some(j) => Ok(cron.enable_job(&job_id, !j.enabled)),
        None => Err(format!("job not found: {}", job_id)),
    }
}
```

**Step 4: Update `src/commands/mod.rs`**

```rust
pub mod channels;
pub mod config;
pub mod cron;
pub mod dashboard;
```

**Step 5: Register all commands in `main.rs`**

```rust
.invoke_handler(tauri::generate_handler![
    commands::config::get_config,
    commands::config::save_config_cmd,
    commands::dashboard::get_status,
    commands::channels::get_channel_status,
    commands::cron::list_cron_jobs,
    commands::cron::add_cron_job,
    commands::cron::remove_cron_job,
    commands::cron::toggle_cron_job,
])
```

**Step 6: Verify compiles**

Run: `cargo build -p rustoctopus-app`

**Step 7: Commit**

```bash
git add crates/rustoctopus-app/src/
git commit -m "feat: add dashboard, channels, and cron IPC commands"
```

---

## Task 7: Frontend — Dashboard View

**Files:**
- Create: `crates/rustoctopus-app/ui/src/lib/invoke.ts`
- Create: `crates/rustoctopus-app/ui/src/views/Dashboard.tsx`
- Create: `crates/rustoctopus-app/ui/src/components/StatusCard.tsx`
- Modify: `crates/rustoctopus-app/ui/src/App.tsx`

**Step 1: Create `src/lib/invoke.ts`** — typed wrapper around Tauri invoke

```typescript
import { invoke } from "@tauri-apps/api/core";

export interface StatusInfo {
  model: string;
  uptimeSecs: number;
  channels: string[];
  cronJobCount: number;
  cronEnabledCount: number;
  cronNextFireMs: number | null;
}

export interface ChannelInfo {
  name: string;
  enabled: boolean;
}

export interface CronJob {
  id: string;
  name: string;
  enabled: boolean;
  schedule: {
    kind: "at" | "every" | "cron";
    atMs: number | null;
    everyMs: number | null;
    expr: string | null;
    tz: string | null;
  };
  payload: {
    kind: string;
    message: string;
    deliver: boolean;
    channel: string | null;
    to: string | null;
  };
  state: {
    nextRunAtMs: number | null;
    lastRunAtMs: number | null;
    lastStatus: string | null;
    lastError: string | null;
  };
  createdAtMs: number;
  updatedAtMs: number;
  deleteAfterRun: boolean;
}

export interface Config {
  agents: { defaults: AgentDefaults };
  channels: ChannelsConfig;
  providers: Record<string, ProviderConfig>;
  gateway: { host: string; port: number };
  tools: ToolsConfig;
}

export interface AgentDefaults {
  workspace: string;
  model: string;
  maxTokens: number;
  temperature: number;
  maxToolIterations: number;
  memoryWindow: number;
}

export interface ChannelsConfig {
  sendProgress: boolean;
  sendToolHints: boolean;
  telegram: { enabled: boolean; token: string; allowFrom: string[]; proxy: string | null; replyToMessage: boolean };
  feishu: { enabled: boolean; appId: string; appSecret: string; encryptKey: string; verificationToken: string; allowFrom: string[] };
  whatsapp: { enabled: boolean; allowFrom: string[]; bridgePort: number; bridgeToken: string | null; autoStartBridge: boolean };
}

export interface ProviderConfig {
  apiKey: string;
  apiBase: string | null;
  extraHeaders: Record<string, string> | null;
}

export interface ToolsConfig {
  web: { search: { apiKey: string; maxResults: number } };
  exec: { timeout: number };
  restrictToWorkspace: boolean;
}

export const api = {
  getStatus: () => invoke<StatusInfo>("get_status"),
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config_cmd", { config }),
  getChannelStatus: () => invoke<ChannelInfo[]>("get_channel_status"),
  listCronJobs: () => invoke<CronJob[]>("list_cron_jobs"),
  addCronJob: (req: { name: string; message: string; scheduleKind: string; everyMs?: number; cronExpr?: string }) =>
    invoke<CronJob>("add_cron_job", { req }),
  removeCronJob: (jobId: string) => invoke<boolean>("remove_cron_job", { jobId }),
  toggleCronJob: (jobId: string) => invoke<boolean>("toggle_cron_job", { jobId }),
};
```

**Step 2: Create `src/components/StatusCard.tsx`**

```tsx
interface Props {
  label: string;
  value: string | number;
  sub?: string;
}

export default function StatusCard({ label, value, sub }: Props) {
  return (
    <div className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4">
      <div className="text-sm text-gray-500 dark:text-gray-400">{label}</div>
      <div className="mt-1 text-2xl font-semibold">{value}</div>
      {sub && <div className="mt-0.5 text-xs text-gray-400">{sub}</div>}
    </div>
  );
}
```

**Step 3: Create `src/views/Dashboard.tsx`**

```tsx
import { useEffect, useState } from "react";
import { api, StatusInfo } from "../lib/invoke";
import StatusCard from "../components/StatusCard";

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m ${s}s` : `${s}s`;
}

export default function Dashboard() {
  const [status, setStatus] = useState<StatusInfo | null>(null);

  useEffect(() => {
    const load = () => api.getStatus().then(setStatus).catch(console.error);
    load();
    const interval = setInterval(load, 5000);
    return () => clearInterval(interval);
  }, []);

  if (!status) return <div className="p-6">Loading...</div>;

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Dashboard</h1>

      <div className="grid grid-cols-3 gap-4">
        <StatusCard label="Model" value={status.model} />
        <StatusCard label="Uptime" value={formatUptime(status.uptimeSecs)} />
        <StatusCard
          label="Cron Jobs"
          value={status.cronEnabledCount}
          sub={`${status.cronJobCount} total`}
        />
      </div>

      <div>
        <h2 className="text-lg font-semibold mb-3">Channels</h2>
        <div className="space-y-2">
          {status.channels.length === 0 ? (
            <div className="text-gray-400 text-sm">No channels active</div>
          ) : (
            status.channels.map((ch) => (
              <div
                key={ch}
                className="flex items-center gap-2 px-3 py-2 rounded-md bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700"
              >
                <span className="h-2 w-2 rounded-full bg-green-400" />
                <span className="text-sm font-medium">{ch}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
```

**Step 4: Update `App.tsx` to use real views**

Replace the Dashboard placeholder route:

```tsx
import Dashboard from "./views/Dashboard";

// In Routes:
<Route path="/" element={<Dashboard />} />
```

**Step 5: Verify**

Run: `cd crates/rustoctopus-app && cargo tauri dev`

Expected: Dashboard shows model name, uptime counter, active channels, cron summary. Uptime refreshes every 5s.

**Step 6: Commit**

```bash
git add crates/rustoctopus-app/ui/src/
git commit -m "feat: Dashboard view with status cards and channel list"
```

---

## Task 8: Frontend — Config View

**Files:**
- Create: `crates/rustoctopus-app/ui/src/views/Config.tsx`
- Modify: `crates/rustoctopus-app/ui/src/App.tsx`

**Step 1: Create `src/views/Config.tsx`**

```tsx
import { useEffect, useState } from "react";
import { api, Config } from "../lib/invoke";

export default function ConfigView() {
  const [config, setConfig] = useState<Config | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.getConfig().then(setConfig).catch(console.error);
  }, []);

  if (!config) return <div className="p-6">Loading...</div>;

  const update = (fn: (c: Config) => void) => {
    const next = structuredClone(config);
    fn(next);
    setConfig(next);
    setSaved(false);
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await api.saveConfig(config);
      setSaved(true);
    } catch (e) {
      console.error(e);
      alert("Save failed: " + e);
    }
    setSaving(false);
  };

  const d = config.agents.defaults;

  return (
    <div className="p-6 space-y-8 max-w-2xl">
      <h1 className="text-2xl font-bold">Configuration</h1>

      {/* Agent */}
      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Agent</h2>
        <Field label="Model" value={d.model} onChange={(v) => update((c) => (c.agents.defaults.model = v))} />
        <Field label="Max Tokens" value={String(d.maxTokens)} onChange={(v) => update((c) => (c.agents.defaults.maxTokens = Number(v)))} type="number" />
        <RangeField label="Temperature" value={d.temperature} min={0} max={2} step={0.1} onChange={(v) => update((c) => (c.agents.defaults.temperature = v))} />
        <Field label="Memory Window" value={String(d.memoryWindow)} onChange={(v) => update((c) => (c.agents.defaults.memoryWindow = Number(v)))} type="number" />
      </section>

      {/* Providers */}
      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Providers</h2>
        {Object.entries(config.providers).map(([name, prov]) => (
          <Field
            key={name}
            label={name}
            value={prov.apiKey}
            onChange={(v) => update((c) => ((c.providers as any)[name].apiKey = v))}
            type="password"
            placeholder="API Key"
          />
        ))}
      </section>

      {/* Tools */}
      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Tools</h2>
        <Field label="Exec Timeout (sec)" value={String(config.tools.exec.timeout)} onChange={(v) => update((c) => (c.tools.exec.timeout = Number(v)))} type="number" />
        <Field label="Web Search API Key" value={config.tools.web.search.apiKey} onChange={(v) => update((c) => (c.tools.web.search.apiKey = v))} type="password" />
      </section>

      <button
        onClick={handleSave}
        disabled={saving}
        className="px-4 py-2 rounded-md bg-indigo-600 text-white font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
      >
        {saving ? "Saving..." : saved ? "Saved!" : "Save"}
      </button>
    </div>
  );
}

function Field({ label, value, onChange, type = "text", placeholder }: {
  label: string; value: string; onChange: (v: string) => void; type?: string; placeholder?: string;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-1">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent outline-none"
      />
    </div>
  );
}

function RangeField({ label, value, min, max, step, onChange }: {
  label: string; value: number; min: number; max: number; step: number; onChange: (v: number) => void;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-1">
        {label}: {value}
      </label>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full"
      />
    </div>
  );
}
```

**Step 2: Update `App.tsx`**

```tsx
import ConfigView from "./views/Config";

// In Routes:
<Route path="/config" element={<ConfigView />} />
```

**Step 3: Verify**

Run: `cargo tauri dev`

Test: Navigate to Config → fields populated from `config.json` → change temperature → click Save → verify `~/.rustoctopus/config.json` updated.

**Step 4: Commit**

```bash
git add crates/rustoctopus-app/ui/src/
git commit -m "feat: Config view with provider keys, agent params, and save"
```

---

## Task 9: Frontend — Channels View

**Files:**
- Create: `crates/rustoctopus-app/ui/src/views/Channels.tsx`
- Modify: `crates/rustoctopus-app/ui/src/App.tsx`

**Step 1: Create `src/views/Channels.tsx`**

```tsx
import { useEffect, useState } from "react";
import { api, ChannelInfo, Config } from "../lib/invoke";

export default function Channels() {
  const [channels, setChannels] = useState<ChannelInfo[]>([]);
  const [config, setConfig] = useState<Config | null>(null);

  useEffect(() => {
    api.getChannelStatus().then(setChannels).catch(console.error);
    api.getConfig().then(setConfig).catch(console.error);
  }, []);

  if (!config) return <div className="p-6">Loading...</div>;

  const channelConfigs: Record<string, { enabled: boolean; fields: { label: string; value: string }[] }> = {
    telegram: {
      enabled: config.channels.telegram.enabled,
      fields: [
        { label: "Token", value: config.channels.telegram.token ? "***configured***" : "Not set" },
        { label: "Allow From", value: config.channels.telegram.allowFrom.join(", ") || "All" },
      ],
    },
    feishu: {
      enabled: config.channels.feishu.enabled,
      fields: [
        { label: "App ID", value: config.channels.feishu.appId || "Not set" },
        { label: "Allow From", value: config.channels.feishu.allowFrom.join(", ") || "All" },
      ],
    },
    whatsapp: {
      enabled: config.channels.whatsapp.enabled,
      fields: [
        { label: "Bridge Port", value: String(config.channels.whatsapp.bridgePort) },
        { label: "Auto Start Bridge", value: config.channels.whatsapp.autoStartBridge ? "Yes" : "No" },
      ],
    },
  };

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Channels</h1>
      <div className="space-y-4">
        {Object.entries(channelConfigs).map(([name, ch]) => {
          const live = channels.find((c) => c.name === name);
          return (
            <div
              key={name}
              className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4"
            >
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2.5 w-2.5 rounded-full ${
                      live?.enabled ? "bg-green-400" : "bg-gray-300 dark:bg-gray-600"
                    }`}
                  />
                  <span className="text-base font-semibold capitalize">{name}</span>
                </div>
                <span
                  className={`text-xs font-medium px-2 py-0.5 rounded-full ${
                    ch.enabled
                      ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                      : "bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400"
                  }`}
                >
                  {ch.enabled ? "Enabled" : "Disabled"}
                </span>
              </div>
              <div className="space-y-1">
                {ch.fields.map((f) => (
                  <div key={f.label} className="flex text-sm">
                    <span className="w-36 text-gray-500 dark:text-gray-400">{f.label}</span>
                    <span className="text-gray-700 dark:text-gray-300">{f.value}</span>
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
      <p className="text-sm text-gray-400">
        To enable/disable channels, edit settings in the Config page and restart the app.
      </p>
    </div>
  );
}
```

**Step 2: Update `App.tsx`**

```tsx
import Channels from "./views/Channels";

// In Routes:
<Route path="/channels" element={<Channels />} />
```

**Step 3: Verify and commit**

```bash
git add crates/rustoctopus-app/ui/src/
git commit -m "feat: Channels view showing status and config per channel"
```

---

## Task 10: Frontend — Cron View

**Files:**
- Create: `crates/rustoctopus-app/ui/src/views/Cron.tsx`
- Modify: `crates/rustoctopus-app/ui/src/App.tsx`

**Step 1: Create `src/views/Cron.tsx`**

```tsx
import { useEffect, useState } from "react";
import { api, CronJob } from "../lib/invoke";

function formatSchedule(job: CronJob): string {
  const s = job.schedule;
  if (s.kind === "every" && s.everyMs) {
    const sec = s.everyMs / 1000;
    if (sec < 60) return `Every ${sec}s`;
    if (sec < 3600) return `Every ${Math.round(sec / 60)}m`;
    return `Every ${Math.round(sec / 3600)}h`;
  }
  if (s.kind === "cron" && s.expr) return s.expr;
  if (s.kind === "at" && s.atMs) return new Date(s.atMs).toLocaleString();
  return "unknown";
}

function formatTime(ms: number | null): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
}

export default function Cron() {
  const [jobs, setJobs] = useState<CronJob[]>([]);
  const [showAdd, setShowAdd] = useState(false);
  const [name, setName] = useState("");
  const [message, setMessage] = useState("");
  const [scheduleKind, setScheduleKind] = useState("every");
  const [everyMin, setEveryMin] = useState("30");
  const [cronExpr, setCronExpr] = useState("0 0 9 * * *");

  const load = () => api.listCronJobs().then(setJobs).catch(console.error);

  useEffect(() => { load(); }, []);

  const handleAdd = async () => {
    try {
      await api.addCronJob({
        name,
        message,
        scheduleKind,
        everyMs: scheduleKind === "every" ? Number(everyMin) * 60 * 1000 : undefined,
        cronExpr: scheduleKind === "cron" ? cronExpr : undefined,
      });
      setShowAdd(false);
      setName("");
      setMessage("");
      load();
    } catch (e) {
      alert("Failed to add job: " + e);
    }
  };

  const handleRemove = async (id: string) => {
    if (!confirm("Remove this job?")) return;
    await api.removeCronJob(id);
    load();
  };

  const handleToggle = async (id: string) => {
    await api.toggleCronJob(id);
    load();
  };

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Cron Jobs</h1>
        <button
          onClick={() => setShowAdd(!showAdd)}
          className="px-3 py-1.5 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
        >
          {showAdd ? "Cancel" : "Add Job"}
        </button>
      </div>

      {showAdd && (
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 space-y-3">
          <input className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" placeholder="Job name" value={name} onChange={(e) => setName(e.target.value)} />
          <textarea className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" placeholder="Message" rows={2} value={message} onChange={(e) => setMessage(e.target.value)} />
          <div className="flex gap-3 items-center">
            <select className="px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" value={scheduleKind} onChange={(e) => setScheduleKind(e.target.value)}>
              <option value="every">Every N minutes</option>
              <option value="cron">Cron expression</option>
            </select>
            {scheduleKind === "every" ? (
              <input className="w-24 px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" type="number" value={everyMin} onChange={(e) => setEveryMin(e.target.value)} placeholder="min" />
            ) : (
              <input className="flex-1 px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" value={cronExpr} onChange={(e) => setCronExpr(e.target.value)} placeholder="0 0 9 * * *" />
            )}
          </div>
          <button onClick={handleAdd} className="px-4 py-2 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors">
            Add
          </button>
        </div>
      )}

      {jobs.length === 0 ? (
        <div className="text-gray-400 text-sm">No cron jobs configured.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm text-left">
            <thead className="text-xs text-gray-500 dark:text-gray-400 uppercase border-b border-gray-200 dark:border-gray-700">
              <tr>
                <th className="py-2 pr-4">Name</th>
                <th className="py-2 pr-4">Schedule</th>
                <th className="py-2 pr-4">Last Run</th>
                <th className="py-2 pr-4">Next Run</th>
                <th className="py-2 pr-4">Status</th>
                <th className="py-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              {jobs.map((job) => (
                <tr key={job.id} className="border-b border-gray-100 dark:border-gray-800">
                  <td className="py-2 pr-4 font-medium">{job.name}</td>
                  <td className="py-2 pr-4 font-mono text-xs">{formatSchedule(job)}</td>
                  <td className="py-2 pr-4 text-gray-500">{formatTime(job.state.lastRunAtMs)}</td>
                  <td className="py-2 pr-4 text-gray-500">{formatTime(job.state.nextRunAtMs)}</td>
                  <td className="py-2 pr-4">
                    <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${job.enabled ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400" : "bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400"}`}>
                      {job.enabled ? "Active" : "Disabled"}
                    </span>
                  </td>
                  <td className="py-2 space-x-2">
                    <button onClick={() => handleToggle(job.id)} className="text-xs text-indigo-600 hover:text-indigo-800 dark:text-indigo-400">
                      {job.enabled ? "Disable" : "Enable"}
                    </button>
                    <button onClick={() => handleRemove(job.id)} className="text-xs text-red-500 hover:text-red-700">
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
```

**Step 2: Update `App.tsx`**

```tsx
import Cron from "./views/Cron";

// In Routes:
<Route path="/cron" element={<Cron />} />
```

**Step 3: Verify and commit**

```bash
git add crates/rustoctopus-app/ui/src/
git commit -m "feat: Cron view with job list, add/remove/toggle"
```

---

## Task 11: Integration Test + Final Polish

**Step 1: Full end-to-end test**

Run: `cd crates/rustoctopus-app && cargo tauri dev`

Test checklist:
- [ ] App opens with sidebar + Dashboard
- [ ] Dashboard shows model, uptime, channels, cron
- [ ] Config loads all fields, save writes to disk
- [ ] Channels shows status for telegram/feishu/whatsapp
- [ ] Cron: add job, toggle, remove all work
- [ ] Close window → hides to tray
- [ ] Click tray icon → window reappears
- [ ] Tray "Quit" → app exits

**Step 2: Add `.gitignore` for frontend build artifacts**

Create `crates/rustoctopus-app/ui/.gitignore`:

```
node_modules/
dist/
```

**Step 3: Final commit**

```bash
git add crates/rustoctopus-app/
git commit -m "feat: complete Phase 3 Tauri GUI with dashboard, config, channels, cron"
```

---

## Summary

| Task | Description | Key Deliverable |
|------|-------------|-----------------|
| 1 | Scaffold Tauri project | Cargo.toml, tauri.conf.json, icons |
| 2 | Scaffold React frontend | Vite + React + Tailwind + sidebar |
| 3 | System tray + close-to-tray | Window management |
| 4 | AppState + gateway startup | Embedded gateway in Tauri process |
| 5 | IPC: Config | get_config, save_config commands |
| 6 | IPC: Dashboard + Channels + Cron | Remaining backend commands |
| 7 | Frontend: Dashboard | Status cards, channel list |
| 8 | Frontend: Config | Form with save |
| 9 | Frontend: Channels | Channel status cards |
| 10 | Frontend: Cron | Job table + add/remove/toggle |
| 11 | Integration test + polish | E2E verification |
