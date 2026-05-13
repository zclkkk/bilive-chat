# bilive-chat

A local Bilibili Live chat overlay for OBS Browser Source, built in Rust.

## Quick Start

```bash
cargo build --release
./target/release/bilive-chat
```

Open http://127.0.0.1:7792 in your browser to access the panel.

## Usage

1. Open the panel at `http://<host>:<port>/`
2. Set your room ID in the Config card
3. (Optional) Paste your Bilibili cookie in the Login State card for logged-in features
4. Click **Start** to connect to the live room
5. Copy the OBS overlay URL from the panel
6. In OBS, add a **Browser Source** and paste the overlay URL

## URLs

| Path | Description |
|------|-------------|
| `/` | Control panel |
| `/overlay` | OBS browser overlay |

The overlay URL includes query parameters for display options:

```
http://<host>:<port>/overlay?max_items=50&lifetime=300&show_avatar=true&font_size=14
```

| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `max_items` | 50 | 1–200 | Maximum items shown |
| `lifetime` | 300 | 1–3600 | Seconds before items fade out |
| `show_avatar` | true | `true`/`false` or `1`/`0` | Show avatar color dot |
| `font_size` | 14 | 8–48 | Font size in pixels |

## Configuration

Configuration is stored in `data/config.json` and editable from the panel.

| Field | Default | Description |
|-------|---------|-------------|
| `host` | `127.0.0.1` | Server bind address |
| `port` | 7792 | Server port |
| `room_id` | 0 | Bilibili live room ID |
| `overlay.max_items` | 50 | Max overlay items |
| `overlay.message_lifetime_secs` | 300 | Item lifetime in seconds |
| `overlay.show_avatar` | true | Show avatar color |
| `filter.blocked_users` | [] | Blocked usernames (exact match) |
| `filter.blocked_keywords` | [] | Blocked keywords (substring match, case-sensitive) |

## Login State

Login state is stored in `data/login-state.json`.

- Paste your Bilibili cookie string in the panel's Login State card
- The cookie is used for WBI-signed API requests and live WebSocket authentication
- A valid cookie is recommended; guest mode may work when Bilibili allows it
- The cookie is never printed in logs

## Supported Events

| Bilibili Command | Event Type | Description |
|-----------------|------------|-------------|
| `DANMU_MSG` | `normal` | Chat message |
| `SEND_GIFT` | `gift` | Gift sent |
| `SUPER_CHAT_MESSAGE` | `super_chat` | Super Chat |
| `GUARD_BUY` | `guard` | Guard purchase |

Unknown commands are silently skipped.

## Filtering

Blocked users and keywords can be configured from the panel's Filters card or via the API:

- `GET /api/filter` — read current filter config
- `POST /api/filter` — update filter config

Blocked users are matched by exact username. Blocked keywords are matched by substring in message text (case-sensitive). Filtering applies to Normal and SuperChat messages only; Gift and Guard events have no text content to match.

## API

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/config` | Read config |
| POST | `/api/config` | Update config |
| GET | `/api/filter` | Read filter config |
| POST | `/api/filter` | Update filter config |
| GET | `/api/overlay-url` | Get OBS overlay URL |
| POST | `/api/bilibili/start` | Start live connection |
| POST | `/api/bilibili/stop` | Stop live connection |
| GET | `/api/bilibili/status` | Get connection status |
| POST | `/api/bilibili/login-state` | Save cookie |
| DELETE | `/api/bilibili/login-state` | Delete cookie |

## Logging

Set `RUST_LOG` for log verbosity:

```bash
RUST_LOG=bilive_chat=info ./target/release/bilive-chat
RUST_LOG=bilive_chat=debug ./target/release/bilive-chat
```

## Build

```bash
cargo build --release
```

The release binary embeds all web assets (HTML/CSS/JS). No external files are needed at runtime beyond the `data/` directory for persistent state.
