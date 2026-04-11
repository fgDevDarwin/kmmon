# kmmon

Keyboard and mouse activity monitor for Linux. Streams live telemetry to
[Foxglove Studio](https://foxglove.dev) and writes
[MCAP](https://mcap.dev) recordings for later playback — without ever
logging what you typed.

## Privacy first

Most input monitors record keystrokes. kmmon doesn't.

When a key event arrives from the kernel, **the key code is discarded
immediately** — before it touches any data structure, log, or network
socket. The only thing retained is the arrival time, which feeds a
rolling 60-second window used to approximate words per minute.

```
kernel key event
       │
       ├─ key code ──► dropped on the floor
       │
       └─ timestamp ──► rolling deque (max 60 s) ──► WPM estimate
```

No keystroke sequence, no key identity, no text reconstruction is
possible from the output. The published stream contains only:

| Topic | Fields |
|---|---|
| `/mouse/position` | `x`, `y` (accumulated position) |
| `/mouse/scroll` | `dx`, `dy` |
| `/keyboard/activity` | `keystrokes_per_minute`, `approx_wpm`, `active` |

## Features

- **Live visualisation** — connect Foxglove Studio to `ws://localhost:8765`
- **MCAP recording** — hourly rolling files, configurable local retention
- **S3 upload** — optional background upload for
  [Foxglove BYOB](https://docs.foxglove.dev/docs/data-platform/byob)
- **NixOS module** — one-liner to run as a hardened systemd service
- **Minimal footprint** — single-threaded async runtime, zero wakeups
  while input is idle

## Quick start

```bash
cargo run
```

Connect Foxglove Studio to `ws://localhost:8765`. Move the mouse or type
to see data appear on the `/mouse/position` and `/keyboard/activity`
topics.

MCAP files are written to `/tmp/kmmon-<timestamp>.mcap` by default.

## Configuration

All options are environment variables:

| Variable | Default | Description |
|---|---|---|
| `KMMON_PORT` | `8765` | WebSocket port |
| `KMMON_MCAP_DIR` | `/tmp` | Directory for MCAP recordings |
| `KMMON_ROLL_SECS` | `3600` | Start a new file after N seconds |
| `KMMON_RETENTION_SECS` | `604800` | Delete local files older than N seconds |
| `KMMON_S3_BUCKET` | *(unset)* | S3 bucket — enables upload when set |
| `KMMON_S3_PREFIX` | `recordings` | Key prefix within the bucket |
| `KMMON_S3_ENDPOINT_URL` | *(unset)* | Custom endpoint (Cloudflare R2, MinIO, …) |
| `AWS_ACCESS_KEY_ID` | *(unset)* | Standard AWS credential chain |
| `AWS_SECRET_ACCESS_KEY` | *(unset)* | Standard AWS credential chain |
| `AWS_REGION` | *(unset)* | AWS region |

## NixOS

```nix
# flake.nix
inputs.kmmon.url = "github:YOU/kmmon";

# nixosConfiguration
imports = [ inputs.kmmon.nixosModules.default ];

services.kmmon = {
  enable = true;
  openFirewall = true;          # allow Foxglove Studio to connect

  recording.rollEverySecs  = 3600;   # 1 h files
  recording.retainForSecs  = 604800; # keep 7 d locally

  s3 = {
    enable          = true;
    bucket          = "my-foxglove-bucket";
    region          = "us-east-1";
    credentialsFile = "/run/secrets/kmmon-aws-creds";
    # or omit credentialsFile and use an IAM instance role
  };
};
```

The service runs as a transient `DynamicUser` with only the `input`
group supplement it needs to read `/dev/input/event*`. Standard systemd
hardening (`ProtectSystem`, `PrivateTmp`, `NoNewPrivileges`, …) is
applied automatically.

## Building

```bash
nix develop   # enter shell with correct Rust toolchain
cargo build --release
cargo test
```

Requires Rust ≥ 1.83 (for the `foxglove` crate).

## WPM calculation

```
keystrokes_per_minute = |timestamps in last 60 s|
approx_wpm            = keystrokes_per_minute / 5
```

The divisor of 5 is the standard assumption (average English word ≈ 5
characters) used by typing-speed tests such as monkeytype and
10fastfingers. Only key-down events are counted; repeats and releases
are ignored.

## License

MIT
