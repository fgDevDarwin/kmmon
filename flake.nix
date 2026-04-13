{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = {
    self,
    flake-utils,
    naersk,
    nixpkgs,
    fenix,
  }:
    # System-specific outputs (packages, devShells)
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = (import nixpkgs) {
          inherit system;
          overlays = [fenix.overlays.default];
        };

        naersk' = pkgs.callPackage naersk {};
      in {
        packages.default = naersk'.buildPackage {
          src = ./.;
        };

        # Rust MSRV >= 1.83 required by the foxglove crate
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            alejandra
            rust-analyzer
            (pkgs.fenix.stable.withComponents [
              "cargo"
              "clippy"
              "rust-src"
              "rustc"
              "rustfmt"
            ])
          ];
        };
      }
    )
    # System-independent outputs (NixOS modules, overlays, …)
    // {
      nixosModules.default = {
        config,
        lib,
        pkgs,
        ...
      }: let
        cfg = config.services.kmmon;
        defaultPackage = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      in {
        options.services.kmmon = {
          enable = lib.mkEnableOption "kmmon keyboard & mouse monitor";

          package = lib.mkOption {
            type = lib.types.package;
            default = defaultPackage;
            defaultText = lib.literalExpression "kmmon";
            description = "The kmmon package to run.";
          };

          port = lib.mkOption {
            type = lib.types.port;
            default = 8765;
            description = ''
              TCP port for the Foxglove WebSocket server.
              Connect Foxglove Studio to ws://&lt;host&gt;:&lt;port&gt;.
            '';
          };

          mcapDir = lib.mkOption {
            type = lib.types.str;
            default = "/var/lib/kmmon";
            description = ''
              Directory where MCAP recordings are written.
              Created and owned by systemd (StateDirectory).
            '';
          };

          openFirewall = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Open the WebSocket port in the host firewall.";
          };

          logLevel = lib.mkOption {
            type = lib.types.str;
            default = "info";
            example = "debug";
            description = "RUST_LOG filter applied to the kmmon process.";
          };

          recording = {
            rollEverySecs = lib.mkOption {
              type = lib.types.ints.positive;
              default = 3600;
              description = "Start a new MCAP file after this many seconds (default: 1 h).";
            };

            retainForSecs = lib.mkOption {
              type = lib.types.ints.positive;
              default = 604800;
              description = "Delete local MCAP files older than this many seconds (default: 7 d).";
            };
          };

          foxglove = {
            projectId = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "prj_abc123";
              description = ''
                Foxglove project ID embedded in every MCAP file.
                Required for the data platform indexer to assign recordings
                to the correct project when files are indexed in place (BYOB).
              '';
            };

            deviceId = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "dev_abc123";
              description = ''
                Pre-registered Foxglove device ID. When set, takes precedence
                over deviceName during indexing. Leave null unless you have
                registered the device with the data platform and have its ID —
                unknown IDs cause the indexer to fail with "Device not found".
              '';
            };

            deviceName = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "Darwin's desktop";
              description = ''
                Human-readable device name embedded in MCAP metadata.
                Defaults to the machine hostname when null.
              '';
            };
          };

          s3 = {
            enable = lib.mkEnableOption "S3 upload of completed MCAP recordings (Foxglove BYOB)";

            bucket = lib.mkOption {
              type = lib.types.str;
              description = "S3 bucket name to upload recordings to.";
            };

            prefix = lib.mkOption {
              type = lib.types.str;
              default = "recordings";
              description = "Key prefix within the bucket (no leading/trailing slash).";
            };

            region = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "us-east-1";
              description = "AWS region. Falls back to AWS_REGION env var if null.";
            };

            endpointUrl = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "https://123abc.r2.cloudflarestorage.com";
              description = ''
                Custom endpoint for S3-compatible storage (Cloudflare R2, MinIO, …).
                Leave null for standard AWS S3.
              '';
            };

            credentialsFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              example = "/run/secrets/kmmon-s3-credentials";
              description = ''
                Path to a secrets file (not world-readable) containing:
                  AWS_ACCESS_KEY_ID=AKIA…
                  AWS_SECRET_ACCESS_KEY=…
                Omit if using an IAM instance role or other ambient credentials.
              '';
            };
          };
        };

        config = lib.mkIf cfg.enable {
          networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [cfg.port];

          systemd.services.kmmon = {
            description = "Keyboard & Mouse Monitor (kmmon)";
            wantedBy = ["multi-user.target"];
            after = ["local-fs.target" "systemd-udev-settle.service"];

            environment =
              {
                RUST_LOG = cfg.logLevel;
                KMMON_PORT = toString cfg.port;
                KMMON_MCAP_DIR = cfg.mcapDir;
                KMMON_ROLL_SECS = toString cfg.recording.rollEverySecs;
                KMMON_RETENTION_SECS = toString cfg.recording.retainForSecs;
              }
              // lib.optionalAttrs (cfg.foxglove.projectId != null) {KMMON_FOXGLOVE_PROJECT_ID = cfg.foxglove.projectId;}
              // lib.optionalAttrs (cfg.foxglove.deviceId != null) {KMMON_FOXGLOVE_DEVICE_ID = cfg.foxglove.deviceId;}
              // lib.optionalAttrs (cfg.foxglove.deviceName != null) {KMMON_FOXGLOVE_DEVICE_NAME = cfg.foxglove.deviceName;}
              // lib.optionalAttrs cfg.s3.enable (
                {
                  KMMON_S3_BUCKET = cfg.s3.bucket;
                  KMMON_S3_PREFIX = cfg.s3.prefix;
                }
                // lib.optionalAttrs (cfg.s3.region != null) {AWS_REGION = cfg.s3.region;}
                // lib.optionalAttrs (cfg.s3.endpointUrl != null) {KMMON_S3_ENDPOINT_URL = cfg.s3.endpointUrl;}
              );

            serviceConfig = {
              ExecStart = "${cfg.package}/bin/kmmon";

              Restart = "on-failure";
              RestartSec = "5s";

              DynamicUser = true;
              SupplementaryGroups = ["input"];

              StateDirectory = "kmmon";
              ReadWritePaths = [cfg.mcapDir];

              # Load AWS credentials from a secrets file when configured.
              # The leading "-" makes a missing file a warning rather than a
              # hard failure, so the service still starts if the secret hasn't
              # been provisioned yet (upload will be a no-op without the creds).
              EnvironmentFile =
                lib.optional (cfg.s3.credentialsFile != null) "-${cfg.s3.credentialsFile}";

              NoNewPrivileges = true;
              ProtectSystem = "strict";
              ProtectHome = true;
              PrivateTmp = true;
              RestrictNamespaces = true;
              RestrictRealtime = true;
              MemoryDenyWriteExecute = true;
            };
          };
        };
      };
    };
}
