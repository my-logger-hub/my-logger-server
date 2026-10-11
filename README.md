# my-logger-server

Log server in Rust. Accepts logs over gRPC/HTTP, stores them locally, and (optionally) forwards to Elastic and Telegram.

## Log storage

Logs are kept in plain files, one pair of files per hour: no search engine and no external process. A search is always bounded by a time interval: the index says where a second starts, and the data is read forward from there.

### The files of an hour

| Stream | Levels | Folder | Kept |
|---|---|---|---|
| main | Warning, Error, FatalError | `<LogsDbPath>/logs/` | on the local disk until 2 hours after the hour ends, then uploaded to S3 and kept there for `hours_to_gc`; without S3, locally for `hours_to_gc` |
| debug-info | Debug, Info | `<LogsDbPath>/debug-info/` | deleted 2 hours after the hour ends |

An hour is `YYYYMMDDHH.data`, `YYYYMMDDHH.index` and `YYYYMMDDHH.index-by-key`.

**`.data`** is made of 32 KiB frames, the layout of the working frames of [archived-data](https://github.com/my-jet-tools/archived-data): a header `size u32 · next u32 · used u16`, then `used` bytes of entries. The frames are one chain in time order; `next` is the offset of the next frame, 0 on the last one. An entry is `[varint length][time i64][zstd block]`: the records of one second, each protobuf-encoded behind a varint length (rust-extensions `UInt32VariableSize`), compressed together, at most 8 KiB. A busy second takes several blocks.

**`.index`** is 3600 cells `offset u32 · count u32`, one per second of the hour: the offset of the first entry of the second in the data file and how many records the second has. Then one byte: 0 when the cells agree with the data file. A flush sets it to 1 before it writes the frames and back to 0 together with the cells; an index found at 1 is rebuilt by walking the chain.

**`.index-by-key`** — for the level, the process, `Application`, `Version` and the `InsightsKeys` of the settings: how many records of each second have each value (lowercase). A directory first — every key and value, and where its seconds are — then, packed, the seconds each value is in: `varint(step from the second before) · varint(records)`, 2–3 bytes a second. A key with more than 1000 values in an hour is not indexed in it. For the current and the previous hour it is kept in memory whole, as the writes build it, and written at most every 5 seconds, when the hour is sealed and when the process stops; one which does not count the records the index counts is rebuilt from the data file. Of an older hour — on the local disk or in S3 alike — only the page of its keys and values is read: one 64 KiB read from its start, its header telling whether the page goes on past it; what the read brought in past the page is kept, so an index shorter than one read is read once. The seconds of a value not brought in yet are read when a filter asks for it — blocks less than 64 KiB apart in one read, always forward — and kept.

### Writing

- Records wait in memory for 5 seconds, grouped by second, so records that come a little late join the block of their second.
- New records for a second that is already written are merged into its last block: the block is decompressed, the records are added, and it is compressed again in its place.
- An insert into the middle never moves the file. A frame with room moves its entries after the place up; a full frame at the end of the chain gets a new frame after it; a full frame elsewhere is halved, and its upper half moves into the next frame or into a new frame appended to the file and chained in between. Only the changed frames are written, and the cells of the moved entries follow them.
- The indexes of the current and the previous hour stay in memory. Other hours are opened when they are read and let go after 10 idle minutes.

### Searching

A search goes one of two ways, both stopping at the other end of the range or at `take` records, whichever comes first:

- **up** (`SearchUp`) — from `from`, the oldest first: the hours one after another, each read forward from the cell of the first second of the range;
- **down** (`SearchDown`, `Read`, `ScanAndSearch`, MCP) — from `to`, the latest first: the bottom second is found in the index, which is in memory — down from `to` until the counts of the seconds add up to `take`, and to one read at least (64 KiB locally, 1 MiB from S3) — and the data is read up from it in one pass. A filter by level or by indexed keys turns the counts into the counts of `index-by-key` for its values — exact for one key, an upper bound for several or with a phrase: when fewer records match, the next bottom second is found the same way for what is missing. Seconds with no match are not read; the ones between two matching seconds are read through only while they hold less than one read. The files are read as streams through rust-extensions `BufferedReader`: a local file from the offset, an S3 object as a ranged download. Context filters are exact `key=value` matches and the phrase is a substring of the message, the process or a context value, both ignoring case. Records still waiting in memory are found too.

### S3

With `S3Settings` set, a main hour 2 hours past its end is sealed and copied for the upload: the data file into frames which follow one another, each filled before the next one starts, so a read goes forward from the cell of its second with no jump; the index pointing into them; the key index whole, its page first. They are uploaded — the data file, the key index, the index last — and deleted locally. Searches read such hours from S3. Every 10 minutes the hours older than `hours_to_gc` are deleted from S3.

## MCP server (read-only for AI)

The server exposes an MCP endpoint at **`/mcp`** on the main HTTP port (Streamable HTTP transport, JSON-RPC 2.0 + SSE). Implemented via `mcp-server-middleware` on top of `my-http-server` ([src/http/start_up.rs](src/http/start_up.rs), [src/mcp/](src/mcp/)).

Goal: let AI assistants read logs. Write/admin operations are **not exposed** through MCP — the endpoint is strictly read-only.

### One registered tool: `search_logs`

Combines every filter shape in a single call. All parameters are optional except the time-range pair.

| Parameter         | Type            | Purpose                                                                       |
|-------------------|-----------------|-------------------------------------------------------------------------------|
| `phrase`          | `string`        | Case-insensitive substring of the message, the process or a context value.   |
| `last_minutes`    | `integer`       | If > 0, range becomes `[now − N min..now]`. Overrides `from_time`/`to_time`.  |
| `from_time`       | `integer` (us)  | Used only when `last_minutes` is absent. Must be paired with `to_time`.       |
| `to_time`         | `integer` (us)  | Paired with `from_time`. Must satisfy `from_time` < `to_time`.                |
| `levels`          | `string[]`      | One of `info`, `warning`, `error`, `fatal_error`, `debug`. Case-insensitive.  |
| `context_filters` | `string[]`      | List of `key=value` entries for exact context match.                          |
| `take`            | `integer`       | Limit. Default 100, range 1..1000.                                            |

**Response** — field `items_json` containing a JSON array of the latest `take` records of the range, sorted by `timestamp` ascending. Each item has `timestamp`, `iso_time`, `level`, `message`, `context` (preserving original case).

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
S3Settings:
  endpoint: https://fsn1.your-objectstorage.com
  region: fsn1
  access_key: string
  secret_key: string
  bucket: my-logs
  prefix: string # optional, EnvName when not set
```

`LogsDbPath` — the root directory: `logs/` and `debug-info/` with the hour files, `settings.json` (ignore-events) and `statistics.json` (hourly aggregates).

`S3Settings` is optional; without it the main hours stay on the local disk for `hours_to_gc`.
