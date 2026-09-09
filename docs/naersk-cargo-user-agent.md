# naersk Cargo User Agent

## Purpose

Ensure crate downloads made while naersk builds `kmmon` identify themselves with Cargo's user agent and version.

## Behavior

### Crate download without existing curl options

Given naersk requests a crate through its `fetchurl` dependency without `curlOptsList`,
when the request is passed to the underlying Nixpkgs `fetchurl`,
then its curl options include `--user-agent` followed by `cargo/${pkgs.cargo.version}`.

### Crate download with existing curl options

Given naersk requests a crate with an existing `curlOptsList`,
when the request is passed to the underlying Nixpkgs `fetchurl`,
then all existing options retain their order and the Cargo user-agent pair is appended.

### Cargo version source

Given the Cargo package in the selected Nixpkgs package set has a version,
when the user-agent value is formed,
then it is exactly `cargo/${pkgs.cargo.version}` using that package set's Cargo version rather than a hard-coded version.

### Scope

Given any `pkgs.fetchurl` use outside naersk's crate-download path,
when it runs,
then this behavior does not add or change its curl options.

## Edge Cases

- A missing `curlOptsList` is treated as an empty list.
- Existing curl options are preserved, including any existing user-agent option; this behavior only appends Cargo's user-agent pair.
- Changing the pinned Nixpkgs Cargo version changes the generated user-agent value without another source edit.

## Non-Goals

- Changing Cargo, naersk, or Nixpkgs input versions.
- Changing download URLs, hashes, retries, proxy settings, or fetch failure behavior.
- Applying the Cargo user agent to fetches outside naersk.

## Acceptance Tests

1. Evaluate the naersk-specific fetch configuration with no `curlOptsList` and assert the resulting list ends with `["--user-agent" "cargo/${pkgs.cargo.version}"]`.
2. Evaluate it with representative existing curl options and assert those options are unchanged and precede the appended user-agent pair.
3. Assert the value uses the evaluated `pkgs.cargo.version`, not a literal version string.
4. Perform an uncached `nix build .#default` with `NIX_CURL_FLAGS` unset and verify a naersk crate request sends the expected `User-Agent` header.
