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
| `deviceId`   | `KMMON_FOXGLOVE_DEVICE_ID`      | No       | *(omitted)* |
| `deviceName` | `KMMON_FOXGLOVE_DEVICE_NAME`    | No       | hostname    |

`projectId` must be set for the indexer to assign the recording to the
correct project. Without it the platform falls back to a server-side
resolver which may create a default project.

**`deviceId` vs `deviceName`**: the data platform treats `deviceId` as a
foreign-key reference to an already-registered device. Setting it to a
value that does not exist produces a 404 "Device not found" at index
time. `deviceName` is free-form — if no device with that name exists,
one is auto-created server-side. The data platform prefers `deviceId`
when both are present.

kmmon therefore **omits `deviceId` by default** and defaults `deviceName`
to the machine hostname. Set `KMMON_FOXGLOVE_DEVICE_ID` only when you
have pre-registered the device and have its real ID.

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
