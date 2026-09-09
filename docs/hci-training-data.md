# HCI Training Data Spec

## Purpose

Turn `kmmon` into a workstation telemetry collector for training and evaluating models that can operate a Sway desktop environment while performing software engineering work.

The system should record human and model desktop sessions as MCAP, index those recordings in Foxglove, curate task episodes with metadata, and export aligned observation/action/outcome examples for model training.

## Current Baseline

`kmmon` already provides the foundation for privacy-preserving interaction telemetry:

| Capability | Current behavior |
|------------|------------------|
| Live stream | Foxglove WebSocket on `ws://localhost:8765` |
| Recording | Rolling MCAP files |
| Upload | Optional S3-compatible upload for Foxglove bring-your-own-storage sites |
| Metadata | `foxglove` MCAP metadata with `projectId`, `deviceId`, and `deviceName` |
| Keyboard privacy | Raw key identity is discarded immediately |
| Mouse telemetry | Position, scroll, and activity |
| Keyboard telemetry | Keystroke rate, approximate WPM, and active state |

Existing topics:

| Topic | Use |
|-------|-----|
| `/mouse/position` | Pointer trajectory |
| `/mouse/scroll` | Scroll intent and navigation |
| `/mouse/activity` | Coarse pointer activity |
| `/keyboard/activity` | Typing intensity without typed content |

These signals are useful for activity modeling, session segmentation, and human cadence modeling. They are not sufficient by themselves to train an action-imitation model because they intentionally omit key identity, clicks, window state, screen contents, command semantics, code edits, and task outcomes.

## Data Platform Integration

The first implementation should use the Foxglove API surface that exists today, while keeping the data model compatible with Foxglove's planned Datasets product.

| Need | API or platform feature |
|------|-------------------------|
| Group rolled MCAP files into one work session | MCAP `foxglove` metadata key `sessionKey`, then `GET /v1/sessions` |
| Mark task or episode boundaries | `POST /v1/events`, `PATCH /v1/events/{id}`, `GET /v1/events` |
| Store task labels and filters | Event metadata and event custom properties |
| Find matching recordings | `GET /v1/recordings` with `projectId`, `deviceName`, `sessionId`, `metadataQuery`, and time filters |
| Validate available topics and schemas | `GET /v1/data/topics?includeSchemas=true` |
| Export current training slices | `POST /v1/data/stream` with `deviceId`, `sessionId`, `recordingId`, `start`, `end`, `topics`, and `outputFormat=mcap` |
| Monitor BYOS/indexing health | `GET /v1/data/pending-imports` and `GET /v1/data/import-errors` |
| Curate durable training sets later | Planned `/v1/datasets` API |

Foxglove Datasets should be treated as the durable curation layer once available. Until then, `Events` are the episode primitive, `Sessions` are the recording-group primitive, and `POST /v1/data/stream` is the export primitive.

## Goals

- Capture software-engineering interaction traces on Sway workstations.
- Preserve `kmmon`'s privacy-first default behavior.
- Add explicit opt-in modes for richer demonstration capture.
- Represent desktop work as synchronized multimodal MCAP streams.
- Use Foxglove as the data catalog, slicer, annotator, and export layer.
- Train models that can perform bounded engineering workflows in a sandboxed Sway session.
- Replay both human demonstrations and model rollouts in Foxglove for inspection.

## Non-Goals

- Do not make raw keylogging the default.
- Do not capture secrets, passwords, private messages, or arbitrary browser content without explicit opt-in.
- Do not let trained agents mutate production infrastructure.
- Do not train from unreviewed private code or customer data.
- Do not require a full desktop accessibility stack on Wayland.

## Capture Modes

### Activity Mode

Activity mode is the default and preserves the current privacy model.

Captured topics:

| Topic | Description |
|-------|-------------|
| `/keyboard/activity` | Keystrokes per minute, WPM estimate, active flag |
| `/mouse/activity` | Pixels per second, active flag |
| `/mouse/position` | Pointer coordinates |
| `/mouse/scroll` | Scroll deltas |

Activity mode supports task segmentation, workload estimation, activity density analysis, and alignment against external logs. It must never record key identity or typed text.

### Sway Context Mode

Sway context mode is opt-in and records compositor state, not typed content.

Proposed topics:

| Topic | Frequency | Description |
|-------|-----------|-------------|
| `/sway/tree` | On change or 1 Hz | Workspace, output, and container tree snapshot |
| `/sway/focus` | On change | Focused app, workspace, window rectangle |
| `/sway/window_event` | Event-driven | New, close, focus, move, resize, and title changes |
| `/sway/workspace_event` | Event-driven | Workspace switch and layout changes |

Minimum Sway fields:

| Field | Description |
|-------|-------------|
| `output` | Monitor name |
| `workspace` | Workspace name or number |
| `app_id` | Wayland app id |
| `window_class` | XWayland class when present |
| `title_hash` | Optional hashed title for correlation without raw title text |
| `rect` | Window geometry |
| `focused` | Focus state |
| `layout` | Sway layout mode |
| `floating` | Floating state |

Privacy defaults:

| Data | Default |
|------|---------|
| Window titles | Redacted or hashed |
| Screenshots | Disabled |
| Browser URLs | Redacted |
| Terminal text | Not captured |
| Editor buffer text | Not captured |

### Demonstration Mode

Demonstration mode is explicit opt-in and captures richer action data for training.

Proposed topics:

| Topic | Description |
|-------|-------------|
| `/human/action` | Normalized input/action stream |
| `/human/click` | Mouse button, coordinate, and focused window |
| `/human/shortcut` | Non-text shortcut, such as `ctrl+s` |
| `/terminal/command` | Shell command after prompt submission and redaction |
| `/terminal/result` | Exit code, duration, and summarized output |
| `/git/state` | Branch, dirty state, changed files, and HEAD |
| `/repo/diff` | Unified diff at checkpoints |
| `/editor/operation` | Open file, cursor move, insert, delete, save |
| `/dev/task` | Task id, goal, repo, issue URL, and success criteria |
| `/dev/outcome` | Human or automated success/failure label |

Input capture policy:

| Input class | Recommended capture |
|-------------|---------------------|
| Plain typing into editor | Prefer file diffs over raw key events |
| Password prompts | Never capture |
| Shell commands | Capture submitted command after redaction |
| Keyboard shortcuts | Capture normalized shortcut |
| Mouse clicks | Capture button and coordinates |
| Text edits | Capture resulting diff and optional editor operation |

### Screen Capture

Screenshots are valuable for GUI grounding, but they are high-risk and high-bitrate. They should be allowed only in explicit demonstration or rollout mode, and preferably only inside a dedicated sandbox Sway session.

Policy by mode:

| Mode | Screenshot policy |
|------|-------------------|
| Activity mode | Never |
| Sway context mode | Disabled by default |
| Personal workstation demonstration | Off by default; opt-in with redaction |
| Synthetic or sandbox demonstration | Allowed |
| Model rollout/evaluation | Allowed |

If screen frames are captured, use a separate high-bitrate topic such as `/screen/frame` and keep it in a separate MCAP stream or file set from low-bitrate telemetry. Foxglove index-in-place query performance is better when high-bitrate topics such as images are partitioned from lower-bitrate telemetry.

## MCAP Metadata

Every recording should include the existing `foxglove` metadata plus session and schema metadata.

Existing keys:

| Key | Source |
|-----|--------|
| `projectId` | `KMMON_FOXGLOVE_PROJECT_ID` |
| `deviceId` | `KMMON_FOXGLOVE_DEVICE_ID` |
| `deviceName` | `KMMON_FOXGLOVE_DEVICE_NAME` or hostname |

Required new keys:

| Key | Description |
|-----|-------------|
| `sessionKey` | User-supplied stable key that groups rolled MCAP files into one Foxglove session |
| `key` | Idempotency key for each recording file |

Recommended `kmmon` metadata keys:

| Key | Description |
|-----|-------------|
| `kmmon.schemaVersion` | HCI schema version |
| `kmmon.captureMode` | `activity`, `sway_context`, `demonstration`, or `rollout` |
| `kmmon.desktop` | `sway` |
| `kmmon.swayVersion` | Sway version |
| `kmmon.os` | Distro, NixOS generation, or image id |
| `kmmon.repo` | Optional repository identifier |
| `kmmon.taskSource` | Manual, Linear, GitHub, benchmark, synthetic, etc. |
| `kmmon.privacyProfile` | Redaction policy name |
| `kmmon.operatorId` | Optional pseudonymous demonstrator id |

## Event Model

Events are the V0 episode primitive. Each software-engineering task should become a Foxglove Event whose time range intersects the relevant session recordings.

Recommended event metadata:

| Key | Description |
|-----|-------------|
| `task_id` | Stable task or benchmark id |
| `task_type` | `build`, `test_fix`, `compile_fix`, `refactor`, `review`, `navigation`, etc. |
| `repo` | Repository identifier |
| `branch` | Branch name during the episode |
| `goal` | Short task goal |
| `success` | `true`, `false`, or `unknown` |
| `outcome_source` | `human`, `test`, `commit`, `benchmark`, or `rollout` |
| `privacy_profile` | Redaction policy used during capture |
| `split_hint` | Optional `train`, `eval`, `test`, or `holdout` |

When Foxglove Datasets are available, create datasets from these Events instead of directly from raw Recordings. This keeps dataset membership aligned to task episodes rather than arbitrary file boundaries.

## Training Example Shape

Each training example should represent a time-aligned state/action transition inside an Event episode.

```json
{
  "episode_id": "evt_abc123:segment-003",
  "t": 123.45,
  "goal": "Fix failing Rust unit test in kmmon",
  "observation": {
    "screen_frame_ref": "frame_000123.jpg",
    "sway_focus": {},
    "sway_tree": {},
    "keyboard_activity": {},
    "mouse_activity": {},
    "git_state": {},
    "terminal_state": {}
  },
  "action": {
    "type": "terminal_command",
    "command": "cargo test",
    "target": "focused_terminal"
  },
  "outcome": {
    "exit_code": 0,
    "task_success": true
  }
}
```

Recommended artifacts:

| Artifact | Format |
|----------|--------|
| Raw session | MCAP |
| Indexed metadata | Foxglove recordings, sessions, events, and datasets |
| Derived features | Parquet or Arrow |
| Model examples | JSONL |
| Screenshot frames | JPEG or PNG bundle, if enabled |
| Text/diff artifacts | JSON and patch files |
| Labels | Foxglove Event metadata and exported JSON |

## Text Edit Representation

Represent text edits as both file diffs and editor operations, with diffs as canonical ground truth.

| Representation | Role |
|----------------|------|
| File diff | Ground truth for what changed |
| Editor operation | HCI/action learning signal |
| Raw keystrokes | Avoid except in synthetic or sandbox-only tasks |
| Final file snapshot | Outcome verification |
| Git commit | Episode-level success artifact |

Diffs are easier to score, replay, deduplicate, and use for supervised fine-tuning. Editor operations remain valuable for HCI modeling but should not be the only source of truth.

## Agent Action Model

The useful agent should be hybrid. Desktop APIs capture HCI behavior, while direct developer tools make the software-engineering workflow practical and safer.

Action layers:

| Layer | Use |
|-------|-----|
| Sway IPC | Focus window, switch workspace, inspect tree |
| Computer-use actions | Screenshot, click, scroll, type, shortcuts |
| Terminal tool | Run commands and observe outputs |
| Patch/editor tool | Make precise code edits |
| Git tool | Inspect branch, diff, status, and commit in sandbox |
| Safety monitor | Block production, secrets, destructive commands |

Maintain two evaluation tracks:

| Track | Purpose |
|-------|---------|
| Desktop-only | Measures HCI competence and GUI grounding |
| Tool-augmented | Measures useful software-engineering performance |

Tool-augmented evaluation should be the product path. Desktop-only evaluation remains useful for research and for tasks where UI interaction is the point.

## Episode Segmentation

Episode start signals:

| Signal | Example |
|--------|---------|
| Manual annotation | User starts a task |
| External task link | Linear or GitHub issue opened |
| Repository transition | New working directory |
| Idle-to-active transition | Long idle gap followed by activity |
| Command start | First shell command in a task |

Episode end signals:

| Signal | Example |
|--------|---------|
| Manual annotation | User marks task complete |
| Tests pass | `cargo test` succeeds |
| Commit created | Git commit on task branch |
| Long idle gap | No activity for N minutes |
| Failure label | Human marks task abandoned |

## Training Objectives

Primary objectives:

| Objective | Description |
|-----------|-------------|
| Next action prediction | Predict human action from current observation and goal |
| Task phase classification | Identify navigation, inspection, editing, testing, debugging |
| Outcome prediction | Predict whether current trajectory is likely to succeed |
| Action grounding | Map screen and Sway state to target window or UI element |
| Recovery behavior | Learn undo, retry, inspect logs, and rerun tests |

Auxiliary objectives from current `kmmon` telemetry:

| Signal | Use |
|--------|-----|
| WPM estimate | Distinguish text-entry phases |
| Mouse activity | Distinguish navigation and inspection |
| Scroll events | Detect reading and searching |
| Idle transitions | Segment cognition pauses and task boundaries |
| Pointer paths | Learn human-like targeting and UI navigation |

## Evaluation

Offline evaluation:

| Metric | Description |
|--------|-------------|
| Action accuracy | Exact or type-level next-action match |
| Target accuracy | Correct window, control, or terminal target |
| Command similarity | Semantic match against human command |
| Phase F1 | Task phase classification quality |
| Trajectory edit distance | Difference from human action sequence |
| Outcome prediction AUROC | Predict task success or failure |

Online sandbox evaluation:

| Task | Success criterion |
|------|-------------------|
| Build project | Runs correct build command successfully |
| Fix compile error | Produces patch and passing build |
| Fix unit test | Makes targeted test pass |
| Navigate repo | Opens correct files from task prompt |
| Run formatter | Applies expected formatting |
| Commit change | Creates correct signed or unsigned benchmark commit in sandbox |

Safety evaluation:

| Check | Requirement |
|-------|-------------|
| Secrets | No passwords or tokens captured or emitted |
| Production actions | Block or require human handoff |
| Filesystem | Restrict to sandbox repo |
| Network | Allowlist package and documentation endpoints |
| Git | Never force push or rewrite history |
| Desktop | Keep agent inside dedicated Sway session |

## Foxglove Workflow

1. `kmmon` records MCAP locally.
2. Completed MCAP files upload to S3-compatible BYOS storage.
3. Foxglove indexes files using embedded `foxglove` metadata.
4. Rolled files are grouped by `sessionKey` into a Foxglove Session.
5. Human or automated jobs create Foxglove Events for task episodes.
6. Event metadata stores task type, repo, labels, success, and split hints.
7. V0 export uses `POST /v1/data/stream` over event time ranges and selected topics.
8. Derived jobs turn MCAP slices into JSONL, Parquet, frame bundles, and diff artifacts.
9. Future Foxglove Datasets group curated Events and expose object-storage paths for training.
10. Model rollouts are recorded back to MCAP and compared against human demonstrations.

## Implementation Phases

### Phase 1: Session And Metadata

- Add `sessionKey` and per-file `key` to the `foxglove` MCAP metadata record.
- Add `kmmon.schemaVersion`, `kmmon.captureMode`, and `kmmon.privacyProfile` metadata.
- Document metadata keys and update NixOS options.
- Confirm BYOS indexing and `GET /v1/data/pending-imports` visibility.

### Phase 2: Sway Context

- Add a Sway IPC collector.
- Publish `/sway/tree`, `/sway/focus`, `/sway/window_event`, and `/sway/workspace_event`.
- Redact or hash window titles by default.
- Add tests for schema serialization and redaction behavior.

### Phase 3: Demonstration Capture

- Add explicit demonstration-mode config.
- Add `/human/click`, `/human/shortcut`, `/terminal/command`, `/terminal/result`, `/git/state`, and `/repo/diff`.
- Add redaction policy configuration.
- Add task annotation CLI or local UI that creates Foxglove Events.

### Phase 4: Dataset Export

- Build an export tool that queries Events, resolves overlapping Sessions/Recordings, and calls `POST /v1/data/stream`.
- Emit aligned JSONL/Parquet examples and optional screenshot bundles.
- Add schema-version validation in export jobs.
- Preserve compatibility with future Foxglove Datasets by keeping Event ids as episode ids.

### Phase 5: Baseline Agent

- Train a baseline behavior-cloning model for Sway navigation, shell command selection, and patch generation.
- Evaluate on deterministic sandbox repositories.
- Record model rollouts back to MCAP for replay.

### Phase 6: Useful Agent

- Add sandboxed Sway runtime.
- Add safety monitor and action allowlist.
- Add benchmark suite for software-engineering tasks.
- Capture human-in-the-loop corrections as new demonstrations.

## Acceptance Criteria

- A `kmmon` session can be recorded, uploaded, indexed, queried, and replayed in Foxglove.
- Rolled MCAP files for one work session are grouped by `sessionKey`.
- A task interval can be represented as a Foxglove Event with searchable metadata.
- An export job can produce aligned observation/action/outcome examples from selected topics and event time ranges.
- A baseline model can learn at least one bounded workflow, such as opening a repo, running tests, editing a file, and rerunning tests.
- Model rollouts can be recorded as MCAP and compared against human demonstrations.
- Privacy-preserving activity mode remains the default and never records key identity or typed text.

## Open Risks

- The planned `/v1/datasets` API may differ from the current product direction; V0 should remain functional using Events and Sessions.
- Screenshot capture may leak sensitive information unless confined to a sandbox or paired with strong redaction.
- Terminal command capture requires robust secret redaction.
- Browser-based software-engineering tasks are exposed to prompt injection through page content and screenshots.
- High-bitrate screen topics can make index-in-place queries expensive unless partitioned from low-bitrate telemetry.
- Wayland input injection and screenshot capture vary by compositor security settings; sandboxed Sway should be the primary rollout target.
