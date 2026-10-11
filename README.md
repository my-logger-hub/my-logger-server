# my-logger-server

A log server: takes logs over HTTP and gRPC, keeps them in files by the hour, optionally archives them to S3 and forwards them to Elastic and Telegram. How it works inside: [docs/specification.md](docs/specification.md).

## Build

Needs Rust and `protoc` (the Protocol Buffers compiler) in `PATH`.

```bash
cargo build --release
```

The [Dockerfile](Dockerfile) packs `target/release/my-logger-server`, so build first, then `docker build .`.

## Settings

The server reads its settings from the YAML file `~/.my-logger-server`. When there is no such file, it reads them from the URL in the `SETTINGS_URL` environment variable (sending `ENV_INFO`, when it is set, as the `env-info` header).

```yaml
logs_db_path: /root/db # required: where the logs and the server's own files are kept
env_name: prod # required: shown in the Telegram messages

telegram_settings: # optional: no Telegram without it
  api_key: string # the bot token
  chat_id: -1001234567890
  message_thread_id: 1

elastic: # optional: the records are copied to Elastic with it
  url: https://elastic:9200
  esecure: string
  env_source: prod

s3: # optional: without it the Warning/Error/Fatal logs are deleted 2 hours after their hour ends
  endpoint: https://fsn1.your-objectstorage.com
  region: fsn1
  access_key: string
  secret_key: string
  bucket: my-logs
  prefix: logs # optional, logs when not set
```

The settings are re-read every minute, but `logs_db_path`, `env_name`, `elastic` and `s3` are taken at start: after changing them, restart the server.

## Environment variables

| Variable | Default | |
|---|---|---|
| `HTTP_PORT` | `8000` | The HTTP port. |
| `UNIX_SOCKET` | — | Also serve HTTP on this unix socket. |
| `SETTINGS_URL` | — | Where to read the settings from when there is no `~/.my-logger-server`. |
| `ENV_INFO` | — | Sent as the `env-info` header with the request to `SETTINGS_URL`. |
| `DEBUG` | — | `true` or `1` — print more diagnostics. |

## Ports

| Port | |
|---|---|
| `8000` (`HTTP_PORT`) | Writing logs: `POST /api/v2` (CLEF JSON), `POST /api/events/raw` (Seq). Swagger. MCP at `/mcp`. |
| `8888` | gRPC — [proto/MyLogger.proto](proto/MyLogger.proto): writing (`Write`) and the UI. |

A writer may name the product scope of its logs with the `scope` header (HTTP) or the `scope` metadata (gRPC). Without it, the logs go by the product scopes set, and to `default` when none matches.

## After the start

Nothing else is required: without further settings all the logs go to the `default` scope, and with `s3` the logs are kept in S3 for a week.

The rest is set at runtime through MCP (`/mcp`) and kept by the server in `<logs_db_path>/settings.json` — not by hand:

- product scopes and the default scope — `set_product_scope`, `delete_product_scope`, `set_default_product_scope`;
- how long the logs are kept in S3 — `set_hours_to_gc`;
- ignore rules — `add_ignore_rule`, `delete_ignore_rule`.
