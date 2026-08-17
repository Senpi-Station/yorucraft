# YoruCraft

A modern, open-source Minecraft launcher built with Rust + Tauri v2 + SvelteKit + TypeScript + SQLite.

Dark-first UI with amber (#f59e0b) accent, Inter font, and shadow-based depth.

## Tech Stack

- **Backend:** Rust + Tauri v2
- **Frontend:** SvelteKit + TypeScript
- **Database:** SQLite (rusqlite)
- **Design:** Dark theme, amber accent, no borders

## Features

### Phase 1: Core Engine (Complete)
- Version manifest fetching with recursive `inheritsFrom` resolution
- Content-addressable asset downloading with concurrent downloads
- Library resolution with Maven coordinate parsing and multi-mirror downloads
- Native library extraction from JARs
- JVM command builder and game process spawning
- Java auto-detection and Mojang JRE download
- Offline authentication with deterministic UUID

### Upcoming
- Smart auto-tuning and pre-flight checks
- Crash diagnosis and fix suggestions
- Fabric and Forge mod loader support
- Multi-instance manager
- Modern dark UI

## Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- [Node.js](https://nodejs.org/) (18+)
- [Tauri CLI](https://tauri.app/)

## Getting Started

```bash
# Install dependencies
npm install

# Run in development
npm run tauri dev

# Build for production
npm run tauri build
```

## Project Structure

```
yorucraft/
├── src/                    # SvelteKit frontend
│   ├── routes/             # Page routes
│   ├── lib/stores/         # State management
│   └── app.css             # Design tokens
├── src-tauri/              # Rust backend
│   └── src/
│       ├── auth/           # Authentication (offline, Microsoft)
│       ├── installer/      # Version manifest, assets, libraries, natives
│       ├── launcher/       # JVM launcher, Java detection
│       ├── modloaders/     # Fabric, Forge support
│       ├── smart/          # Auto-tuner, diagnostics, crash fixer
│       ├── instance/       # Multi-instance manager
│       ├── advanced/       # Skin preview, log viewer, backups
│       ├── db/             # SQLite database
│       └── utils/          # Crash analysis, CDN, integrity
└── static/                 # Static assets
```

## License

MIT
