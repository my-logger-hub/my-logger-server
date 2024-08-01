Settings example:

```rust
DefaultTenant: Default
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
