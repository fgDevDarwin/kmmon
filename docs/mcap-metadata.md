# MCAP Metadata

kmmon embeds a `"foxglove"` MCAP Metadata record in every file it creates.
The Foxglove data platform indexer reads this record to resolve project and
device context when files are indexed in place (BYOB without the inbox→lake
flow).

## Record

| Field       | Value                                              |
|-------------|----------------------------------------------------|
| Record name | `foxglove`                                         |
| Encoding    | MCAP Metadata (key-value `BTreeMap<String,String>`) |

## Keys

| Key          | Source                          | Required | Default     |
|--------------|---------------------------------|----------|-------------|
| `projectId`  | `KMMON_FOXGLOVE_PROJECT_ID`     | Yes      | —           |
| `deviceId`   | `KMMON_FOXGLOVE_DEVICE_ID`      | No       | hostname    |
| `deviceName` | `KMMON_FOXGLOVE_DEVICE_NAME`    | No       | hostname    |

`projectId` must be set for the indexer to assign the recording to the
correct project. Without it the platform falls back to a server-side
resolver which may create a default project.

`deviceId` and `deviceName` default to the machine hostname. Override them
when the same host records under multiple identities or when the hostname
is not human-friendly.

## Timing

The metadata record is written immediately after each MCAP file is opened
— both the initial file and every file produced by a roll. This ensures the
record appears in the file's summary section and is available to the
indexer without a full scan.

## NixOS options

```nix
services.kmmon.foxglove = {
  projectId  = "prj_abc123";        # required when S3 upload is enabled
  deviceId   = "darwin-workstation"; # optional, defaults to hostname
  deviceName = "Darwin's desktop";   # optional, defaults to hostname
};
```
