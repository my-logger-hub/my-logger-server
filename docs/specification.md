# my-logger-server — specification

This document is the contract between the UI and the server, and the description of how the server keeps and searches logs. Where the implementation differs from it, that is a bug. The gRPC contract itself is [proto/MyLogger.proto](../proto/MyLogger.proto).

## Writing logs

- HTTP `POST /api/v2` — records in Compliant JSON (CLEF);
- HTTP `POST /api/events/raw` — records in the Seq format;
- gRPC `Write` — a stream of `LogEventGrpcModel`.

A batch first goes through the ignore events and the ignore single events, then it is counted in the hourly statistics and for Telegram, copied to Elastic when it is set up, and put into the storage. The `scope` header (HTTP) or the `scope` metadata (gRPC) names the product scope of the whole batch — see [Product scopes](#product-scopes).

---

## Time-range contract

Every method taking a time range (`Read`, `ScanAndSearch`, `SearchUp`, `SearchDown`) encodes it in `from_time` (`int64`) and `to_time` (`int64`) by one rule. The UI encodes one of three states; the server decodes them the same way.

### States

| UI state                  | `from_time`                                           | `to_time`                | Meaning on the server                            |
|---------------------------|-------------------------------------------------------|--------------------------|--------------------------------------------------|
| `HoursAgo(N)`, `N >= 0`   | `-N` (`0` for `N = 0`)                                | `0`                      | one hour: `now + from_time` hours                |
| `ExactHour(date_key)`     | `date_key` as `YYYYMMDDHH` (>= `2024010100`)          | `0`                      | one hour by its absolute key                     |
| `Range(from_us, to_us)`   | unix microseconds (> 0)                               | unix microseconds (> 0)  | the range `[from_us..to_us]`                     |

### Decoding (server)

```
if to_time == 0:
    if from_time <= 0:
        # an offset in hours from now
        date_key = DateHourKey(now + from_time hours)
    else:
        # an absolute date_key YYYYMMDDHH
        date_key = DateHourKey(from_time)
    → read ONE hour
else:
    # a range in unix microseconds
    → read every hour within [from_time .. to_time]
```

### UI side

[`models/time_range.rs::TimeRange::get_date_from_date_to`](https://github.com/MyJetTools/MyLogger/blob/main/my-logger-ui/src/models/time_range.rs):

```rust
match self {
    HoursAgo(n)         => (-(n as i64), 0),
    Range(from, to)     => (from.unix_us, to.unix_us),
    ExactHour(date_key) => (date_key.value, 0),
}
```

### Server side

[`grpc_server/my_logger_grpc_service.rs`](../src/grpc_server/my_logger_grpc_service.rs) — every method calls the one function `RequestType::from_request(from_time, to_time)`; the parsing lives in exactly one place:

```rust
if to_time == 0 {
    if from_time <= 0 {
        DateHourKey::from(now.add_hours(from_time))
    } else {
        DateHourKey::from(from_time)
    }
} else {
    DateRange(from_time.into(), to_time.into())
}
```

### Not allowed

- A positive `from_time < 2024010100` with `to_time == 0` — the UI does not send it; the server takes it as an invalid date_key and returns nothing.
- `to_time < 0` — not supported.
- `from_time > to_time` in the range mode — undefined (an empty result).

---

## Product scopes

Records are kept and searched by product scope:

- a record goes to the scope its writer names in the `scope` header (HTTP `POST /api/v2`, `/api/events/raw`) or the `scope` metadata (gRPC `Write`) — for the whole batch;
- without it, to the first scope with a regular expression matching its `Application` whole, ignoring case (`(?i)^(?:…)$`), in the order the scopes are set;
- otherwise — or with no `Application` — to the default scope.

A scope name is `a-z`, `0-9`, `-` and `_`, at most 64 characters; upper case is turned to lower case. A header which can not name a scope is ignored: the record goes by the settings.

The scopes and the default scope are set through MCP and kept in `settings.json`; the default scope is `default` until another one is set. It may be one of the scopes which are set: then both its applications and everything no scope matches go to it. A change applies to the records which come after it; the ones written stay in their scope and are searched by its name.

A search goes over one scope: `ProductScope` of `ReadLogEventRequest`, `SearchLogsRequest` and `ScanAndSearchRequest` (`optional`; missing or empty — the default scope; a name which can not be a scope — `INVALID_ARGUMENT`), `product_scope` of the MCP `search_logs`. A scope with no records gives an empty result.

`GetProductScopes` returns the default scope (`DefaultScope`) and the list: the default scope first, then the scopes which are set, in the order they are matched, each with its regular expressions, then the other scopes there are records of on the disk (from the header only, for example), with none.

Statistics, ignore events and Telegram are shared by all scopes. `GetHourlyStatistics` and the MCP `get_dashboard_statistics` count every scope, each application with the scope its records went to (`Scope`, `product_scope`) — the one to search them in. An application whose records went to several scopes has a row per scope.

---

## Search modes

A search is bounded by a range and `take`, and goes one of two ways — until the other end of the range or until `take` matching records, whichever comes first:

- **up** — from the start of the range, the oldest first: the hours one after another, each read forward from the index cell of the first second of the range;
- **down** — from the end of the range, the latest first: the bottom second is found in the index, which is in memory — down from the end of the range until the counts of the seconds add up to `take`, and to one read at least (64 KiB locally, 1 MiB from S3) — and the data is read up from it in one pass. A filter by level or by indexed keys turns the counts into the counts of `index-by-key` for its values — exact for one key, an upper bound for several or with a phrase: when fewer records match, the next bottom second is found the same way for what is missing, corrected by the share which matched. Seconds with no match are not read; the ones between two matching seconds are read through only while they hold less than one read.

The files are read as streams through rust-extensions `BufferedReader`: a local file from the offset, an S3 object as a ranged download. Context filters are exact `key=value` matches and the phrase is a substring of the message, the process or a context value, both ignoring case. Records still waiting in memory are found too.

### `SearchUp` / `SearchDown` gRPC

One request, `SearchLogsRequest`, for both ways: `product_scope`, `from_time`, `to_time` (encoded as in `Read`), `take`, `levels`, `context_keys`, `phrase`. The filters apply together; an empty one does not filter.

- `SearchUp` — the first `take` records from `from_time`, `timestamp` ascending. To read on, the same request with `from_time` after the last record received.
- `SearchDown` — the last `take` records before `to_time`, `timestamp` descending. To read on, the same request with `to_time` before the last record received.

### CTX Search (`Read` gRPC)

Exact `key=value` matches in the context of the record (`Process` counts as a key too), ignoring case. The same as `SearchDown` without a phrase; kept until the UI moves to `SearchUp` / `SearchDown`.

- The UI sends `product_scope`, `context_keys: Vec<{key, value}>`, `levels`, `from_time`, `to_time`, `take`.
- It returns a stream of `LogEventGrpcModel`, `timestamp` descending: the last `take` records of the range.

### Text Search (`ScanAndSearch` gRPC)

A case-insensitive substring of `message`, `process` and the context **values** (not the keys). There is no query syntax: AND/OR/NOT, quotes and prefixes are not parsed, the phrase is searched as it is. The same as `SearchDown` with a phrase alone.

- The UI sends `product_scope`, `phrase`, `from_time`, `to_time`, `take`.
- The result is as in CTX Search.

---

## Storage layout

Logs are kept in plain files, by the hour: no search engine and no external process.

### The files of an hour

| Stream | Levels | Folder | Kept |
|---|---|---|---|
| main | Warning, Error, FatalError | `<logs_db_path>/logs/<scope>/` | on the local disk until 2 hours after the hour ends, then uploaded to S3 and kept there for `hours_to_gc`; without S3, deleted then |
| debug-info | Debug, Info | `<logs_db_path>/debug-info/<scope>/` | deleted 2 hours after the hour ends |

An hour is its data files `YYYYMMDDHH.data00`, `.data01` … `.dataFF`, `YYYYMMDDHH.index` and `YYYYMMDDHH.index-by-key`. A data file takes new frames until it is 3 GiB long, then the next one does — a new frame of an insert into an older file too, so the files never grow past that by more than a frame.

**`.dataNN`** are made of 32 KiB frames, the layout of the working frames of [archived-data](https://github.com/my-jet-tools/archived-data): a header `size u32 · next_file u8 · next u32 · used u16`, then `used` bytes of entries. The frames of all the files of the hour are one chain in time order; `next_file · next` is the next frame — in the same file or another one — none on the last one. An entry is `[varint length][time i64][zstd block]`: the records of one second, each protobuf-encoded behind a varint length (rust-extensions `UInt32VariableSize`), compressed together, at most 8 KiB. A busy second takes several blocks.

**`.index`** is 3600 cells `file u8 · offset u32 · count u32`, one per second of the hour: the data file of the first entry of the second, the entry's offset in it, and how many records the second has. Then one byte: 0 when the cells agree with the data files. A flush sets it to 1 before it writes the frames and back to 0 together with the cells; an index found at 1 is rebuilt by walking the chain.

**`.index-by-key`** — for the level, the process, `Application` and `Version`: how many records of each second have each value (lowercase). A directory first — every key and value, and where its seconds are — then, packed, the seconds each value is in: `varint(step from the second before) · varint(records)`, 2–3 bytes a second. A key with more than 1000 values in an hour is not indexed in it. For the current and the previous hour it is kept in memory whole, as the writes build it, and written at most every 5 seconds, when the hour is sealed and when the process stops; one which does not count the records the index counts is rebuilt from the data. Of an older hour — on the local disk or in S3 alike — only the page of its keys and values is read: one 64 KiB read from its start, its header telling whether the page goes on past it; what the read brought in past the page is kept, so an index shorter than one read is read once. The seconds of a value not brought in yet are read when a filter asks for it — blocks less than 64 KiB apart in one read, always forward — and kept.

### Writing

- Records wait in memory for 5 seconds, grouped by second, so records which come a little late join the block of their second.
- New records for a second which is already written are merged into its last block: the block is decompressed, the records are added, and it is compressed again in its place.
- An insert into the middle never moves the file. A frame with room moves its entries after the place up; a full frame at the end of the chain gets a new frame after it; a full frame elsewhere is halved, and its upper half moves into the next frame or into a new frame appended to the file and chained in between. Only the changed frames are written, and the cells of the moved entries follow them.
- The indexes of the current and the previous hour stay in memory. Other hours are opened when they are read and let go after 10 idle minutes.

### The other files of `logs_db_path`

- `settings.json` — what is set through MCP, see below;
- `statistics.json` — the hourly statistics;
- `one-time-skip.yaml` — the ignore single events.

### Hourly statistics

In memory, `BTreeMap<StatisticsHour, BTreeMap<Scope, BTreeMap<Application, Counters>>>`. Updated synchronously in `flows::post_items` — the scope of a record is found there the same way as for the storage. Persisted to `<logs_db_path>/statistics.json` every 60 seconds by `PersistStatisticsTimer`; `flows::init` restores it at start. A file of the format before product scopes is not read — the statistics start anew. `GetStatistic` returns the counters of the current hour over all scopes and applications.

### settings.json

`<logs_db_path>/settings.json` holds everything set through MCP: the ignore events, the product scopes, the default scope and `hours_to_gc`. The YAML settings hold the infrastructure only: `logs_db_path`, `env_name`, `telegram_settings`, `elastic`, `s3_conn_string`. The file is read at start and written whole on every change. A file of the older format — the array of the ignore events alone — is read as such.

```json
{
  "ignore_events": [{ "level": "Error", "application": "billing", "marker": "*" }],
  "default_product_scope": "default",
  "hours_to_gc": 168,
  "product_scopes": [{ "name": "trading", "applications": ["trading-.*", "quotes"] }]
}
```

`hours_to_gc` is how long the main hours are kept in S3 — a week until it is set. An hour is uploaded 3 hours after it starts, so with 3 or less nothing goes to S3 and the main hours are deleted 2 hours after they end.

---

## Retention

- `GcTimer` (every 30 seconds):
  - deletes the hours of `debug-info/<scope>/` 2 hours after the hour ends;
  - without `s3_conn_string`, deletes the hours of `logs/<scope>/` 2 hours after the hour ends too; with S3 they are uploaded by then — when the upload fails, the local hour is deleted once it is older than `hours_to_gc`;
  - deletes what the storage before this one left (`logs-YYYYMMDDHH/` of Tantivy, `debug|info/logs-*.db` of sqlite) by the same terms;
  - `hourly_statistics.gc()` keeps 48 hours in memory at most.
- `UploadToS3Timer` (every minute, when `s3_conn_string` is set):
  - an hour of `logs/` 2 hours past its end is sealed — nothing is written into it any more — and copied for the upload: the data files into frames which follow one another, each filled before the next one starts and over to the next file after 3 GiB, so a read goes forward from the cell of its second with no jump; the index pointing into them; the key index whole, its page first. They are uploaded as `<prefix>/<scope>/YYYYMMDDHH.data00` …, `.index-by-key`, `.index` — the data files first, the index last; the prefix is `logs` unless `Prefix` of `s3_conn_string` gives one — and deleted locally. Searches read such hours from S3;
  - every 10 minutes deletes the hours older than `hours_to_gc` from S3.
- Records which come for an hour already uploaded or deleted are dropped (a message in the debug console).

---

## Elastic

With `elastic` set, every record which passes the ignore rules is queued and sent to Elastic in batches of up to 1000, into the daily index `services_logs_<env_source>`.

## Telegram

With `telegram_settings` set:

- for every minute with Error or FatalError records, the counts of FatalError, Error and Warning of that minute, with `env_name` and a link to the UI — the `UiUrl` the UI sends in its requests;
- a message when an ignore event is added or deleted.

---

## MCP server

Endpoint: `POST/GET/DELETE /mcp` on the HTTP port, beside REST and swagger. Streamable HTTP transport (JSON-RPC 2.0 + SSE), implemented with `mcp-server-middleware` on top of `my-http-server`; the tools are registered in [src/mcp/builder.rs](../src/mcp/builder.rs).

| Tool | Purpose |
|---|---|
| `search_logs` | Search the records of a scope — see below. |
| `get_dashboard_statistics` | Hourly counters by scope and application, newest hour first. |
| `get_product_scopes` | The default scope and the list, as `GetProductScopes`. |
| `set_product_scope` | `name`, `applications` (the regular expressions): creates the scope last, or replaces the one of the same name where it is. |
| `delete_product_scope` | `name`. |
| `set_default_product_scope` | `name`. |
| `get_hours_to_gc` | `hours_to_gc`, and whether S3 is set up. |
| `set_hours_to_gc` | `hours_to_gc`; lowering it deletes the older hours from S3 for good. |
| `get_ignore_rules` | The ignore rules. |
| `add_ignore_rule` | `level`, `application`, `marker` (`*` matches any), optional `expires_in_minutes`. |
| `delete_ignore_rule` | `level`, `application`, `marker`. |
| `get_memory_stats` | jemalloc statistics and the state that holds memory. |
| `get_debug_console` | The last internal diagnostic messages. |

### `search_logs` — parameters

| Field           | Type       | Purpose                                                                                       |
|-----------------|------------|-----------------------------------------------------------------------------------------------|
| `from_date`     | `string`   | Required. The start of the range, ISO-8601 / RFC-3339 in UTC, inclusive.                      |
| `to_date`       | `string`   | Required. The end of the range, inclusive; after `from_date`.                                 |
| `product_scope` | `string`   | The scope to search; the default scope when it is not given.                                  |
| `application`   | `string`   | The `Application` context value, ignoring case.                                               |
| `version`       | `string`   | The `Version` context value, ignoring case.                                                   |
| `phrase`        | `string`   | A substring of the message, the process or a context value, ignoring case.                    |
| `levels`        | `string[]` | Any of `Info`, `Warning`, `Error`, `FatalError`, `Debug`; all when it is empty or not given.  |
| `take`          | `integer`  | At most this many records; 100 by default, 1..1000.                                           |

### `search_logs` — response

`count`, `truncated` — `take` records were found before `from_date` was reached, so there may be earlier ones: to read on, ask again with `to_date` just before the first record returned — and `items_json`, a JSON array of the latest `take` records of the range, `timestamp` ascending:

```json
{
  "iso_time": "2026-04-25T22:00:00.123456+00:00",
  "timestamp": 1714060800123456,
  "level": "Error",
  "application": "Billing",
  "version": "1.4.2",
  "process": "ingest",
  "message": "...",
  "context": { "TraceId": "..." }
}
```

The tool calls [`flows::search_logs`](../src/flows/search_logs.rs) → [`LogsStorage::search`](../src/logs_storage/logs_storage.rs) going down — the same path as gRPC `SearchDown`.
