# my-logger-server — gRPC contract

Документ фиксирует контракт между UI и сервером. Любое расхождение в реализации с этим документом — баг.

## Time-range contract

Все методы, принимающие диапазон по времени (`Read`, `ScanAndSearch`), используют единое правило кодирования полей `from_time` (`int64`) и `to_time` (`int64`). UI обязан шифровать одно из трёх состояний; сервер обязан декодировать в той же логике.

### Состояния

| Состояние UI                          | `from_time`                                | `to_time`        | Семантика на сервере                              |
|----------------------------------------|--------------------------------------------|------------------|---------------------------------------------------|
| `HoursAgo(N)`, `N >= 0`                | `-N` (для `N=0` это `0`)                   | `0`              | один шард (один час) = `now + from_time часов`    |
| `ExactHour(date_key)`                  | `date_key` в формате `YYYYMMDDHH` (>= `2024010100`) | `0`     | один шард (один час) по абсолютному ключу         |
| `Range(from_us, to_us)`                | unix-микросекунды (>0)                     | unix-микросекунды (>0) | диапазон шардов `[from_us..to_us]`           |

### Алгоритм декодирования (сервер)

```
if to_time == 0:
    if from_time <= 0:
        # «смещение в часах от текущего времени»
        date_key = DateHourKey(now + from_time hours)
    else:
        # «абсолютный date_key YYYYMMDDHH»
        date_key = DateHourKey(from_time)
    → читать ОДИН шард
else:
    # «диапазон в unix-микросекундах»
    → читать ВСЕ шарды, чьи часы попадают в [from_time .. to_time]
```

### Реализация на стороне UI

[`models/time_range.rs::TimeRange::get_date_from_date_to`](https://github.com/MyJetTools/MyLogger/blob/main/my-logger-ui/src/models/time_range.rs):

```rust
match self {
    HoursAgo(n)         => (-(n as i64), 0),
    Range(from, to)     => (from.unix_us, to.unix_us),
    ExactHour(date_key) => (date_key.value, 0),
}
```

### Реализация на стороне сервера

[`grpc_server/my_logger_grpc_service.rs`](../src/grpc_server/my_logger_grpc_service.rs) — оба метода `read` и `scan_and_search` зовут единую функцию `RequestType::from_request(from_time, to_time)`. Логика парсинга существует ровно в одном месте:

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

### Запрещено

- Положительные `from_time < 2024010100` при `to_time == 0` — UI такие не присылает; если придёт, сервер интерпретирует как невалидный date_key и вернёт пустой результат.
- `to_time < 0` — не поддерживается.
- `from_time > to_time` при range-режиме — поведение не определено (вернётся пустой результат).

---

## Search modes

UI имеет два режима поиска, переключаемых через `SearchType`:

### CTX Search (`Read` gRPC)

Точное совпадение пар `key=value` в контексте лога. Регистронезависимо.

- UI шлёт `context_keys: Vec<{key, value}>`, `levels`, `from_time`, `to_time`, `take`.
- Сервер строит `BooleanQuery` из `TermQuery(ctx="key=value")` (lowercase) для каждой пары + `RangeQuery(timestamp)` + `TermQuery(level)` если задан.
- Возвращает поток `LogEventGrpcModel`, отсортированных по `timestamp DESC`.

### Text Search (`ScanAndSearch` gRPC)

Полнотекстовый поиск с токенизацией. Регистронезависимо. Поддерживает:
- одиночные слова: `timeout`
- AND/OR/NOT: `error AND db`, `(timeout OR refused) AND NOT retry`
- фразы: `"connection refused"` (с кавычками)
- префиксы: `connect*`

Поле поиска — `text_search`, в которое на запись складываются: `message`, `process`, и все **значения** контекста (без ключей).

- UI шлёт `phrase`, `from_time`, `to_time`, `take`.
- Сервер парсит `phrase` через Tantivy `QueryParser` по полю `text_search`.

---

## Storage layout

### Логи

Tantivy. Один индекс на час, путь `<LogsDbPath>/logs-YYYYMMDDHH/`. Schema:

| Поле          | Индекс                                  | Назначение                                  |
|---------------|------------------------------------------|---------------------------------------------|
| `timestamp`   | `INDEXED + FAST + STORED` (i64)          | range-фильтр + сортировка DESC              |
| `id`          | `STORED`                                 | возврат                                     |
| `level`       | `STRING raw, INDEXED + STORED`           | `TermQuery` по уровню                       |
| `message`     | `STORED`                                 | только возврат, не индексируется            |
| `ctx`         | `STRING raw, INDEXED only` (multi-value) | `TermQuery` по парам `application=billing`  |
| `ctx_data`    | `STORED`                                 | возврат JSON оригинального контекста        |
| `text_search` | `TEXT default+positions, INDEXED only`   | полнотекст: message + process + значения ctx |

Контекст индексируется в нижнем регистре для поиска, возвращается в исходном (через `ctx_data`).

### Hourly statistics

In-memory `BTreeMap<StatisticsHour, BTreeMap<Application, Counters>>`. Обновляется синхронно из `flows::post_items::update`. Персистится в `<LogsDbPath>/statistics.json` каждые 60 секунд таймером `PersistStatisticsTimer`. На старте `flows::init` восстанавливает структуру из этого файла.

### Ignore events

JSON-файл `<LogsDbPath>/settings.json`. Перезаписывается полностью при каждом изменении (add/delete). Читается на старте.

---

## Retention

- `GcTimer` (раз в 30 секунд):
  - удаляет шарды старее `hours_to_gc` (из настроек) — `tokio::fs::remove_dir_all`
  - срезает уровни `Debug`/`Info` старее 60 минут и `Warning` старее 6 часов в последних двух шардах через `IndexWriter::delete_query`
  - `hourly_statistics.gc()` оставляет максимум 48 часов в памяти

---

## MCP server

Endpoint: `POST/GET/DELETE /mcp` (тот же HTTP-порт, что и REST/swagger). Streamable HTTP transport (JSON-RPC 2.0 + SSE), реализован через `mcp-server-middleware` поверх `my-http-server`. Регистрация — в [`src/http/start_up.rs`](../src/http/start_up.rs).

Сейчас зарегистрирован один tool: **`search_logs`** ([src/mcp/search_logs_tool_call.rs](../src/mcp/search_logs_tool_call.rs)).

### `search_logs` — параметры

Все опциональны кроме описанной комбинации `last_minutes` или `(from_time, to_time)`.

| Поле              | Тип             | Назначение                                                                 |
|-------------------|-----------------|----------------------------------------------------------------------------|
| `phrase`          | `string`        | Полнотекстовый поиск. Пустое или отсутствует — без фразы.                  |
| `last_minutes`    | `integer`       | Если > 0 — диапазон `[now - N min .. now]`. Перебивает `from_time`/`to_time`. |
| `from_time`       | `integer` (us)  | Используется только если `last_minutes` отсутствует/0. Парный с `to_time`. |
| `to_time`         | `integer` (us)  | Парный с `from_time`. Должно быть `from_time < to_time`.                   |
| `levels`          | `string[]`      | Допустимы: `info`, `warning`, `error`, `fatal_error`, `debug`. Регистронезависимо. |
| `context_filters` | `string[]`      | Каждая запись `key=value`. Регистронезависимое точное совпадение.          |
| `take`            | `integer`       | Максимум записей. Default 100, диапазон 1..1000.                           |

### `search_logs` — ответ

Поле `items_json` — строка с JSON-массивом найденных записей, отсортированных `timestamp DESC`. Каждый элемент:

```json
{
  "timestamp": 1714060800123456,
  "iso_time": "2026-04-25T22:00:00.123456+00:00",
  "level": "Error",
  "message": "...",
  "context": { "Application": "Billing", "Version": "1.4.2", "Process": "ingest" }
}
```

### Резолв временного интервала (одно правило для AI)

```
if last_minutes > 0:
    from = now - last_minutes minutes
    to   = now
elif from_time > 0 and to_time > 0:
    use as-is
else:
    error
```

### Под капотом

Tool вызывает [`LogsRepo::search`](../src/repo/logs_repo.rs) — единая функция, объединяющая `RangeQuery` по `timestamp`, `TermQuery` по `level` и `ctx`, `QueryParser` по `text_search`, top-N с `order_by_fast_field` по `timestamp DESC`. Это тот же путь, что и gRPC `Read`/`ScanAndSearch`, только с обоими наборами фильтров одновременно.
