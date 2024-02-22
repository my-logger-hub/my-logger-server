Settings example:

```rust
DefaultTenant: Default
LogsDbPath: /root/db
IgnoreEvents:
- level: Info
  application: app-name
  marker: TextEntry
TelegramSettings:
- api_key: string
  chat_id: string
  message_thread_id: number
  env_info: string
```
