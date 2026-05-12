# PLAN.md

## Goal

Build `bilive-chat`: a from-scratch Rust Bilibili Live chat overlay for OBS Browser Source.

This is a local-first tool, not a cloud service and not a data pipeline. It should capture Bilibili live chat events, normalize them into chat-domain events, and render them in a clean OBS browser overlay.

The first milestone is a usable local overlay for one Bilibili live room using the `web_live` source.

## Core Direction

Use:

```text
Rust + Tokio
axum local server
hyper for HTTP
WebSocket live transport
vanilla HTML/CSS/JS frontend
embedded web assets
local data/ directory
````

Do not use:

```text
reqwest
frontend framework
database
remote auth
plugin system
multi-room support
Coyote concepts
```

Source decisions:

```text
First implemented source: web_live
Known future source: open_platform
Current milestone: web_live only
```

`web_live` means the reverse-engineered ordinary Bilibili live-room web/client path: room resolution, WBI-signed requests, danmu info retrieval, live WebSocket authentication, packet decoding, compression handling, and raw command extraction.

Do not call this source `broadcast` in the new project. `broadcast` is only the old project’s name for this path.

## Phase Scope Rule

Only the active phase is implementation scope.

Architecture notes, future components, and later phase descriptions are context only. Do not implement them early.

If a later concept seems useful for the current phase, use the smallest local placeholder and report the real work as a known limitation or next step.

Do not add abstractions only for future Open Platform support. Keep source-specific protocol code separate from normalized chat events so Open Platform can be added later, but do not build a generic source framework before it is needed.

## Local Reference Policy

A previous project may exist at:

```text
.temp/bilive-coyote
```

Use it only as a read-only protocol reference when a phase explicitly needs Bilibili protocol behavior.

It contains two Bilibili source implementations:

```text
broadcast        ordinary live-room client path; rename this concept to web_live here
open_platform    Bilibili Open Platform path
```

For the first milestone, implement only `web_live`.

When a phase references the old `broadcast` implementation, use it only for protocol behavior:

```text
room id resolution
WBI behavior
danmu auth request
live WebSocket authentication
heartbeat behavior
packet framing
compression handling
JSON extraction
known message shapes
```

Do not copy the old architecture, module structure, manager structure, config shape, event names, panel model, Coyote logic, gift-rule logic, or assumptions.

## Target Layout

The project should converge on this layout:

```text
src/
  main.rs
  app.rs

  bilibili/
    mod.rs
    web_live/
      mod.rs
      auth.rs
      http.rs
      socket.rs
      parser.rs

  chat/
    mod.rs
    event.rs
    filter.rs

  overlay/
    mod.rs
    server.rs
    ws.rs

  config/
    mod.rs
    types.rs

web/
  overlay.html
  overlay.css
  overlay.js
  panel.html
  panel.css
  panel.js

data/
  .gitkeep
```

`data/` is runtime state. Track `data/.gitkeep`, but ignore generated runtime files such as:

```text
data/config.json
data/login-state.json
```

Do not create `bilibili/open_platform/` during the first milestone unless a later phase explicitly adds Open Platform.

## Long-Term Boundaries

These are architecture notes, not implementation permission for the current phase.

```text
web_live protocol code
  -> source-specific raw commands
  -> chat-domain normalization
  -> ChatEvent
  -> filter
  -> OverlayEvent
  -> browser clients
```

Future Open Platform should later enter at the same normalization boundary:

```text
web_live raw command       ┐
                           ├── ChatEvent -> filter -> OverlayEvent -> browser
open_platform raw event    ┘
```

Do not let raw source-specific data leak into overlay rendering. Do not let overlay rendering shape source-specific protocol code.

## Runtime Behavior

Default behavior:

```text
Start local server only.
Do not auto-connect to Bilibili on startup.
User starts/stops connection from the panel.
Panel generates the OBS overlay URL.
Overlay is served at /overlay.
Panel is served at /.
```

Persistent files:

```text
data/config.json
data/login-state.json
```

`login-state.json` stores an extracted cookie string locally.

Do not add encryption, keychain integration, user accounts, or remote access controls.

Do not print the cookie in logs.

## Routes

Target local routes:

```text
GET  /
GET  /overlay
GET  /ws/overlay
GET  /ws/panel

GET  /api/config
POST /api/config

POST /api/bilibili/start
POST /api/bilibili/stop

POST   /api/bilibili/login-state
DELETE /api/bilibili/login-state
```

Do not add extra routes unless a phase explicitly requires them.

## Acceptance Policy

The Agent must distinguish automatic validation from user-gated validation.

### Automatic Acceptance

Run and report available checks whenever possible:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo check
local server startup
HTTP route smoke checks
WebSocket smoke checks
unit tests for parser/config/filter behavior when relevant
```

Use mocks, fixtures, and synthetic events where appropriate.

### User-Gated Acceptance

Do not claim success for checks that require:

```text
real Bilibili API access
real Bilibili room connection
real login cookie
OBS Browser Source behavior
manual browser UI judgement
visual quality judgement
long-running reconnect behavior
```

The Agent may provide commands and instructions for these checks, but must not mark them passed unless the user confirms.

## Phase Report Format

Every phase must end with:

```text
Implemented:
- Concrete changes with important files/modules.

Auto-verified:
- Commands/checks run and whether they passed.

Needs user validation:
- Anything requiring OBS, real Bilibili, browser UI, or manual testing.

Known limitations:
- Intentional gaps, risks, or unfinished edges.

Next step:
- Whether this phase appears acceptable or what needs review.
```

## Implementation Phases

### Phase 1: Skeleton

Goal:

Create only the initial Rust project skeleton and a local server that can serve placeholder embedded web pages.

Allowed changes:

```text
Create Cargo project files.
Create declared module directories and placeholder files from Target Layout.
Add minimal dependencies required for server startup, logging, and embedded assets.
Serve / and /overlay as placeholder embedded pages.
Track data/.gitkeep.
Ignore generated runtime files under data/.
```

Do not implement:

```text
Config structs.
Config load/save.
Login-state load/save/delete.
ChatEvent or domain event structs.
Filtering logic.
WebSocket endpoints.
API endpoints.
Bilibili protocol behavior.
Open Platform behavior.
Source traits, source registry, or generic source framework.
Future-phase model types just to satisfy later phases.
```

Expected files:

```text
Cargo.toml
src/main.rs
src/bilibili/mod.rs
src/bilibili/web_live/mod.rs
empty placeholder files under src/bilibili/web_live/
src/chat/mod.rs
empty placeholder files under src/chat/
src/overlay/mod.rs
src/overlay/server.rs
empty src/overlay/ws.rs
src/config/mod.rs
empty src/config/types.rs
web/panel.html
web/overlay.html
placeholder CSS/JS files
data/.gitkeep
.gitignore
```

Automatic acceptance:

```text
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
cargo check passes
binary starts local server
GET / returns placeholder panel HTML
GET /overlay returns placeholder overlay HTML
no files except .gitkeep are generated under data/
no Bilibili connection is attempted on startup
```

User-gated acceptance:

```text
none
```

### Phase 2: Local Server and Web Shell

Goal:

Add only the local browser shell and WebSocket plumbing needed for panel and overlay to connect.

Allowed changes:

```text
Add /ws/panel.
Add /ws/overlay.
Add minimal in-memory state needed to send synthetic messages.
Add browser-side WebSocket connection code.
Add a synthetic status message for panel.
Add a synthetic display message for overlay.
Add route/WebSocket smoke tests where practical.
```

Do not implement:

```text
Config persistence.
Login-state persistence.
Real ChatEvent domain model.
Filtering.
Bilibili HTTP flow.
Bilibili WebSocket flow.
Open Platform.
Start/stop API.
Final overlay styling.
Query parameter configuration.
Generated OBS URL.
Any real protocol parsing.
```

Expected behavior:

```text
GET / serves panel.
GET /overlay serves overlay.
Panel connects to /ws/panel.
Overlay connects to /ws/overlay.
Panel can display a synthetic status from the server.
Overlay can display a synthetic chat-like item from the server.
Synthetic message types are local to Phase 2 and not treated as final domain models.
```

Automatic acceptance:

```text
server starts locally
GET / returns 200
GET /overlay returns 200
/ws/panel accepts a WebSocket client
/ws/overlay accepts a WebSocket client
mock status reaches panel
mock display item reaches overlay
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
user opens panel in browser
user opens overlay in browser
basic placeholder visual direction is acceptable
```

### Phase 3: Config and Login State

Goal:

Add local persistence for user config and login cookie.

Allowed changes:

```text
Define config types.
Define login-state type.
Create defaults when data/config.json is missing.
Create empty/default login state when data/login-state.json is missing.
Load/save data/config.json.
Load/save/delete data/login-state.json.
Add GET /api/config.
Add POST /api/config.
Add POST /api/bilibili/login-state.
Add DELETE /api/bilibili/login-state.
Add panel UI for editing config.
Add panel UI for saving/deleting login cookie.
```

Do not implement:

```text
Bilibili connection start/stop.
Bilibili HTTP flow.
Bilibili WebSocket flow.
Chat parsing.
Filtering behavior beyond config shape.
Overlay final rendering.
Open Platform.
Generic source framework.
```

Config should include only:

```text
server host/port
room id
overlay options needed by current plan
filter options needed by current plan
```

Login state should include only:

```text
cookie
updated timestamp
```

Automatic acceptance:

```text
missing config creates defaults
missing login state creates default empty state
config load/save round-trip tests pass
login-state save/load/delete tests pass
cookie value is not printed in logs
server still does not auto-connect on startup
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
user edits config through panel
user saves/deletes login cookie through panel
data/config.json appears as expected
data/login-state.json appears as expected
```

### Phase 4: Overlay Experience

Goal:

Implement the first usable YouTube-like overlay using synthetic events only.

Allowed changes:

```text
Define display event shape needed by overlay.
Render synthetic normal message.
Render synthetic gift.
Render synthetic super chat.
Render synthetic guard.
Enforce max item count in overlay.
Enforce message lifetime in overlay.
Support avatar visibility option.
Support basic font/style options.
Support query parameters for overlay display options.
Generate/copy OBS overlay URL from panel.
```

Do not implement:

```text
Bilibili HTTP flow.
Bilibili WebSocket flow.
Real chat parsing.
Filtering behavior.
Open Platform.
Final theme system.
Plugin system.
Multiple overlay profiles.
```

Expected behavior:

```text
Overlay looks like a clean YouTube-like live chat, adapted to Bilibili semantics.
Panel can generate an overlay URL.
Overlay query parameters override relevant display options.
Overlay remains transparent/OBS-friendly by default.
```

Automatic acceptance:

```text
server can emit synthetic message/gift/super_chat/guard events
overlay receives and renders synthetic event set
max item count behavior works
message lifetime behavior works
query parameter parsing works where practical
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
user verifies overlay visual direction in browser
user verifies generated OBS URL
user optionally tests URL in OBS Browser Source
```

### Phase 5: web_live HTTP Flow

Goal:

Implement the `web_live` HTTP preparation flow using `hyper`.

Allowed changes:

```text
Read .temp/bilive-coyote only as protocol reference.
Add hyper-based HTTP client code for web_live.
Resolve usable room id.
Perform WBI signing where required.
Fetch danmu auth information.
Include persisted cookie when present.
Prepare live WebSocket endpoint/auth material.
Report room/auth/HTTP errors to panel state.
Add tests using mocks or fixtures where practical.
```

Do not implement:

```text
Live WebSocket connection.
Packet decoding.
Chat parsing.
Overlay rendering changes except status display.
Open Platform.
Generic source framework.
reqwest.
```

Expected behavior:

```text
Panel can trigger preparation flow only through the planned start path or temporary test command if needed.
HTTP/auth failures are visible in panel status.
Cookie is used when present but never printed.
```

Automatic acceptance:

```text
HTTP client compiles with hyper
request-building tests pass where practical
WBI signing tests pass where practical
mocked room/auth success flow passes
mocked HTTP failure classification passes
mocked invalid room behavior passes
cookie inclusion behavior is tested without printing cookie
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
real room id resolves through Bilibili web live path
real danmu auth returns endpoint/auth material
cookie behavior works with real login state if needed
```

If real Bilibili access is not run by the user, report:

```text
implementation complete; live validation pending
```

### Phase 6: web_live Socket

Goal:

Implement the `web_live` live WebSocket connection and lifecycle.

Allowed changes:

```text
Connect to live WebSocket endpoint from Phase 5.
Send auth packet.
Maintain heartbeat.
Decode incoming packets.
Decompress compressed payloads.
Extract raw JSON commands.
Expose connection status to panel.
Allow panel start.
Allow panel stop.
Ensure repeated start/stop does not leave stale tasks.
```

Do not implement:

```text
Final chat-domain parsing.
Filtering.
Open Platform.
Generic source framework.
Overlay visual redesign.
Extra reconnect strategy beyond what is needed for clean manual start/stop.
```

Expected behavior:

```text
Panel starts connection.
Panel stops connection.
Panel shows disconnected/connecting/connected/error states.
Raw commands can be observed through logs or internal diagnostics without leaking cookie.
Unknown packets do not crash the app.
```

Automatic acceptance:

```text
packet decode tests pass
compression fixture tests pass
heartbeat/auth packet tests pass where practical
mock WebSocket tests pass where practical
start/stop lifecycle tests pass where practical
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
real room connection succeeds
real heartbeat works
real packets arrive
manual stop disconnects cleanly
manual repeated start/stop behaves correctly
```

### Phase 7: Chat Parsing

Goal:

Normalize supported Bilibili `web_live` commands into chat-domain events.

First command set:

```text
DANMU_MSG
SEND_GIFT
SUPER_CHAT_MESSAGE
GUARD_BUY
```

Allowed changes:

```text
Define final chat-domain event types.
Parse supported web_live commands into chat-domain events.
Skip unknown commands.
Classify or skip malformed target commands without panic.
Send normalized chat events to overlay.
Preserve enough data for overlay rendering.
Add parser fixtures/tests.
```

Do not implement:

```text
Open Platform parsing.
Generic source framework.
Advanced event types beyond the first command set.
Filtering, except wiring to existing no-op if needed.
Theme system.
```

Expected behavior:

```text
Normal messages render.
Gifts render.
Super chats render.
Guards render.
Raw Bilibili JSON does not leak into overlay API.
```

Automatic acceptance:

```text
DANMU_MSG fixture parses
SEND_GIFT fixture parses
SUPER_CHAT_MESSAGE fixture parses
GUARD_BUY fixture parses
unknown command fixture is skipped
malformed JSON does not panic
malformed target command is classified or skipped intentionally
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
real normal messages render
real gifts render
real super chats render if observed
real guards render if observed
```

If real SC/guard events are unavailable, report live validation pending.

### Phase 8: Filtering

Goal:

Add basic local filtering after normalization and before overlay broadcast.

Allowed changes:

```text
Implement blocked users.
Implement blocked keywords.
Persist filter config.
Add panel UI for editing filters.
Apply filters after ChatEvent normalization.
Prevent filtered events from reaching overlay clients.
Add filter tests.
```

Do not implement:

```text
Advanced moderation.
Regex filters.
Remote blocklists.
Per-overlay filter profiles.
Open Platform.
Protocol changes.
```

Expected behavior:

```text
Blocked users do not appear in overlay.
Messages containing blocked keywords do not appear in overlay.
Filter config survives restart.
```

Automatic acceptance:

```text
blocked user tests pass
blocked keyword tests pass
filter config round-trip tests pass
overlay broadcast excludes filtered events in tests
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
```

User-gated acceptance:

```text
user verifies filters through panel
user verifies filtered messages do not appear in overlay
```

### Phase 9: Polish and Release Readiness

Goal:

Prepare the project for normal local use.

Allowed changes:

```text
Write README.
Document usage.
Document panel URL.
Document OBS overlay URL.
Document config behavior.
Document login-state behavior.
Document supported event types.
Improve logs if needed.
Ensure release build embeds web assets.
Optionally add simple release notes or build instructions.
```

Do not implement:

```text
New product features.
New event types.
Open Platform.
Plugin system.
Theme marketplace.
Remote access.
Database.
```

Expected behavior:

```text
User can download/build one binary, run it locally, open panel, configure room/cookie, add overlay to OBS, and use web_live source.
```

Automatic acceptance:

```text
cargo fmt --check passes
cargo clippy --all-targets -- -D warnings passes
cargo test passes
cargo build --release passes
release binary starts local server
embedded web assets load in release build
```

User-gated acceptance:

```text
user runs release binary
user opens panel
user adds overlay to OBS
user connects to real room
user confirms basic visual and operational experience
```

## Operational Requirements

The local tool should provide:

```text
clear connection status
last error in panel
manual start/stop
bounded overlay item count
bounded message lifetime
no cookie in logs
clean shutdown where practical
no stale connection task after stop
useful protocol/parser diagnostics
```

Do not add heavy operational machinery unless the project grows beyond local overlay use.

## Design Rules

Prefer ownership over shared mutable state.

Prefer typed chat events over dynamic JSON inside the system.

Prefer explicit lifecycle states over scattered booleans.

Prefer local clarity over generic framework abstractions.

Prefer source-specific protocol boundaries over premature source frameworks.

Prefer small, reviewable changes.

Prefer measured improvements over speculative optimization.

Use domain names based on chat, overlay, live connection, `web_live`, and Bilibili protocol behavior.

## Development Workflow

Work phase by phase. Do not implement future phases early.

For each phase:

```text
1. Prompt the Agent to implement only the requested phase.
2. Agent implements the phase.
3. Agent outputs a concise Phase Report.
4. Push the resulting code to a branch or commit.
5. Review the code together with ChatGPT using the Phase Report and diff.
6. Produce a Review Decision.
7. Feed the Review Decision back to the Agent.
```

Review Decision:

```text
ACCEPT
PARTIAL ACCEPT
REJECT
```

`ACCEPT`: proceed to the next phase.

`PARTIAL ACCEPT`: only listed fixes are required; do not start the next phase.

`REJECT`: direction is wrong; rollback or rewrite as instructed.

## Refactoring Rule

Refactoring is allowed when it directly supports the current phase and improves ownership, boundaries, state space, or domain clarity.

Do not perform broad cleanup unrelated to the current phase. Larger ideas should be reported as follow-up candidates, not implemented early.

## First Real Milestone

The first real milestone is:

```text
local server starts
panel opens at /
overlay opens at /overlay
config persists in data/config.json
login cookie persists in data/login-state.json
user can start/stop web_live connection from panel
normal messages, gifts, super chats, and guards are supported
overlay renders YouTube-like chat
panel shows connection status and last error
OBS can use the generated overlay URL
release binary embeds web assets
```

This milestone is user-gated where real Bilibili and OBS behavior are involved.

The Agent may implement tooling and instructions needed to validate it, but must not mark real Bilibili or OBS validation complete until the user confirms.

## Final Guiding Sentence

Keep the first source `web_live`, the future source boundary honest, the runtime local-first, the event model typed, the overlay clean, the panel useful, and every piece of complexity justified by correctness, clarity, OBS usage, protocol reality, or a known source boundary.
