# 短视频解析（Rust / GPUI）

`rust` 分支包含 Rust 核心、GPUI 桌面端与 Tauri Android 端及其平台工程。

## 目录

- `rust/crates/core`：链接解析、A-Bogus/SM3 签名、HTTP、下载脚本等核心逻辑。
- `rust/gui/crates/ui`：GPUI 界面与交互。
- `rust/gui/apps/desktop`：Windows / macOS 桌面入口。
- `rust/gui/apps/android`：Android Tauri/Vue 入口、前端工程与 `gen/android` Gradle 工程。
- `rust/gui/apps/ios`：iOS 静态库、Swift 壳与 XcodeGen 配置。
- `.github/workflows/build-rust.yml`：Windows、macOS、Android、iOS 构建与发布。

## 本机运行

需要 Rust stable。macOS 还需要 Xcode Command Line Tools。

```bash
cd rust/gui
cargo run --locked -p reels
```

Release 模式运行：

```bash
cd rust/gui
cargo run --release --locked -p reels
```

指定界面显示的版本号：

```bash
cd rust/gui
APP_VERSION=1.0.3 cargo run --release --locked -p reels
```

只构建可执行文件：

```bash
cd rust/gui
cargo build --release --locked -p reels
```

产物位置：

- macOS / Linux：`rust/gui/target/release/短视频解析`
- Windows：`rust/gui/target/release/短视频解析.exe`

## 检查

```bash
cd rust
cargo test --locked
cargo fmt --all -- --check

cd gui
cargo check --locked -p reels
cargo fmt --all -- --check
```

首次编译会下载较大的 GPUI Git 依赖，耗时取决于网络。

## Android 构建

需要 Android SDK、NDK 与 Java 17。先构建 Vue 前端，再交给 Tauri 构建 APK：

```bash
cd rust/gui/apps/android/frontend
npm ci
npm run build

cd ..
export TAURI_CLI_JS="$PWD/frontend/node_modules/@tauri-apps/cli/tauri.js"
node "$TAURI_CLI_JS" android build --apk --ci --target aarch64
```

## Android Release 签名

CI 发布 `tauri android build --apk` 时从以下 GitHub Actions Secrets 读取签名材料：

- `ANDROID_KEYSTORE_BASE64`：PKCS#12 keystore 的 Base64 内容。
- `ANDROID_KEYSTORE_PASSWORD`：keystore 密码。
- `ANDROID_KEY_ALIAS`：key alias。
- `ANDROID_KEY_PASSWORD`：key 密码。

本地签名使用 `rust/gui/apps/android/gen/android/keystore.properties`，该文件和
`.secrets/` 下的密钥材料均已被 Git 忽略。Base64 可用以下命令生成：

```bash
base64 < reels-release.jks | tr -d '\n'
```

PR 构建没有 Secrets 时会回退到 debug 签名；push、tag 和手动触发缺少 Secrets
会直接失败，避免误发布 debug 签名的 APK。
