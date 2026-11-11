# gpui-android 本地快照

来源：`pierreaubert/gpui-toolkit`，提交
`8e3a43549c2f8dd6ae43fda875fcf2f2812d1095`。

保留本地快照是因为该版本的 `gpui-android` 尚未适配当前
`sotf-3rdparties` GPUI 的 `Platform` / `PlatformWindow` / `PlatformDisplay`
trait。上游同步升级后应删除此目录，并恢复 git 依赖。

相对上游只做了编译兼容改动：

- 补齐窗口可见性、显示变更和系统睡眠相关回调。
- 补齐 `restart`、`on_quit`、`refresh_interval`、`prevent_idle_sleep`。
- 移除已废弃的 `completed_frame`，并补全 `RequestFrameOptions` 默认字段。
- 将 `FallbackAtlas::get_or_insert_with` 的 key 改为按值传入。

Android Java 源码也放在此目录中，Gradle 会优先使用这里的版本。
