# Todo List: UI Shell Correction

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 一层桌面壳样式 | `.app-shell` 贴满 Tauri 窗口，无内层圆角外框和 12px 二次窗口感 | 验证成功 |
| quota 状态表达 | UI 明确区分 real / estimate / unavailable，不用 N/A 让用户猜状态，smoke 覆盖 | 验证成功 |
| Provider 删除安全 | Delete 使用 danger 样式并要求确认，避免与 Test/Attach 同级误触 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
