# 抖音视频解析（Rust / GPUI）

`rust` 分支只包含 Rust + GPUI 实现及其平台工程。

## 目录

- `rust/crates/core`：链接解析、A-Bogus/SM3 签名、HTTP、下载脚本等核心逻辑。
- `rust/gui/crates/ui`：GPUI 界面与交互。
- `rust/gui/apps/desktop`：Windows / macOS 桌面入口。
- `rust/gui/apps/android`：Android 入口与 Gradle 工程。
- `rust/gui/apps/ios`：iOS 静态库、Swift 壳与 XcodeGen 配置。
- `.github/workflows/build-rust.yml`：Windows、macOS、Android、iOS 构建与发布。

## 本机运行

需要 Rust stable。macOS 还需要 Xcode Command Line Tools。

```bash
cd rust/gui
cargo run --locked -p douyin-parser
```

Release 模式运行：

```bash
cd rust/gui
cargo run --release --locked -p douyin-parser
```

指定界面显示的版本号：

```bash
cd rust/gui
APP_VERSION=1.0.3 cargo run --release --locked -p douyin-parser
```

只构建可执行文件：

```bash
cd rust/gui
cargo build --release --locked -p douyin-parser
```

产物位置：

- macOS / Linux：`rust/gui/target/release/抖音视频解析`
- Windows：`rust/gui/target/release/抖音视频解析.exe`

## 检查

```bash
cd rust
cargo test --locked
cargo fmt --all -- --check

cd gui
cargo check --locked -p douyin-parser
cargo fmt --all -- --check
```

首次编译会下载较大的 GPUI Git 依赖，耗时取决于网络。
