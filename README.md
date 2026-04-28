# my-logger-server

Log server in Rust. Accepts logs over gRPC/HTTP, stores them locally, and (optionally) forwards to Elastic and Telegram.

## Log storage

Logs are stored in [Tantivy](https://github.com/quickwit-oss/tantivy) — an embedded search engine in pure Rust. No external process (SQLite/Elastic/JVM) is needed for storage.

### Hourly sharding

Each hour gets its own Tantivy index — a separate folder named `logs-YYYYMMDDHH` inside `LogsDbPath`. This gives:

- **Time-based pruning**: a query with interval `[from..to]` opens only the hour shards inside that range.
- **Trivial retention**: GC removes the whole folder (`remove_dir_all`), no `DELETE` or vacuum.

### Index schema

| Field         | Type                            | Purpose                                                  |
|---------------|---------------------------------|----------------------------------------------------------|
| `timestamp`   | `i64` INDEXED + FAST + STORED   | range filter and DESC ordering                           |
| `id`          | STORED                          | record identifier                                        |
| `level`       | STRING (raw, lowercase)         | exact level match                                        |
| `message`     | STORED                          | returned to clients (full-text covered by `text_search`) |
| `ctx`         | STRING multi-value (raw)        | each context pair indexed as `key=value` lowercase       |
| `ctx_data`    | STORED                          | original-case context as JSON for retrieval              |
| `text_search` | TEXT (default + lowercase)      | full-text over `message` + process + context values      |

Context is indexed in lowercase for case-insensitive lookup, but returned to clients with original case via `ctx_data`.

### Query patterns

Every query is bounded by a time interval. Example combined query:

```
timestamp:[from..to]
  AND level:error
  AND ctx:"application=billing"
  AND ctx:"version=1.4.2"
  AND text_search:timeout
```

Returns top-N results sorted by `timestamp DESC` via the FAST field — tens of milliseconds even on millions of records.

### GC

- `gc_files` — deletes shards older than `hours_to_gc`.
- `gc_level` — inside the last two shards, prunes Debug/Info older than 60 minutes and Warning older than 6 hours via `IndexWriter::delete_query`.

## MCP server (read-only for AI)

The server exposes an MCP endpoint at **`/mcp`** on the main HTTP port (Streamable HTTP transport, JSON-RPC 2.0 + SSE). Implemented via `mcp-server-middleware` on top of `my-http-server` ([src/http/start_up.rs](src/http/start_up.rs), [src/mcp/](src/mcp/)).

Goal: let AI assistants read logs. Write/admin operations are **not exposed** through MCP — the endpoint is strictly read-only.

### One registered tool: `search_logs`

Combines every filter shape in a single call. All parameters are optional except the time-range pair.

| Parameter         | Type            | Purpose                                                                       |
|-------------------|-----------------|-------------------------------------------------------------------------------|
| `phrase`          | `string`        | Full-text search over message, process, and context values.                   |
| `last_minutes`    | `integer`       | If > 0, range becomes `[now − N min..now]`. Overrides `from_time`/`to_time`.  |
| `from_time`       | `integer` (us)  | Used only when `last_minutes` is absent. Must be paired with `to_time`.       |
| `to_time`         | `integer` (us)  | Paired with `from_time`. Must satisfy `from_time` < `to_time`.                |
| `levels`          | `string[]`      | One of `info`, `warning`, `error`, `fatal_error`, `debug`. Case-insensitive.  |
| `context_filters` | `string[]`      | List of `key=value` entries for exact context match.                          |
| `take`            | `integer`       | Limit. Default 100, range 1..1000.                                            |

**Response** — field `items_json` containing a JSON array sorted by `timestamp DESC`. Each item has `timestamp`, `iso_time`, `level`, `message`, `context` (preserving original case).

### Typical AI queries

| User intent                                          | Tool-call arguments                                                         |
|------------------------------------------------------|-----------------------------------------------------------------------------|
| "logs for the last hour"                             | `{ "last_minutes": 60 }`                                                    |
| "errors in the last 30 minutes"                      | `{ "last_minutes": 30, "levels": ["error", "fatal_error"] }`                |
| "what billing was doing in the last hour"            | `{ "last_minutes": 60, "context_filters": ["Application=billing"] }`        |
| "find timeout in logs for the last 2 hours"          | `{ "last_minutes": 120, "phrase": "timeout" }`                              |
| "billing v1.4.2 errors over last 24h"                | `{ "last_minutes": 1440, "levels": ["error"], "context_filters": ["Application=billing", "Version=1.4.2"] }` |

What AI **cannot** do via MCP: write logs, modify ignore-events, trigger GC, read dashboard statistics, or change settings. Search only.

## Settings

```yaml
EnvName: env-name
LogsDbPath: /root/db
hours_to_gc: 6
IgnoreEvents:
- level: Info
  application: app-name
  marker: TextEntry
TelegramSettings:
  api_key: string
  chat_id: string
  message_thread_id: number
  env_info: string
```

`LogsDbPath` — root directory that holds hourly Tantivy index folders (`logs-YYYYMMDDHH/`) plus `settings.json` (ignore-events) and `statistics.json` (hourly aggregates).
