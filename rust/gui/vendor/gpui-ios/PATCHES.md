# gpui-ios 本地快照

来源：`pierreaubert/gpui-toolkit`，提交
`8e3a43549c2f8dd6ae43fda875fcf2f2812d1095`。

上游该提交只存在于 Cargo 的 git checkout 里，CI（GitHub Actions）拉取的
是干净的上游代码，因此本地对 `gpui-ios` 的修复必须随仓库一起提交，否则
发布包会缺少这些行为。上游合入后应删除此目录并恢复 git 依赖。

本地相对上游的改动：

## 触摸 / hover

- 点按、拖动、滚动结束后 150ms 通过 GCD 派发一次
  `MouseMove(-10000, -10000)` 并请求重绘，清除 `hover` 样式，修复
  iOS 上点完按钮后 hover/click 状态卡住、必须点别处才能恢复的问题。
- 前后台切换、窗口激活状态变化时同样重置 hover。
- 补一个 500ms 的 GCD 长按识别器：上游 Tap/Scroll 状态机不发送
  `PlatformInput::Touch`，GPUI 的长按识别器在 iOS 上收不到原始触摸；
  长按被文本元素认领后，移动改为扩展选区而不是滚动。

## 文本输入

- 新增原生编辑器接管 `NativeEditorParams` / `show_native_editor` /
  `hide_native_editor` / `native_editor_text`，让系统 `UITextView`
  负责键盘、光标、选区手柄和编辑菜单。
- `register.rs` 中 `insertText` / `deleteBackward` / `setMarkedText` /
  `unmarkText` 在接管期间直接回读原生编辑器，避免按键被重复应用。
- 键盘 `becomeFirstResponder` 失败时延迟重试。