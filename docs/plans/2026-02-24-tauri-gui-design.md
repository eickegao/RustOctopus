# RustOctopus Phase 3: Tauri GUI - Design Document

**Date:** 2026-02-24
**Status:** Approved

## Goal

Build a cross-platform (macOS / Windows / Linux) desktop control console for RustOctopus using Tauri 2 + React + TypeScript. The app embeds the gateway (AgentLoop + ChannelManager + CronService) and runs as a system tray application.

## Architecture

```
┌─ Tauri Process ──────────────────────────────────┐
│                                                   │
│  ┌─ React UI (WebView) ──────────────────────┐   │
│  │  Dashboard │ Config │ Channels │ Cron     │   │
│  └─────────────┬─────────────────────────────┘   │
│                │ Tauri IPC (invoke / listen)      │
│  ┌─────────────┴─────────────────────────────┐   │
│  │  Rust Backend (src-tauri)                  │   │
│  │  ├─ AppState (Arc<Mutex<...>>)             │   │
│  │  │   ├─ Config                             │   │
│  │  │   ├─ AgentLoop                          │   │
│  │  │   ├─ ChannelManager                     │   │
│  │  │   ├─ CronService                        │   │
│  │  │   └─ MessageBus                         │   │
│  │  ├─ IPC Commands                           │   │
│  │  └─ System Tray                            │   │
│  └────────────────────────────────────────────┘   │
│                                                   │
│  Window close → hide to tray (service continues)  │
│  Tray "Quit"  → stop service and exit process     │
└───────────────────────────────────────────────────┘
```

## System Tray Behavior

- App launches with main window + tray icon (octopus icon)
- Clicking window close button → hides window, tray icon stays, service keeps running
- Clicking tray icon → show/restore window
- Tray right-click menu: "Show Window", "Quit"
- "Quit" stops all channels/cron, then exits process

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri 2 |
| Frontend | React 18 + TypeScript |
| Styling | Tailwind CSS |
| Build tool | Vite |
| Icons | Tauri icon generator from `assets/icon.png` |
| Core logic | `rustoctopus-core` (workspace dependency) |

## IPC Commands

### Config

| Command | Direction | Description |
|---------|-----------|-------------|
| `get_config` | frontend → backend | Return full Config as JSON |
| `save_config` | frontend → backend | Write updated Config to disk, reload |

### Dashboard

| Command | Direction | Description |
|---------|-----------|-------------|
| `get_status` | frontend → backend | Return runtime status: uptime, model, channel states, cron summary |

### Channels

| Command | Direction | Description |
|---------|-----------|-------------|
| `get_channel_status` | frontend → backend | Return list of channels with name, enabled, connected |
| `toggle_channel` | frontend → backend | Enable/disable a channel, restart if needed |

### Cron

| Command | Direction | Description |
|---------|-----------|-------------|
| `list_cron_jobs` | frontend → backend | Return all cron jobs |
| `add_cron_job` | frontend → backend | Add a new cron job (schedule + message) |
| `remove_cron_job` | frontend → backend | Remove a cron job by ID |
| `toggle_cron_job` | frontend → backend | Enable/disable a cron job by ID |

### Events (backend → frontend)

| Event | Description |
|-------|-------------|
| `channel-status-changed` | Emitted when a channel connects/disconnects |
| `cron-job-fired` | Emitted when a cron job executes |

## UI Layout

Fixed left sidebar navigation + right content area.

```
┌──────┬──────────────────────────────────┐
│ Logo │                                  │
│      │  Content Area                    │
│ ──── │                                  │
│ Dash │                                  │
│ Conf │                                  │
│ Chan │         (view content)           │
│ Cron │                                  │
│      │                                  │
│ ──── │                                  │
│ Quit │                                  │
└──────┴──────────────────────────────────┘
```

### Dashboard View

- Top: status cards (current model, uptime, total messages)
- Middle: channel status list (name, on/off, connected/disconnected)
- Bottom: cron summary (active jobs count, next execution time)

### Config View

- Provider section: model selector dropdown, API key input (masked)
- Agent section: temperature slider, maxTokens input, memoryWindow input
- Tools section: exec timeout, web search API key
- Save button at bottom, writes to `~/.rustoctopus/config.json`

### Channels View

- One card per channel (Telegram / Feishu / WhatsApp)
- Each card shows: enable toggle, connection status indicator, config fields
- Telegram: token input, allowFrom list
- Feishu: appId, appSecret inputs
- WhatsApp: bridge port, auto-start toggle, connection status

### Cron View

- Table: job name/message, schedule, last run, next run, enabled toggle
- "Add Job" button opens dialog: schedule input + message textarea
- Delete button per row with confirmation

## Project Structure

```
crates/rustoctopus-app/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
├── icons/                    # Generated from assets/icon.png
├── src/                      # Rust backend
│   ├── main.rs               # Tauri setup, tray, window management
│   ├── state.rs              # AppState definition
│   └── commands/
│       ├── mod.rs
│       ├── config.rs         # get_config, save_config
│       ├── dashboard.rs      # get_status
│       ├── channels.rs       # get_channel_status, toggle_channel
│       └── cron.rs           # list/add/remove/toggle cron jobs
├── ui/                       # React frontend
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── tailwind.config.js
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx           # Router + sidebar layout
│       ├── components/
│       │   ├── Sidebar.tsx
│       │   ├── StatusCard.tsx
│       │   └── Toggle.tsx
│       └── views/
│           ├── Dashboard.tsx
│           ├── Config.tsx
│           ├── Channels.tsx
│           └── Cron.tsx
```

## Style

- Clean, modern look (similar to Vercel Dashboard)
- Tailwind CSS for utility-first styling
- Consistent across all three platforms
- Color scheme: indigo/blue tones matching the octopus icon
- Dark mode support (follow system preference)

## Cross-Platform Packaging

| Platform | Format | Notes |
|----------|--------|-------|
| macOS | `.dmg` | Universal binary (x86_64 + aarch64) |
| Windows | `.msi` | Requires WebView2 (bundled or auto-install) |
| Linux | `.AppImage` + `.deb` | Requires WebKitGTK |

Tauri's built-in bundler handles all three via `tauri build`.
