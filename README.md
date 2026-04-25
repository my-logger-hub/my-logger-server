# my-logger-server

Лог-сервер на Rust. Принимает логи по gRPC/HTTP, хранит локально и (опционально) форвардит в Elastic и Telegram.

## Хранение логов

Логи хранятся в [Tantivy](https://github.com/quickwit-oss/tantivy) — встроенном поисковом движке на чистом Rust. Никакого внешнего процесса (SQLite/Elastic/JVM) для хранения логов не требуется.

### Шардинг по часу

Каждый час получает свой Tantivy-индекс — отдельная папка вида `logs-YYYYMMDDHH` внутри `LogsDbPath`. Это даёт:

- **Pruning по времени**: запрос с интервалом `[from..to]` физически открывает только нужные часовые индексы.
- **Простой retention**: GC удаляет папку целиком (`remove_dir_all`), без `DELETE` и vacuum.

### Схема индекса

| Поле         | Тип                          | Назначение                                                  |
|--------------|------------------------------|-------------------------------------------------------------|
| `timestamp`  | `i64` INDEXED + FAST + STORED | range-фильтр и сортировка DESC по времени                  |
| `id`         | STORED                       | идентификатор записи                                        |
| `level`      | STRING (raw, lowercase)      | точное совпадение по уровню                                 |
| `message`    | TEXT (default + lowercase)   | полнотекстовый поиск со стеммингом                          |
| `ctx`        | STRING multi-value (raw)     | каждая пара контекста индексируется как `key=value` lowercase |
| `ctx_data`   | STORED                       | JSON оригинального контекста (с исходным регистром)         |

Контекст индексируется в нижнем регистре для регистронезависимого поиска, но возвращается клиенту в исходном виде (через `ctx_data`).

### Сценарии поиска

Все поиски ограничены интервалом времени. Пример комбинированного запроса:

```
timestamp:[from..to]
  AND level:error
  AND ctx:"application=billing"
  AND ctx:"version=1.4.2"
  AND message:timeout
```

Возвращает топ-N результатов, отсортированных по `timestamp DESC` через FAST-поле — десятки миллисекунд на миллионах записей.

### GC

- `gc_files` — удаляет шарды старше `hours_to_gc`.
- `gc_level` — внутри последних двух шардов точечно удаляет логи уровней Debug/Info (старше 60 минут) и Warning (старше 6 часов) через `IndexWriter::delete_query`.

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

`LogsDbPath` — корневая папка, в которой создаются часовые подпапки-индексы и сопровождающие SQLite-файлы (`settings.db`, `hour_statistics.db`).
