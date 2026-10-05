# RapidRAW 鸿蒙(HarmonyOS / OpenHarmony)移植报告

> 分析日期:2026-10 · 分支:`feature/harmonyos-port`
> 状态:Phase 0–2 全部完成(窗口控制 6.5 / FileKit 导入导出 6.6-6.7 / 系统 UI 适配 6.8),Phase 3 模拟器侧完成(编辑器 GPU 管线点亮 6.8、直渲评估 6.9、高像素基线与缓解评估 6.10/6.11),Phase 4 前置打通(AI 降噪单模型全通 6.12、HAP 签名 2026-10-05);当前阻塞:无真机硬件;全部待办与优先级见 §9 盘点

## 1. 结论(TL;DR)

**可行,推荐进行**。综合评级:**中高可行性,MVP 工作量 1.5~3 人月**。

RapidRAW 的架构对鸿蒙移植异常友好——它已经完成了最贵的两件事:

1. **移动端适配**(Android 已发布):触摸 UI、URI 文件访问、移动布局、移动端默认值均已存在;
2. **GPU 无关的回退渲染路径**(`gpu_processing.rs` 中 Android/Linux 走 compute-only + GPU 回读 + IPC 传位图给 Canvas 2D),该路径在 OHOS 上可直接复用。

推荐主路线为 **Tauri-ohos 社区分支(策略 A)**,备选 **ArkWeb 宿主 + NAPI(策略 B)**,不建议 ArkUI 全重写(策略 C)。

## 2. RapidRAW 架构盘点(与移植相关的关键事实)

| 维度 | 事实 | 移植含义 |
|---|---|---|
| 前端 | React 19 + TS,~50K 行,Konva/Canvas 2D 渲染,**无任何 WebGL/WebGPU 依赖** | ArkWeb(Chromium M114/M132)可直接运行,无需改造 |
| 后端 | Rust ~33K 行,94 个 Tauri IPC 命令 | 命令层是纯 Rust,Tauri-ohos 分支已实现 IPC |
| GPU | 桌面端 wgpu 直接渲染窗口 surface;Android/Linux 已走 compute-only + 回读 + IPC 传位图路径 | OHOS 直接复用回读路径;wgpu GLES 后端已进上游 |
| 移动适配 | 84 处 `target_os = "android"` 分支;触摸 UI、URI 文件访问、移动布局均已存在 | OHOS 集成层可参照 `android_integration.rs`(21K 行)的成熟模式编写 |
| RAW 解码 | rawler 为**纯 Rust**,无原生依赖 | 直接交叉编译,零改动 |
| AI 推理 | ONNX Runtime(`load-dynamic` 特性,运行时从 HuggingFace 下载模型:SAM/U2Net/DepthAnything/CLIP/LaMa) | 动态加载模式恰好规避了交叉编译问题,只需一个 OHOS 版 `libonnxruntime.so` |
| 镜头校正 | Lensfun 数据库 + quick-xml 解析(纯 Rust) | 数据文件随 HAP 打包即可 |
| C 依赖 | `mozjpeg-rs`、`webp`、`onig`(tokenizers) | Android 构建已在编译这三个库,OHOS NDK 同样可行 |
| i18n | 已内置简体中文 | 无需额外工作 |

## 3. 鸿蒙生态现状(2026-10)

| 组件 | 状态 | 证据 |
|---|---|---|
| Rust OHOS 目标 | ✅ **Tier 2 with Host Tools**(`aarch64`/`armv7`/`x86_64-unknown-linux-ohos`,Rust 1.87 起) | [rust#137011](https://github.com/rust-lang/rust/pull/137011)、[官方平台支持文档](https://doc.rust-lang.org/stable/rustc/platform-support/openharmony.html) |
| Tauri 2 on OHOS | ✅ 社区移植已合入官方 `feat/open-harmony` 分支(wry#1607、tao#1128、tauri#15064),Eclipse Oniro 背书,2026-06 仍活跃 | [tauri 分支](https://github.com/tauri-apps/tauri/tree/feat/open-harmony)、[richerfu/tauri-demo](https://github.com/richerfu/tauri-demo) |
| wgpu on OHOS | ✅ GLES 后端已进上游(PR #7085,2025-02,含 CI);**Vulkan 真机(Maleoon 916)已验证可用**(ash fork 以仓内 vendor 补丁落地,见 6.13;仅 aarch64/x86_64) | [wgpu#7085](https://github.com/gfx-rs/wgpu/pull/7085) |
| raw-window-handle | ✅ `OhosNdkWindowHandle` 官方支持 | 上游 `src/ohos.rs` |
| napi-ohos(Rust↔ArkTS) | ✅ 活跃维护,262K+ 下载,Tauri-ohos 底层同款 | [ohos-rs/ohos-rs](https://github.com/ohos-rs/ohos-rs) |
| ONNX Runtime | ⚠️ 无微软官方 OHOS 构建;社区预编译 v1.16.3(sherpa-onnx / csukuangfj),或走官方 NNRt/MindSpore Lite | [onnxruntime#20895](https://github.com/microsoft/onnxruntime/issues/20895) |
| ArkWeb WebGPU | ❌ 不支持,无时间表(WebGL1/2 可用) | ArkWeb 官方 Release Note |

**对 RapidRAW 的关键利好**:前端不用 WebGPU(纯 Canvas 2D),ArkWeb 缺 WebGPU 这一最常见拦路虎对本项目**不构成障碍**。

### 关键技术事实:OHOS 的 Rust cfg 判定

`*-unknown-linux-ohos` 目标满足 **`target_os = "linux"` 且 `target_env = "ohos"`**(与 Servo/makepad 等项目实践一致)。由此:

- 现有代码中所有裸 `target_os = "linux"` / `any(windows, linux)` 门控会**错误匹配 OHOS**,把 gtk/webkit2gtk/-trash/wayland-quirk/单实例插件等桌面专属依赖拉进 OHOS 构建 → **编译失败**;
- `any(android, linux)` 门控(gpu_processing 的 compute-only 路径等)恰好把 OHOS 引向正确的移动端行为。

本分支已按此规则完成全量门控修正(见 §6)。

## 4. 三条移植路线对比

| | A. Tauri-ohos 分支(推荐) | B. ArkWeb 宿主 + NAPI(备选) | C. ArkUI 全重写(不推荐) |
|---|---|---|---|
| 复用率 | 前后端 ~95%,含 IPC/插件体系 | 前端 100%,后端命令层需重新桥接 | 仅算法核心可复用 |
| 工作量 | **1.5~2 人月** | 2.5~3 人月 | 6+ 人月 |
| 主要风险 | 依赖未合入主干的 fork 分支 | 需自行重建 94 个命令的桥接 + 事件系统 | 丢弃全部 React/移动适配投入 |
| 适用场景 | MVP 首选 | 若 fork 停滞的退出路线 | 需要深度鸿蒙分布式特性时(本产品不需要) |

## 5. 风险清单(按严重度)

1. **Fork 依赖(高)**:`feat/open-harmony` 未合入 Tauri 主干,上游表示要等 winit 集成进展。缓解:锁 commit + B 路线保底。
2. **C 依赖交叉编译(中)**:`onig`/`mozjpeg`/`webp` 需接 OHOS NDK 工具链(`CC_aarch64_unknown_linux_ohos` 等);Android 已编译过这三个库,风险可控。
3. **ORT 版本(中)**:社区预编译停在 1.16.3,MLAS 的 bf16/fp16 快速路径被禁用,ARM 推理偏慢。缓解:AI 功能是可选增强路径;追求 NPU 需走 MindSpore Lite 二线。
4. **文件访问沙箱(中)**:用户文件须走 FileKit/photoAccessHelper(类似 Android SAF),需编写 `ohos_integration.rs` 对应层(Phase 2)。
5. **Tauri 插件空缺(中低)**:dialog/fs/os/process/shell 插件无 OHOS 实现,仅有 demo 先例。
6. **Windows 开发机链接器(低)**:OHOS NDK clang 是 Unix 脚本,Windows 主机构建需 `.cmd` 包装;建议 Linux/macOS 出包。
7. **手机端性能(低,已验证)**:Android 已趟过高像素 RAW 的回读路径;**真机已验证(6.13,2026-10-05)**——Maleoon GLES 转译层读回必崩(驱动缺陷,`__stack_chk_fail`),Vulkan 后端交互预览 ~250-290ms/趟、缩略图 ~500ms;首趟 WGSL 管线编译 ~11s 已由启动期后台预热吸收(6.13 遗留清单)。
8. **mimalloc(低)**:已在 OHOS 目标回退系统分配器(本分支已处理),待真机验证后再评估启用。

## 6. 本分支已完成的脚手架(Phase 0)

**cfg 约定**(本分支引入,后续 OHOS 相关代码请遵循):

- OHOS 平台判定:`target_env = "ohos"`
- "移动端"(Android + OHOS):`any(target_os = "android", target_env = "ohos")`
- "桌面 Linux"(排除 OHOS):`all(target_os = "linux", not(target_env = "ohos"))`
- "桌面三平台"(排除 OHOS):`any(target_os = "windows", target_os = "macos", all(target_os = "linux", not(target_env = "ohos"))))`

**代码变更清单**:

| 文件 | 变更 |
|---|---|
| `src-tauri/Cargo.toml` | 桌面三平台段(trash/single-instance/reqwest)、gphoto2 段、linux 段(gtk/webkit2gtk/wayland-quirk)全部排除 OHOS;新增 `[target.'cfg(target_env = "ohos")']` 段(reqwest);mimalloc 移入 `not(ohos)` 段 |
| `src-tauri/build.rs` | 新增 OHOS 分支:不自动下载 ORT(无官方 OHOS 构建产物),校验开发者自备的 `src-tauri/libs/ohos/arm64-v8a/libonnxruntime.so`,缺失时告警但不断构建(AI 功能运行时降级) |
| `src-tauri/src/ohos_integration.rs` | 新建模块骨架:`initialize_ohos()` 入口 + Phase 2 待办清单(文件桥/证书校验/相册导出) |
| `src-tauri/src/lib.rs` | mimalloc cfg 扩展;`mod ohos_integration`;window-state/wayland/单实例/monitor 等桌面逻辑排除 OHOS;OHOS 走移动端路径(`initialize_ohos` 调用、跳过窗口状态恢复);`ORT_DYLIB_PATH` 在 OHOS 指向 HAP 内裸 soname |
| `src-tauri/src/app_settings.rs` | 5 对移动端默认值(preview 分辨率/缩略图/zoom 倍率/worker 线程/缓存)扩展到 OHOS |
| `src-tauri/src/file_management.rs` | 4 处 trash 删除门控排除 OHOS(移动端走永久删除,同 Android);`xdg-open` 分支排除 OHOS;新增 OHOS "show in file manager" 提示分支 |
| `src-tauri/src/window_customizer.rs` | gtk 缩放手势禁用分支排除 OHOS |

**不变性保证**:以上所有 cfg 修改对既有平台(Windows/macOS/Linux/Android)在语义上完全等价——非 OHOS 目标上 `not(target_env = "ohos")` 恒为真,各 cfg 表达式取值与修改前一致。

**验证状态**:宿主平台(Windows MSVC)`cargo check` 已通过(Rust 1.99.0 + CMake 4.4.3,含全部依赖链与 build.rs ORT 下载校验),本分支 cfg 修改对既有平台无回归。OHOS 目标交叉验证已于 2026-10-03 完成:核心依赖探针工程 `cargo check --target aarch64-unknown-linux-ohos` 全量通过;主仓完整 OHOS 检查被上游 tauri 的 Linux 桌面栈阻断(预期边界,Phase 1 处理),详见 6.1;该阻断已于 Phase 1 解除,见 6.2。

### 6.1 OHOS 交叉编译验证详情(2026-10-03,Phase 0 收官)

**探针工程**(临时工程,依赖镜像主仓核心栈、排除 tauri 应用层):wgpu、ort、rawler、reqwest(rustls + aws-lc-sys)、tokenizers、mozjpeg-rs、webp、image、jxl-oxide、jxl-encoder 等,`cargo check --target aarch64-unknown-linux-ohos` **全量通过**(Rust 1.98,DevEco Studio API 26 SDK,Windows 主机)。

**三项关键配方**(均已实证):

1. **aws-lc-sys**:cmake-rs 无 OHOS 内建支持,默认回退 MSVC 生成器必失败。设置 `CMAKE_TOOLCHAIN_FILE=<仓库>/src-tauri/ohos/ohos-toolchain.cmake`(已提交入库,经 `OHOS_NDK_HOME` 环境变量参数化;CMakeCache 已实证记录该文件、`OHOS_ARCH=arm64-v8a` 与 Ninja 生成器)+ `CMAKE_GENERATOR=Ninja`(CMake 与 Ninja 需在 PATH)。
2. **ort-sys**:上游无 OHOS 预编译产物。设置 `ORT_SKIP_DOWNLOAD=1` + `ORT_DYLIB_PATH=libonnxruntime.so`(裸 soname;Phase 1 自备 so 放入 `src-tauri/libs/ohos/arm64-v8a/` 并打包进 HAP)。
3. **archmage 版本错配隐患(jxl-encoder 依赖链)**:`archmage-macros` 必须与 `archmage` 严格配套。全新依赖解析会拉到 archmage-macros 0.9.29,其 `#[arcane]` 宏展开生成 `__ARCHMAGE_ASSERT_TIER_*` 断言调用,而 archmage ≤0.9.28 预生成的 arm.rs 缺这些常量,导致**所有 aarch64 目标(含 Android)编译失败**。主仓受 Cargo.lock 冻结的自洽三件套(archmage 0.9.28 + archmage-macros 0.9.28 + magetypes 0.9.26)保护;`cargo update` 若触及 archmage 三件套,须整体核对 aarch64 可编译性。

**注意**:cargo `[env]` 为全局注入,上述变量**不可**写入 `.cargo/config.toml`(会污染宿主构建——宿主 ort-sys 依赖 build.rs 下载 onnxruntime.dll)。交叉构建时在 shell 中设置;本地 `src-tauri/.cargo/config.toml`(gitignored)仅含链接器与 CC/CXX/AR 包装器变量(按目标三元组命名,天然不影响宿主构建)。

**交叉检查命令(PowerShell,Windows 主机已验证)**:

```powershell
$env:Path = "C:\Program Files\CMake\bin;<VS 2022 自带 Ninja 路径>;$env:USERPROFILE\.cargo\bin;" + $env:Path
$env:OHOS_NDK_HOME = 'C:\Program Files\Huawei\DevEco Studio\sdk\default\openharmony\native'
$env:CMAKE_TOOLCHAIN_FILE = '<仓库路径>\src-tauri\ohos\ohos-toolchain.cmake'
$env:CMAKE_GENERATOR = 'Ninja'
$env:ORT_SKIP_DOWNLOAD = '1'
$env:ORT_DYLIB_PATH = 'libonnxruntime.so'
cargo check --target aarch64-unknown-linux-ohos
```

**主仓边界**:主仓完整 OHOS 检查仍被上游 tauri 的 Linux 桌面栈阻断——OHOS 目标满足 `target_os = "linux"`,上游 tauri 2.12 无条件拉入 webkit2gtk / gtk / muda / zbus(libdbus-sys 依赖 pkg-config,OHOS 无此栈)。此为路线图预期的 fork 边界,Phase 1 经 `[patch.crates-io]` 指向 tauri `feat/open-harmony` 分支解决;应用层 cfg 门控已全部生效(Phase 1 已落地,见 6.2)。

### 6.2 Phase 1:fork 补丁接入与主仓双目标验证(2026-10-03)

**成果**:主仓 `cargo check --target aarch64-unknown-linux-ohos` **全量通过**(约 800 个依赖 + 应用本体),宿主(Windows MSVC)`cargo check` 无回归。Phase 0 遗留的上游 tauri 桌面栈阻断正式解除。

**变更清单**:

| 文件 | 变更 |
|---|---|
| `src-tauri/Cargo.toml` | `tauri = "2.11"`(fork 基线;上游 2.12 无 OHOS 支持);`[patch.crates-io]` 8 条 → tauri-apps/tauri `feat/open-harmony`(e3bf6eb1:tauri 2.11.5、tauri-build、tauri-runtime、tauri-runtime-wry、tauri-utils、tauri-macros)、wry(6aaf4b84,v0.56.0)、tao(813572fb,v0.36.0);`[patch."https://github.com/harmony-contrib/openharmony-ability.git"]` 指向本地 vendor;tauri-plugin-dialog 移入 `not(target_env = "ohos")` 段;ohos 段新增 napi-ohos 1.2(`napi8`)+ napi-derive-ohos 1.2 |
| `src-tauri/vendor/openharmony-ability/` | **入库 vendored 固定版**(rev 295a276a,v0.3.0,`webview` feature 完好)。原因有二:① 上游 master 已迭代到 1.0.0-beta.2 并把 webview 拆入独立插件 crate,wry fork 仍依赖 0.3 的 `features = ["webview"]`;② cargo 不允许 patch 指回同一 git 源("patches must point to different sources"),无法仅以 rev 区分 |
| `src-tauri/src/lib.rs` | dialog 插件注册移入 `#[cfg(not(target_env = "ohos"))]` 块 |
| `src-tauri/capabilities/default.json` | 移除 `dialog:default` |
| `src-tauri/capabilities/dialog.json` | 新建,平台作用域 `["windows", "linux", "macOS", "android", "iOS"]`(fork Target 的 serde 命名为 camelCase,`openHarmony` 亦然) |

**代价说明**:补丁直接提交进主清单(而非 CI 专用清单),意味着**全平台统一骑在 fork 上**——桌面 tauri 由 2.12.1 降至 fork 2.11.5,并连带 muda 0.19.3、tray-icon 0.24.2、webview2-com 0.38.2、window-vibrancy 0.6.0、dirs 6、keyboard-types 0.7.0、brotli 8 等降级(宿主 cargo check 已验证无碍)。fork 合入上游后摘除 `[patch]` 段即可回到官方版本。

**两个非显然的坑**:

1. **rfd→gtk 泄漏链**:tauri-plugin-dialog → rfd 0.16 → gtk-sys(rfd 仅以 `target_os = "linux"` 门控 GTK,不排除 ohos)。OHOS 必须三处一致关闭 dialog:依赖段、lib.rs、capability。
2. **napi 裸路径**:`#[cfg_attr(mobile, tauri::mobile_entry_point)]` 在 OHOS 经 openharmony-ability `#[ability]` 派生展开,向应用 crate 发射 `::napi_ohos` / `#[napi_derive_ohos::napi]` 裸路径;`tauri::ohos` 不再导出 napi 系 crate,应用须自行声明上述两个依赖。

**Cargo.lock 风险登记(6.1 archmage 条目之外新增)**:

- **gpu-allocator→windows 边**:wgpu-hal 29.0.4 严格要求 `windows = "0.62"`(→0.62.2);gpu-allocator 0.28.0 要求 `">=0.53, <=0.62"`,cargo 把 `<=0.62` 补零为 `<=0.62.0`,**0.62.2 落在范围外**——任何重解析都只会选到 0.61.3,导致宿主 wgpu-hal D3D12 类型失配(E0308/E0277)。本仓锁已手工锚定 `windows 0.62.2`(历史可编译态,cargo 校验容忍)。**禁止无差别 `cargo update`**;定向更新用包名列表(如 `cargo update tauri tauri-build tauri-runtime tauri-runtime-wry tauri-utils tauri-macros wry tao`),更新后核对:① archmage 三件套(6.1)② gpu-allocator 的 windows 边仍为 0.62.2。

**验证记录**(Rust 1.98.1,Windows 主机):OHOS 全量检查 EXIT=0(增量复验 12~21s,仅 9 条 OHOS 门控死代码警告);宿主检查 EXIT=0(23.26s);OHOS 依赖树零 gtk/rfd 泄漏;archmage 三件套 0.9.28/0.9.28/0.9.26 未漂移。工具:cargo-tauri v2.11.4(fork 版,含 `ohos` 子命令)、ohrs v1.5.0;`rust-toolchain.toml` 位于 `src-tauri/`(非仓库根)。

### 6.3 Phase 1:HAP 出包攻坚(Windows 主机,2026-10-03)

**成果**:Rust 交叉编译全链路(vite 前端 → ohrs Rust 构建 → ohpm install)打通,`librapidraw_lib.so`(dev profile,399MB)落位 `gen/ohos/entry/libs/arm64-v8a/`。~~当前唯一阻断:hvigor 打包~~ **hvigor 打包已于 2026-10-03 晚打通,见下。**

**OHOS_HOME 契约**:cargo-mobile2 fork 的 `env.rs` 把 `OHOS_NDK_HOME = OHOS_HOME` 原样下发给子进程;fork 的构建把 Rust 交叉编译委托给 `ohrs build`,而 ohrs 以 `<OHOS_NDK_HOME>/native/llvm` 推导链接器/工具、`<OHOS_NDK_HOME>/native/sysroot` 推导 sysroot。故 **`OHOS_HOME` 必须是 SDK 根(`.../sdk/default/openharmony`),不是 native 目录**(两种指法曾分别导致链接器路径双 `native` 与 aws-lc-sys 找不到 clang)。`ohos-toolchain.cmake` 已改为容错两种 `OHOS_NDK_HOME` 约定(检测 `llvm/` 位置归一到 NDK 根),与 6.1 手工交叉检查共用一份工具链文件;ohrs 检测到 `CMAKE_TOOLCHAIN_FILE`/`CMAKE_GENERATOR` 已设置时会尊重现有值。

**空格陷阱**:ohrs 在 `<OHOS_NDK_HOME>/../hms` 存在时向 CFLAGS 注入 HMS include。DevEco 装于 `C:\Program Files\...`,cc-rs 按空白切分 CFLAGS,带空格的 include 被切成残参,ring/aws-lc-sys 全灭(clang: no such file or directory)。修复:为 SDK 建无空格 junction 别名(`$env:USERPROFILE\ohos-devstudio\sdk` → `$dev\sdk`),`OHOS_HOME` 指向 junction 路径,HMS include 变为无害单 token。

**npm 对齐**:tauri CLI 版本门禁要求 Rust tauri(2.11.5)与 `@tauri-apps/api` 同 major.minor,`package.json` 固定 `"@tauri-apps/api": "2.11"`(实装 2.11.1)。

**构建姿势**:`cargo tauri ohos build` 必须在**仓库根**执行(beforeBuildCommand 在 CLI cwd 找 `package.json`,在 src-tauri/ 下会报 Missing script: build);PowerShell 直调 npm 需 `npm.cmd`(执行策略拦截 npm.ps1);debug 构建命令 `cargo tauri ohos build -d -t aarch64`,完整环境配方见 §8。

**hvigor 打包阻断(原 3 项,已全部解除;另发现 2 项新坑)**:

1. ~~`env.rs` 强制 `DEVECO_SDK_HOME = parent³(OHOS_HOME)`,按 SDK 根契约算得 junction 伪根 → 00303312~~ **已解决**:在用户空间为伪根补一个 `default` junction(`$env:USERPROFILE\ohos-devstudio\default` → `$dev\sdk\default`),使 parent³ 落点本身成为合法 SDK 根(含 `default\sdk-pkg.json`,hvigor 即通过)。`OHOS_HOME` 保持不变,ohrs/交叉编译侧零影响。注意:不能把 `26` 版本层 junction 直接建进真实 SDK 目录——经 junction 写 `Program Files` 会被拒(Access Denied),补伪根是纯用户空间操作;
2. `compatibleSdkVersion: "5.0.0(12)"` **实测非阻断**:SDK 路径修复后 CompileArkTS → PackageHap 全程放行(all-in-one SDK 容忍该旧值),暂不改动;
3. hvigor daemon 假死:**对策固化**——迭代一律用 `cmd /c "... > log 2>&1"` 文件重定向(cmd 只等主进程退出,不随 daemon 句柄挂起;PowerShell `*>`/Tee 才会假死),每轮构建前杀 hvigor daemon(node/java,按 CommandLine 匹配);
4. **新坑:WS 脐带**。hvigor 的 tauri 集成在 `:entry:default@PreBuild` 后调用内部子命令 `cargo tauri ohos dev-eco-studio-script --target aarch64`,该子命令从 `%TEMP%\<identifier>-server-addr` 读父进程写下的 WebSocket 地址并经 JSON-RPC 拉取构建选项(tauri-cli `crates/tauri-cli/src/mobile/mod.rs:410`)。**独立 `hvigorw assembleHap` 不可行**:没有活着的父 `cargo tauri ohos build` 进程提供 WS 服务器;读到陈旧 addr 文件则 ConnectionRefused panic(00308018)。本文档原"下一步"设想的独立 hvigorw 迭代路线作废;
5. **新坑:java PATH**。`:entry:default@PackageHap` 调 SDK toolchains 的 Java 打包工具(app_packing_tool),Node spawn 裸命令 `java`——PATH 无 java 时报 `spawn java ENOENT`(00308018)。修复:PATH 加入 DevEco 自带 JBR(`$dev\jbr\bin`)并设 `JAVA_HOME=$dev\jbr`。此前所有尝试都死在更早环节,从未到达需要 java 的打包步骤。

**最终打通(2026-10-03 21:45)**:`cargo tauri ohos build -d -t aarch64` 全链路 EXIT=0,33 个 hvigor 任务全过(SignHap 因无 signingConfigs 跳过,符合预期)→ `entry-default-unsigned.hap`(52.4MB;dev .so 378.7MB 经 `DoNativeStrip` 裁剪后打包)落位 `gen/ohos/entry/build/default/outputs/default/`。端到端自动化脚本:`scripts/build-ohos.ps1`(2026-10-05 起入库,原 `%TEMP%\opencode\` 机器本地版;自包含环境 + fail-fast 预检 + 看门狗超时杀进程树 + 10s 心跳进度 + daemon 清理,成功/失败路径均已实测)。

### 6.4 Phase 1 收官:模拟器首亮(2026-10-03 23:25)

**成果**:DevEco x86_64 模拟器(`hdc list targets` → `127.0.0.1:5555`,`uname -m` = x86_64)上完整点亮 RapidRAW——**非空白窗口**:欢迎页全套 UI 渲染(标题/示例摄影图/Open Folder 按钮/设置图标/版本号 1.6.4),Canvas 图片正常,ArkWeb 渲染进程存活,`tauri`/`asset` 自定义协议已注册(hilog 实证 `--ohos-scheme-handler-custom-scheme={"asset":…,"tauri":…}`),无崩溃、无报错弹窗(仅有 OHOS 首次运行"添加到桌面"常规提示)。

**x86_64 交叉链路(增量改造,全部复用既有配方)**:

| 项 | 变更 |
|---|---|
| `src-tauri/ohos/ohos-toolchain.cmake`(入库) | 架构参数化:`OHOS_ARCH` env(`aarch64` 默认 \| `x86_64` \| `armv7`);未设时与旧版行为等价,既有配方零影响 |
| `~/.cargo/bin`(机器本地) | 新增 `x86_64-unknown-linux-ohos-{clang,clang++,ar}.cmd`(junction 无空格路径,同 6.1 模式,已冒烟验证) |
| `src-tauri/.cargo/config.toml`(机器本地,gitignored) | 新增 `[target.x86_64-unknown-linux-ohos]` 链接器段 |
| `scripts/build-ohos.ps1`(现入库;原 `%TEMP%\opencode\`) | 参数化 `-Target aarch64(默认) \| x86_64 \| armv7`,短名→完整三元组映射驱动 CC/CXX/AR env 与 `-t` 参数 |

**装机/启动/取证命令(已验证)**:

```powershell
hdc install -r <hap>                    # 未签名 HAP 模拟器直接可装
hdc shell aa start -a EntryAbility -b io.github.CyberTimon.RapidRAW
hdc shell snapshot_display -f /data/local/tmp/x.jpeg; hdc file recv /data/local/tmp/x.jpeg <本地>
hdc shell "hilog -x | grep -iE 'CppCrash|rapidraw.*(fatal|crash)'"   # 崩溃扫描(结果:无)
```

**已知瑕疵(Phase 2 候选项)**:

1. ~~应用显示名为 "label"~~ **已解决(2026-10-03 23:49)**:根因是 entry 模块 `resources/base/element/string.json` 的 `EntryAbility_label` 为字面量 "label"(`module.json5` 的 ability label 引用它;AppScope 的 `app_name` 本就是 "RapidRAW")。修改三处:① entry 字符串资源 `EntryAbility_label`/`EntryAbility_desc`/`module_desc` → "RapidRAW";② 分层图标:`AppScope` 与 entry 两处 `layered_image` 的前景层换为 `src-tauri/icons/full_res_original.png`(960→1024 高质量缩放;源图四角透明,由同色背景层补足、无缝),背景层换为实色 `#1D1D1D`(源图标边缘采样值),`startIcon.png`(144×144)同源缩放;③ **坑:桌面快捷方式缓存首次安装时的 label**——`hdc install -r` 只刷新图标不刷新文字,需 `hdc uninstall` 全卸重装、首启时在"添加至桌面"弹窗确认后重新生成。截屏验证:桌面/Dock 图标为深色底白色光圈 R,文字 "RapidRAW",系统弹窗文案亦正确引用 "RapidRAW";
2. ~~frameless 未生效:双标题栏~~ **已解决(2026-10-03 23:39)→ 方案反转(2026-10-04,见 6.5)**:曾用 `setWindowDecorVisible(false)` 隐藏系统装饰、自绘标题栏独占;后发现该 API 只隐藏视觉层,系统窗口按钮的**输入矩形原位残留**并继续拦截触摸(点自绘"最大化"图标实际触发系统"最小化",数据丢失风险)→ 2026-10-04 起改为**保留系统装饰栏 + OHOS 隐藏自绘三按钮 + 拖动走桥**(完整方案与验证见 6.5)。仍然有效的教训:
   - `super.onWindowStageCreate` 必须保持 fire-and-forget——一旦加 `await`,ability 启动后 ~200ms 即被终止(Kill Reason:ClearSession,exit 0),系生命周期时序敏感;
   - `setWindowDecorVisible` 若再用:必须延迟到 `loadContentByName` 之后(更早调用抛 1300002 "window state abnormal"),且模拟器冷启动显著慢于热启动,固定延迟会偶发失败(须重试循环);
   - tao-ohos 的窗口操作仍是 stub(`set_minimized`/`set_maximized`/`set_fullscreen` 空操作、`drag_window` 返回 NotSupported)——tauri JS API 路径(`appWindow.minimize()` 等)在 OHOS 无效;应用层已用 Rust↔ArkTS 桥绕过(见 6.5),tao 级修复属 fork 工作;
   - `setWindowDecorVisible` 只隐藏标题栏视觉、保留窗口边框(仍可拖边调整大小),**且系统按钮输入矩形残留**——这正是方案反转的根因;
    - EntryAbility post-init 补丁现为**窗口控制回调注册 + pick_folder 桥**(不含任何 decor 代码),仍位于 gitignored 的 `gen/ohos/entry/src/main/ets/entryability/EntryAbility.ets`——**`cargo tauri ohos init` 重跑后需按 6.5/6.6 重打**;
3. 双架构 .so 同包:`entry/libs` 同时存在 `arm64-v8a`(383.7MB)与 `x86_64`(389.9MB)dev .so → strip 后 HAP 120.5MB;模拟器取 x86_64 可正常运行,出真机包前应清 `entry/libs` 或配 abiFilters 瘦身。

### 6.5 Phase 2:窗口控制桥与系统装饰输入矩形(2026-10-04)

**成果**:窗口控制在 OHOS 上可用——系统装饰栏原生提供 最小化/最大化/关闭(所见即所点),窗口拖动走 Rust↔ArkTS 桥(`startMoving()`,实测 swipe +400px 窗口精确移动 +400px)。

**1. Rust↔ArkTS 窗口控制桥(已入库)**

- `src-tauri/src/ohos_integration.rs` `window_controller` 模块:`#[napi] register_ohos_window_controller` 经 napi-ohos `ThreadsafeFunction` 把分发回调注册进 ArkTS;tauri 命令 `ohos_window_control(operation)`(minimize/maximize/restore/start_drag;非 OHOS 返回 Err)与 `is_ohos_build()`(`tauri-plugin-os` 在 OHOS 返回 "linux",前端以此区分);`lib.rs` generate_handler 已注册
- `src/window/TitleBar.tsx`:`isOhos` 探测(`is_ohos_build`);拖动区 `onPointerDown` → `start_drag`(`data-tauri-drag-region` 在 tao-ohos 是 stub 死路);min/max handler 的 OHOS 分支与桥命令**休眠保留**(见 4)
- `gen/ohos/…/EntryAbility.ets`(机器本地):`registerRapidrawWindowController` 动态 import `librapidraw_lib.so`(**必须变量名**,字面量触发 ArkTS 模块解析失败)注册回调,分发到 `win.minimize()/maximize()/restore()/startMoving()`

**2. 坑:napi-ohos TSF 的 CalleeHandled 约定**

默认 `ThreadsafeFunction<…, true>`(CalleeHandled)是 error-first 约定——Rust `call(value)` 时 JS 回调收到 `(null, value)`,ArkTS 读第一个参数拿到 `null`(曾表现为 `'unknown window op null'`)。必须用 `ThreadsafeFunction<String, Unknown<'static>, String, Status, false>`,且此时 `call()` 直接收值、不收 `Result`。

**3. 关键发现:系统装饰输入矩形残留(方案反转的根因)**

`setWindowDecorVisible(false)` 只隐藏视觉层;系统窗口按钮的输入矩形**原位残留**并继续拦截标题栏区触摸,且系统按钮顺序与自绘不同。x86_64 模拟器实测(freeform 窗 x520-2600、系统条 y285-355、按钮中心 y≈317):系统 **最大化@x2370-2418 / 最小化@x2450-2500 / 关闭@x2525-2565**(距窗右缘约 206/125/55 物理px),自绘按钮 最小化@2413 / 最大化@2480 / 关闭@2550——交错重叠:**点自绘"最大化"命中系统"最小化"(窗口消失)、点自绘"最小化"命中系统"最大化"**,自绘"关闭"恰与系统"关闭"重合(动作碰巧正确)。触摸只有错过全部系统矩形才会精准到达 webview(内容区不受影响)。d.ts 全量核查:无 API 可移除该输入矩形(setWindowDecorVisible 仅视觉、DecorButtonStyle 仅样式、setWindowDecorHeight 下限 37vp)。

**4. 最终方案:系统装饰栏 + 自绘三按钮隐藏**

- EntryAbility **不再调用** `setWindowDecorVisible(false)`(连同 maximize 后的 `setTitleAndDockHoverShown` 抑制链一并移除——那是 decor 隐藏方案的伴生补丁)
- 前端 OHOS 隐藏自绘三按钮:`{isLinux && !isOhos && (…)}`(TitleBar.tsx);自绘标题栏其余部分(标题文字、拖动区)保留
- 拖动走 `start_drag` 桥;min/max/restore 桥命令与 handler 休眠保留——若未来 OHOS 修复"隐藏 decor 后输入矩形残留",删掉 `!isOhos` 即可恢复自绘按钮(届时须重验矩形行为)

**5. 验证证据(2026-10-04,x86_64 模拟器;tap 坐标为本会话窗口位置样例)**

| 项 | 操作 | 结果 |
|---|---|---|
| 系统最大化 | tap (2394,317) | 全屏 ✓ |
| 系统最小化 | tap 输入矩形区(2450-2500) | 最小化到 dock,`aa start` 恢复 ✓ |
| 系统关闭 | tap 输入矩形区(2525-2565) | 应用退出,`aa start` 重启 ✓ |
| 系统还原 | 最大化态输入矩形 tap | 还原自由窗 ✓(可见态还原钮未逐像素定位,原生开关与最大化同钮) |
| 拖动桥 | swipe 自绘栏 (1200,405)→(1600,405) | 窗口左缘 515→915,+400px 精确 ✓(WMS 日志实证) |
| 内容区交互 | tap 齿轮按钮 | 设置面板打开 ✓ |

**6. 模拟器测试方法论补充**

- 触摸注入:`hdc shell uinput -T -m x y x y 120`(tap;带位移即 swipe);截屏 `snapshot_display -f …` + `hdc file recv`;状态判别用截屏字节数(无窗 ~200KB / 自由窗 ~296KB / 全屏 ~303KB)+ 定点像素亮度——注意全屏态应用左半是黑白照片(含大片亮区),判窗口态须用窗外缘点(如 (2900,800):自由窗=亮壁纸,全屏=暗面板)
- uinput 触摸注入会**会话级死亡**(键盘注入仍活;判别:tap Dock 图标无反应);`hdc shell reboot` 重启设备即恢复
- 冷启动显著慢于热启动(曾致 500ms 固定延迟的 decor 隐藏偶发失败)——"加载后调用"逻辑必须重试循环而非固定延迟

**7. 已知限制/外观**

- 系统条颜色跟随系统主题:浅色主题下"浅系统条+深色应用"有视觉断层(真机深色主题则协调)
- 系统条左侧显示应用名 "RapidRAW",与自绘栏标题文字重复
- EntryAbility post-init 补丁(重跑 `cargo tauri ohos init` 后需重打):**窗口控制回调注册块 + pick_folder 桥**(见 6.6),不含任何 decor 代码

### 6.6 Phase 2:FileKit 文件夹选择桥——"打开文件夹"(2026-10-04)

**成果**:"打开文件夹"在 OHOS 端到端可用——欢迎页/图库"添加文件夹"按钮 → 系统"选择路径"对话框(DocumentViewPicker)→ 选定文件夹成为图库根(树渲染、扫描链无错)。~~跨重启持久化~~ → **2026-10-05 真机修正:授权为进程会话级,任何进程死亡即失效(模拟器才持久;失效已由 6.14 探针自愈,详见该节)**;**同日深夜再修正:声明 `FILE_ACCESS_PERSIST` 后 2in1/PC 形态经 v3(fileShare.persistPermission)恢复跨重启持久,会话级语义现仅适用于 pad 形态与未声明权限的构建(见 6.14 第 5 节)**。

**1. 背景:tauri-plugin-dialog 在 OHOS 被排除**

其 `rfd` 后端无 OHOS 支持(Cargo.toml 已 gated `not(ohos)`),桌面流的 `open({ directory: true })` 在 OHOS 是死路。方案:FileKit `DocumentViewPicker` 经 Rust↔ArkTS 桥——镜像 6.5 窗口控制桥模式(TSF 分发 + napi 回传)。

**2. 桥架构(已入库)**

- `src-tauri/src/ohos_integration.rs` `file_bridge` 模块:`static PICK_TX: Mutex<Option<oneshot::Sender<Option<String>>>>` + 防重入守卫;`#[napi] resolve_ohos_pick_folder(path)`(ArkTS 回传,null=取消);`pick_folder()`(oneshot + `dispatch("pick_folder")` + rx.await);tauri 命令 `pick_ohos_folder`(非 OHOS 返回 Err),`lib.rs` generate_handler 已注册
- 前端:`AppProperties.tsx` Invokes 枚举(`IsOhosBuild`/`PickOhosFolder`);`useSettingsStore.ts` `isOhos` 状态(`initPlatform` 探测 `is_ohos_build`——`tauri-plugin-os` 在 OHOS 返回 "linux" 不可用);`useAppNavigation.ts` `handleOpenFolder` OHOS 分支(`invoke(PickOhosFolder) ?? ''`),选中后走桌面同款扫描链(GetFolderTree/ListImagesInDir 零改动)
- `gen/ohos/…/EntryAbility.ets`(机器本地)`pick_folder` case:`DocumentSelectMode.FOLDER` + `maxSelectNumber: 1` → `DocumentViewPicker(this.context).select()` → `.then` 内 `new fileUri.FileUri(uri).path` + `fs.statSync` 校验(同进程可读 ⇒ Rust `std::fs` 可读)→ `resolvePickFolder(path)`;`.catch`/取消回传 null

**3. 关键验证事实(x86_64 模拟器,2026-10-04)**

- **URI 形态**:返回 `file://docs/storage/Users/currentUser/Images`(侧栏"图片"的物理目录名是 `Images`)→ `FileUri.path` 直出应用沙箱路径 `/storage/Users/currentUser/Images`,无需手工解析
- **Rust 可读性**:`fs.statSync` 成功(isDirectory=true)——ArkTS fs 与 Rust std::fs 同进程同挂载命名空间,stat 通过即原生可读;GetFolderTree/ListImagesInDir 实测无错
- **跨重启可读**:选定目录在 force-stop + 重装 + 重启后,会话恢复的 GetFolderTree 仍成功
- **picker 记忆上次位置**:再次打开直接停在上次所在目录
- hilog 证据(09:49:44,最终构建):

```
[RapidRAW] pick_folder raw uri: file://docs/storage/Users/currentUser/Images
[RapidRAW] pick_folder path: /storage/Users/currentUser/Images
[RapidRAW] pick_folder stat ok: isDirectory=true
```

**4. UI 观察与排障教训(非桥缺陷)**

- 图库头部路径显示在窄中列被 truncate 成 "/"+微弱省略号——**显示假象**:Sources 树行高亮(`isSelected = node.path === currentFolderPath`)证明 currentFolderPath 是完整正确路径。OHOS 自由窗(2080px 宽)中列窄,标题"图库"亦竖排;桌面宽窗口无此现象。判状态勿信路径显示、要信树行高亮
- 欢迎页有"首次/回来"两变体(回来变体为"继续会话"+深色"添加文件夹");**会话自动恢复是异步慢流程**(模拟器 90~120s,GetFolderTree 走沙箱 FUSE 慢),启动初期取证须考虑该延迟
- 左面板底部图标行(info/folder/export)是**面板模式切换器**;"添加文件夹"的真实入口:欢迎页按钮、FolderOptionsMenu 下拉项、空树 Plus 行。look_at 曾把 export 切换钮误判为 folder-plus(两次点错、面板被切走)——**图标语义必须回源码 grep onClick 接线交叉验证**
- hilog 缓冲在 chromium vsync 刷屏下快速轮转——**事件后 2~4s 内立即抓取**;grep 模式用 `RapidRAW\]`(带右括号)避开 WMS bundle 名与 qos_ctrl "Failed to open" 噪声
- 截屏字节数判别补充:欢迎页 ≈297KB / 图库 ≈219KB / picker ≥237KB

**5. 已知限制/顺延项**

- 网格实际图片显示未在本会话直接验证(用户存储无图片可扫;shell 受 SELinux 限制写不进用户存储,root/su 不可用)——但读取链已由 stat/GetFolderTree/ListImagesInDir(空结果无错)证实,图片渲染链已由 6.4 编辑器照片测试覆盖,残余风险低(→ 缩略图渲染已由 6.7 导入链实证)
- FileKit 单文件选择(导入流)与保存对话框(LUT/预设/导出面板等其余 plugin-dialog 消费点)仍待接桥——同模式复制即可(→ 已由 6.7 落地)
- 相册导出 `save_image_bytes_to_ohos_gallery`(photoAccessHelper)顺延(→ 已由 6.7 落地)

### 6.7 Phase 2:文件选择与相册导出桥——导入/导出全链路(2026-10-04)

**成果**:OHOS 端到端打通两条链——①文件选择+导入(FileKit picker → 图库缩略图渲染);②相册导出(编辑后 RAW → JPEG 编码 → 系统保存对话框 → 媒体库落库)。铁证:

```
[RapidRAW] save_to_gallery ok: file://media/Photo/4/IMG_1791093991_003/DSC09105_edited.jpg
```

图库 picker 侧栏「来自应用 → RapidRAW」可见导出产物(两次成功的对话框导出均落库,系统侧归属正确)。

**1. 桥架构升级(6.6 单请求 oneshot → requestId 注册表,支持并发多请求)**

- `ohos_integration.rs` `file_bridge`:`static REQUESTS: Mutex<Option<HashMap<u32, oneshot::Sender<String>>>> = Mutex::new(None)`——**`HashMap::new` 非 const fn,`Mutex::new(HashMap::new())` 触发 E0015**,必须 Option 包裹 + `get_or_insert_with` 惰性初始化;配 `park_sender`/`remove_sender`/`complete_request`/`NEXT_REQUEST_ID: AtomicU32`
- 4 个类型化 napi 导出:`resolve_ohos_pick_folder` / `resolve_ohos_pick_files` / `resolve_ohos_save_to_gallery` / `resolve_ohos_save_file_as`;回传统一为 Rust 侧构造的 JSON 字符串(避开 ArkTS 对象字面量严格检查)
- `dispatch_event(payload)`(JSON 请求)与 `dispatch(op)`(纯窗口控制)双格式;`request()`(async 命令上下文)+ `save_request_blocking()`(`blocking_recv`,供 `spawn_blocking` 导出线程)
- `EntryAbility.ets`(机器本地):回调内 `JSON.parse(op) as Record<string, Object>` 路由到 4 个 handler,try/catch 回退 6.5 的纯 op 窗口控制

**2. 导入链(pick_files)**

- 命令 `pick_ohos_files(supported_extensions, max_select)`;`collapse_suffix_filters`:**FileKit `DocumentSelectOptions.fileSuffixFilters` 全输入 ≤100 字符,超限 picker 打开 0.6s 后静默自灭**(d.ts 实证);格式 `'描述|.ext1,.ext2'` 或 `'.ext'`;实现为去重 + 归并单元素,>96 字符整组放弃(前端 validFiles 客户端校验兜底,与 Android 行为一致)
- `DocumentViewPicker` 多选 → `new fileUri.FileUri(uri).path` → **Rust `std::fs` 可直读媒体库路径**(`/storage/media/...`,此前最大未知点,导入链实证)
- 前端消费点:MainLibrary 导入 FAB 门控(`props.isAndroid || useSettingsStore.getState().isOhos`)、ImagePicker、useFileOperations、LUTControl、PresetsPanel(`pickPresetExportPath` helper)

**3. 导出链(save_to_gallery)——全部四个坑**

Rust:`save_image_with_metadata` 增 `_app_handle` 尾参;OHOS 分支 `save_image_bytes_to_ohos_gallery` → `ohos_export_temp_file`(`{app_cache_dir}/export_bridge/{counter}_{name}`,AtomicU64 单调防并发撞名)→ `save_request_blocking` 派发;mask alpha(PNG)与 cube LUT 走 `save_file_bytes_to_ohos_picker`;两处 `create_dir_all` 加 OHOS 守卫(移动端哨兵路径不落盘建目录)。

ArkTS(EntryAbility.saveToGallery),每坑皆有实锤:

- **Tauri 路径解析**:OHOS 满足 `not(target_os="android")` → path 模块用 **desktop.rs 解析器**(`dirs` crate),`app_cache_dir()` 返回 **`/storage/Users/currentUser/.cache/io.github.CyberTimon.RapidRAW/`——用户公共存储区,非应用沙箱**(hilog 实证 tempPath)。跨进程系统服务无法授权该路径
- **14000011**:`showAssetsCreationDialog` 的 `srcFileUris` 必须是 `fileUri.getUriFromPath()` 产出的规范沙箱 URI;传真实路径 → CommonSaveAbility 判 `uris or photoTypeArray parameter is invalid` → 应用侧只见 14000011 "Internal system error"(modal 创建 ~4s 后自灭)。修复:ArkTS 先把字节中转到 `this.context.cacheDir/export_bridge/`(文件名复用 Rust 计数器前缀保证唯一)再转 URI
- **13900015**:**OHOS `fs.mkdirSync(path, true)` 不幂等**——目录已存在照抛 "File exists"(与 POSIX `mkdir -p` 语义相反);修复:`fs.accessSync` 探测,不存在才 mkdir
- **arkts-limited-throw**:ArkTS 的 throw 只接受 Error 类型,不能 rethrow catch 变量(嵌套 try + `(e as BusinessError).code` 判断的方案也因 rethrow 编译失败)
- **createAsset 权限墙(201)**:`WRITE_IMAGEVIDEO` 为受限权限,常规签名不可得;`showAssetsCreationDialog`(API 12)弹窗授权替代——**用户同意后系统创建媒体资产并返回带永久写权限的 URI 列表,应用必须自行 fd-to-fd 拷贝字节**(`fs.openSync(沙箱源)` + `fs.openSync(URI, READ_WRITE|CREATE)` + `fs.copyFileSync(fd, fd)`,官方 demo 模式);返回空数组=用户拒绝;数组中非 URI 项=批量错误码(-3006 无效字符 / -2004 类型与扩展名不匹配 / -203 无效文件操作);`PhotoCreationConfig` 勿传 `subtype`(传了配置不生效);module.json5 abilities 的 label+icon 必配(对话框显示应用名)

**4. 验证证据(x86_64 模拟器)**

- 导入:picker 导航(图库 → 相册)→ 选中截图 → `pick_files -> 1 file(s)` → 图库缩略图 +「导入完成!」
- 导出:DSC09105.ARW(25.6MP Sony RAW + 实际编辑:AgX/曝光 0.63/对比度 16/阴影 22/曲线)→ JPEG q90 导出 → 系统对话框「允许"RapidRAW"保存 1 张图片?」(带预览)→ 允许 → `save_to_gallery ok`(见成果);导出后应用状态健康(选中保留、面板数据完整、无错误)
- 媒体库:图库 picker「来自应用 → RapidRAW (2 项)」——两次成功的对话框导出均落库
- 静态门禁:cargo fmt / clippy `-D warnings` / host+aarch64 `cargo check` / tsc / eslint 全绿(修改文件零新增)

**5. 运维与排障教训**

- **hvigor daemon OOM**:连续多次构建后 node 守护进程堆耗尽;修复 = 杀 hvigor node 进程 + `$env:NODE_OPTIONS='--max-old-space-size=8192'`(每个新 shell 须重设;build-ohos.ps1 启动时自清 stale daemon)
- **UI 测试坐标随面板状态漂移**:库视图布局取决于源面板展开/折叠与文件夹视图(同一缩略图在 (1354,882) 与 (820,528) 两种布局间切换);自动化必须逐步快照校准,历史坐标不可信
- **hilog 环形缓冲轮转极快**(chromium vsync 刷屏 + 对话框加载洪流):事件后 2~4s 内抓取;错过后系统侧真相(CommonSaveAbility 日志)不可复现,只能重放操作
- **系统侧日志才是根因现场**:14000011 的真实原因在 CommonSaveAbility(独立 PID)的 `uris or photoTypeArray parameter is invalid`,应用侧只看到 "Internal system error"——排障要抓系统进程日志,别只 grep 应用 tag

**6. 已知限制/顺延项**

- 批量导出每张图各弹一次系统对话框(`save_request_blocking` 串行等待;API 单次调用上限 100 张但对话框粒度为单次)——follow-up:评估 `MediaAssetChangeRequest` 批量授权或收集式 UX
- 库视图导出成功无 toast(静默完成;编辑器视图未复测)
- 调整面板直方图 canvas 无选中时渲染噪点/空白帧——待查(疑似 GPU readback 或未初始化缓冲,非本链路引入)
- `save_file_as`(.cube LUT / 预设另存为)路径已实现,设备端验证顺延
- Tauri `app_cache_dir()` 在 OHOS 指向用户公共存储区的偏差由 ArkTS 沙箱中转兜底;长期可考虑让 Rust 直写沙箱缓存目录

### 6.8 Phase 2:系统 UI 适配(深色模式/安全区/返回手势)与 Phase 3-1 编辑器 GPU 管线点亮(2026-10-04)

**成果**:Phase 2 系统 UI 三件套全部实证——深色模式跟随系统(双向实时翻转)、返回手势(三分支优先级正确)、安全区日志;Phase 3-1 提前完成——编辑器 compute+回读路径全链路点亮(Mali-G77 GLES 后端,曝光交互 ~29 FPS)。

**1. 实现链路**

Rust 侧(`ohos_integration.rs` 新增 `system_bridge` 模块):

- napi 导出 `notify_ohos_back_pressed()` / `notify_ohos_color_mode(dark: bool)`;`OnceLock<AppHandle>` 存 handle;`AtomicU8` 缓存色彩模式;事件 `ohos-back-pressed` / `ohos-color-mode`(payload `{dark}`);日志 `OHOS system color mode changed: dark={}`
- 命令 `get_ohos_color_mode`(lib.rs 注册,启动时前端查询当前系统色);`initialize_ohos` 存 AppHandle
- `gpu_processing.rs` 附加 adapter 日志:`Using GPU adapter: {} (backend: {:?}, type: {:?})`

前端:

- `Theme.System = 'system'`(themes.ts)+ `resolveThemeId(theme, isSystemDark)` 解析;splash 主题解析(MainLibrary)
- `isSystemDark` state(useSettingsStore)+ 初始化 `GetOhosColorMode` invoke(useAppInitialization)+ `'ohos-color-mode'` 事件监听 + `prefers-color-scheme` 兜底
- SettingsPanel 主题下拉新增"系统"选项;i18n 全部 14 locale 增加 `system` 键
- `useAndroidBackHandler` 扩展 `isOhos`:`'ohos-back-pressed'` 事件 → 复用 Android 的合成 Escape 链(模态关闭 > 编辑器退库 > 库退主页 优先级不变),保留 `window.__handleAndroidBack` 兼容

ArkTS 侧(gen/ohos 机器本地,模板已固化 `src-tauri/ohos/EntryAbility.template.ets` + `Index-page.template.ets`,防 `cargo tauri ohos init` 重跑丢失,重打步骤同 6.4):

- `EntryAbility.ets`:`onConfigurationUpdate` → super 调用后 `pushColorMode(ConfigurationConstant.ColorMode.COLOR_MODE_DARK/LIGHT)`;avoid area 日志;局部变量 window→win(修遮蔽);`await windowStage.loadContent('pages/Index')`
- `pages/Index.ets` 重写为自定义 @Entry 页面:`DefaultXComponent()` 托管 webview + **页面级** `onBackPress()` → `notifyOhosBackPressed()` → `return true`

**2. 关键坑(@ohos-rs/ability 0.4.0-beta.0 与 vendored 0.3.0 的差异)**

- 实装 0.4.0-beta.0(ohpm 源,`gen/ohos/oh_modules/.ohpm/`)与 vendored 0.3.0 API 不兼容:无 MainPage `onBackPressIntercept` 机制;`onConfigurationUpdate` 存在且**必须先 super 调用**;`DefaultXComponent()` 无参(从 AppStorage 取 moduleName)
- `UIAbility.onBackPressed()` 语义:**true=转后台 / false=销毁**,没有"保持前台并消费"的语义 → 系统返回手势必须在**页面级** `onBackPress` 返回 true 拦截,Ability 级 API 不可用
- ArkTS `console.*` 不进 hilog(JSAPP 通道关闭)——取证用 Rust fern 日志(app.log)+ 截图像素采样
- 返回注入:`hdc shell "uitest uiInput keyEvent Back"` 有效;`uinput -K 158`(KEY_BACK)无效;键盘 Escape 走 webview 键盘链,不触发系统返回

**3. 验证证据(x86_64 模拟器,逐步快照校准坐标)**

- 深色模式:设置→主题下拉出现"系统"(i18n 生效)→ 选中持久化(`settings.json theme: "system"`)→ 系统浅色下应用立即翻浅色;控制中心(托盘 sliders 图标)切深色 → app.log `OHOS system color mode changed: dark=true` + UI 全域翻转(标题栏/设置卡片/面板);切回 → `dark=false` + 翻回浅色——**双向实时实证**
- 返回手势三分支(`uitest uiInput keyEvent Back`):编辑器→图库;复制粘贴设置模态→关闭(优先级正确,不退出图库);图库→主页(Escape 链 `handleGoHome` 分支)
- Phase 3-1 铁证(app.log,小图 rr_test.ARW):
```
Using GPU adapter: Mali-G77 (backend: Gl, type: IntegratedGpu)
Creating new GPU Processor for dimensions up to 1280x1024
[apply_adjustments] 1280x853 processed (ROI: 1280x853) on GPU in 320.501419ms (3.12 FPS)
[apply_adjustments] ... (ROI: 512x341) ... on GPU in ...ms (28.71 FPS)   ← 曝光滑条拖动交互
[estimate_export_size] 1280x853 processed (ROI: 1280x853) on GPU in 127.132444ms (7.87 FPS)
[generate_thumbnail_data] ... on GPU in ...
```
- 编辑视觉生效:曝光拖至 4.25 → 月亮过曝泛白(编辑参数 → GPU compute → 回读 → IPC → Canvas 全链路)
- `[ERROR] Fake map`:wgpu-hal GL 后端回读路径的良性产物(渲染正常),已知现象非缺陷

**4. 门禁**:cargo fmt / clippy `-D warnings` / host + aarch64-ohos `cargo check` / tsc(70 项既有基线,零新增)/ prettier / eslint 零新增。

### 6.9 Phase 3-2:wgpu GLES surface 直渲可行性评估(2026-10-04)

**结论:GO(有条件)**——技术链路全部就绪,估算 1~2 周 / 300~600 LOC;建议排在真机性能基线(6.10)之后立项,以真机数据决策优先级。

**1. 现状与动机**

OHOS 复用 Android 的 compute-only + 回读路径:GPU compute → 回读(map)→ 编码 → IPC bitmap → 前端 Canvas 2D 绘制。每帧成本 = 回读 stall(`Fake map` 即此路径产物)+ IPC 序列化 + Canvas 纹理上传。模拟器实测:编辑器全预览首帧 244~320ms、交互 ROI 上限 ~29 FPS(6.8/6.10)——回读+IPC 是主要 suspected 瓶颈,直渲可整体消除该段。

**2. 可行性事实(2026-10 核验)**

- **wgpu 上游已支持 OHOS GLES surface**:gfx-rs/wgpu PR #7085(feat(gles): support gles backend on openharmony,2025-02-13 合并,v25.0.0 起);本仓现依赖 **wgpu 29.0.4**(Cargo.toml `wgpu = "29.0"`,注释:为修 Apple P3 色偏自更高版本降级)——**无需 fork wgpu**
- **raw-window-handle 0.6.2**(现依赖)提供 `OhosNdkWindowHandle { native_window: NonNull<c_void> }`(rwh PR #164 引入,`RawWindowHandle::OhosNdk` 变体,`target_env = "ohos"` 启用)——句柄类型已在依赖树内
- **tao fork**(patch 表,feat/open-harmony)已实现 OHOS 窗口 → raw-window-handle 链——窗口侧就绪
- **`tauri::ohos::APP`** 全局 `Mutex<Option<OpenHarmonyApp>>`(tauri fork 提供)可从 Rust 侧访问 OHOS 应用/窗口状态
- 集成形态:ArkTS 侧独立 XComponent 提供第二 native surface(与 webview 宿主的 DefaultXComponent 分层),Rust 侧经 `OhosNdkWindowHandle` 构建 `wgpu::Surface`,编辑器 canvas 直渲;参照 jinleili/wgpu-in-app PR #17(wgpu OHOS XComponent 示例)

**3. 风险与开放问题**

- **gfx-rs/wgpu #9158(GLES srgb issues (mainly on openharmony),OPEN,area:correctness)**:OHOS GLES 的 FRAMEBUFFER_SRGB 依赖 EXT_sRGB_write_control,扩展缺失时 set 即报错——sRGB surface 配置需 PoC 验证(规避:linear 中间纹理 + blit,或协商非 sRGB surface 格式)
- **层级合成**:webview 覆盖整窗,GPU surface 需 XComponent 分层/透明打孔;ArkUI 支持独立 surface 层,但与 webview 的输入事件穿透、resize 同步需实测(Android 正因该问题走回读路径)
- **双路径共存**:须保留回读路径作 fallback(Android 共用代码),feature flag 或运行时探测切换;注意 Cargo.lock 冻结纪律(§6.2)
- 模拟器 GLES 透传(Mali-G77 报告)≠ 真机驱动行为,PoC 须真机复核

**4. 工作量估算**:PoC(XComponent surface 创建 + wgpu Surface + 单帧 present)2~3 天;编辑器管线接入(替换回读输出 → surface present)3~5 天;输入/resize/生命周期 + 回退开关 2~4 天;合计 1~2 周,300~600 LOC(不含上游 issue 修复等待)。

**5. 建议路线**:真机到位先跑 6.10 基线量化回读+IPC 占比 → 若真机交互 FPS 不达目标(60 FPS)则立项 PoC,否则直渲降级 backlog;PoC 顺序 = surface 创建 → 单帧 present → 编辑器 canvas 替换 → 输入/生命周期 → fallback 开关。

### 6.10 Phase 3-3:高像素 RAW(≥60MP)性能与内存基线——模拟器(2026-10-04)

**成果**:4GB x86_64 模拟器上的 60MP 级基线落定:61MP 编辑器加载被系统 LowMemoryKill、45MP 编辑器全通(交互 14~29 FPS)、45MP 全分辨率导出进程死亡;**真机基线受阻(无硬件)**,资产与方法学已固化,设备到位即可复测。

**1. 测试资产**

- **DSC00395.ARW**(Sony ILCE-7RM4,61MP:有效 9504×6336 / 含边 9600×6376,123,111,424 B):raw.pixls.us 公共领域样张。CN 网络直连仅 ~19KB/s(15 分钟 16.8MB),改 12 路并行 Range 分块下载(curl -r,服务器支持 Range;~9 分钟全量)+ .NET 流按字节序拼接,首 8 字节 `49 49 2A 00` 校验通过
- **synth45.dng**(合成 45.4MP:8256×5504,86.7MB):Node 脚本生成最小有效 DNG(TIFF LE + RGGB CFA 16-bit 未压缩 + DNGVersion 1.4 + ColorMatrix1/AsShotNeutral/WhiteLevel/CalibrationIlluminant1)——用于隔离"内存天花板"与"管线缺陷":61MP 真机文件死亡后,需证明管线本身在次高 MP 下完好
- 合成素材注意:生成器 SRATIONAL 分子被 `Math.round` 取整(ColorMatrix1 实际写入 [[2,0,0],[0,2,0],[1,0,2]])+ 管线色调映射 → 默认曝光下渲染极暗(近黑),拖曝光 +EV 后色带正常显现(已实证管线完好)——不影响性能/内存结论;正式素材应写真有理数

**2. 测量通道**

- app.log(fern):`Raw enhancing`(全尺寸解码+去马赛克+校准)/ `downscale_f32_image` / `Creating new GPU Processor for dimensions up to WxH` / `[apply_adjustments]`(GPU 处理 + ms + FPS)/ `[process_preview_job]` / `[estimate_export_size]` / `[generate_thumbnail_data]` / `Batch Export: N cores, X GB free RAM -> M threads`
- `hdc shell "hidumper --mem <pid>"`(PSS 分解:native heap / mmap)
- `hdc shell "hilog -x | grep -E 'LowMemoryKill|onAbilityDied'"`(死亡归因;hilog 环形缓冲轮转快,事件后数秒内抓)
- `snapshot_display` + 宿主 System.Drawing 像素采样(画布渲染地面真相,替代目测)

**3. 结果(4GB x86_64 模拟器,Mali-G77 透传)**

| 场景 | 结果 | 证据 |
|---|---|---|
| 61MP ARW 库扫描/缩略图 | ✓ | ARW 走内嵌预览提取路径(无去马赛克日志行),downscale 20.25/9.43ms |
| 61MP ARW 编辑器打开 | ✗ LowMemoryKill | 去马赛克+校准后 crop 阶段被杀(日志止于 `crop: Rect{32:20, 9504x6336}`);hilog `errorReason: LowMemoryKill, removeSession true` |
| 45MP DNG 库缩略图 | ✓ | 无内嵌预览 → 全管线(去马赛克+校准+downscale)正常 |
| 45MP DNG 编辑器 | ✓ 全通 | `Raw enhancing took 983.48ms`;downscale 41.26ms;GPU Processor 按预览尺寸创建(1280×1024);全预览首帧 apply_adjustments 1280×853 244.47ms(4.09 FPS);estimate_export_size 238.38ms;PSS 1.76 GiB(1,846,777 kB,native heap 1.65GB / mmap 1.6GB) |
| 45MP 曝光拖动交互 | ✓ | interactive ROI 512×341 六样本 14.2~29.2 FPS(全样本中位 21.26;小图参照 ~29 FPS 上限) |
| 45MP 全分辨率导出 | ✗ 进程死亡 | `Batch Export: 4 cores, 1.0 GB free RAM -> 1 threads`(资源感知自适应单线程)+ `Creating new GPU Processor for dimensions up to 8448x5632` 后 hilog `errReason: onAbilityDied` |

**4. 结论**

- **交互编辑性能与源 MP 数无关**:GPU Processor 按**预览/ROI 分辨率**创建(1280×1024 / 512×341),45MP 与小图交互 FPS 同级(~29 上限,受回读+IPC 主导——6.9 直渲动机的直接依据)
- 源 MP 数影响三处:加载时间(45MP 全尺寸 CPU 增强 983ms)、内存峰值(f32 RGB ≈ 12 B/px:45MP ≈ 545MB、61MP ≈ 734MB;PSS 实测 45MP 编辑器驻留 1.76 GiB 含金字塔+缓冲)、全分辨率导出
- **4GB 模拟器天花板**:编辑器路径 45MP 通 / 61MP 亡(去马赛克 f32 中间链 ~2GB+ 触发系统查杀);全分辨率导出 45MP 即亡(编辑器驻留 1.76GB + 8448×5632 全尺寸 GPU 缓冲超出剩余预算)
- 61MP 缩略图走 ARW 内嵌预览提取,零去马赛克成本——大文件**浏览**不受编辑器天花板影响
- **真机预期**:8~16GB RAM + 独立 GPU 内存预算下,按上述内存模型推算 61MP 编辑器与全分辨率导出可行,待真机按本节方法学复测确认
- **缓解方向已评估**(2026-10-04,见 6.11):死因定位为 rawler 开发管线校准/裁剪阶段的"双缓冲同栖峰值"(1.6GB),非 f32 字节宽度本身;建议路径 = 短期降分辨率编辑模式+资源预检(编排层,零 fork)→ 中期驻留瘦身+校准/裁剪 in-place 小 fork;全管线 u16/f16 替换明确不做

**5. 阻塞与顺延**

- 真机基线:无硬件(项目级阻塞);复测清单 = DSC00395.ARW + synth45.dng + 本节测量通道命令序列
- 模拟器 GPU 为宿主透传,绝对数值不可外推;仅结构性结论(路径连通性/天花板存在性/资源感知行为)可迁移

### 6.11 Phase 3-3 后续:61MP LowMemoryKill 缓解方向——f32 管线可行性评估(2026-10-04)

**背景**:6.10 落定 61MP 编辑器加载被 LowMemoryKill 后,引出问题——**能否不使用 f32 管线**(全分辨率 RAW 管线改用 u16/f16 承载)以降内存?结论:**技术上可行但不值得——死因是开发管线内部的"拷贝语义峰值",不是 f32 的字节宽度;全类型替换是手术最大、收益错位的杠杆。**

**1. f32 在管线中的三个角色(代码证据)**

| 角色 | 位置 | 证据 |
|---|---|---|
| 计算介质 | rawler `Intermediate` 全程 | `PixF32`/`Color2D<f32,3>`(`RapidRAW-DngLab` fork,`imgop/develop.rs:131` `develop_intermediate`);校准矩阵 `xyz2cam` 含负系数、WB 增益 2~4×、线性域 >1.0,数学上必须浮点 |
| 传输类型 | 全下游公共契约 | `DynamicImage::ImageRgb32F`:`downscale_f32_image`(`image_processing.rs:216`)15 个调用方;GPU 上传在边界 `to_rgba_f16` 转 f16;降噪/蒙版/导出全部以 Rgb32F 为输入 |
| 驻留格式 | 会话状态 | `AppState`/`CachedPreview` 持全分辨率 `Arc<DynamicImage>`(61MP=734MB)+ small_image + GPU 缓存 |

注:rawler 为上游作者自有 fork(`Cargo.toml` git 依赖 @ `934af4b2`),技术上可改;但桌面版共用同一管线,改动有回归面。

**2. 61MP 加载峰值解剖(死因定位)**

`develop_intermediate` 内部字节账(9600×6376,各阶段存活缓冲):

```
rawimage.clone()      u16 CFA   122MB   ← develop 入口整份克隆(L132),全程存活
Monochrome 中间态     f32 CFA   245MB   ← as_f32() 转换(L138)
demosaic 输出         f32 RGB   734MB
── 校准 map_3ch_to_rgb(&pixels,…)  新分配 734MB,输入输出同栖 → 峰值 1.59GB(L252)
── 裁剪 pixels.crop(crop)          新分配 723MB,同栖        → 峰值 1.58GB(L279-284)
+ file_bytes(123MB 压缩原文件同时在内存)+ 应用基线 0.6~1.2GB
= 加载瞬间 ~2.3-2.9GB → 4GB 模拟器 LMK(与 6.10"日志止于 crop 行"吻合)
```

**关键推论**:驻留改 u16 只省"编辑期"的 367MB,**加载瞬间的 1.6GB 峰值分毫未减——61MP 照死在加载**。治峰值需动 rawler 校准/裁剪的拷贝语义,而这与"换掉 f32"是两件事:两个函数 in-place 化(不改任何类型)即可把峰值压到 ~1.1GB(demosaic 阶段成为新峰值:克隆 122 + Monochrome 245 + ThreeColor 734)。

**3. 选项评估**

| 方案 | 改动面 | 收益 | 判断 |
|---|---|---|---|
| A. 全管线换 u16/f16 | rawler fork(负值→偏置编码约定贯穿 demosaic/校准)+ 下游 15+ 调用点全改型 + 画质回归风险(u16 阴影 banding;f16 十位尾数在色彩矩阵连乘后误差累积) | 峰值与驻留都减半 | ✗ 不推荐:手术最大、与桌面版永久分叉,同额收益用 C/D 小手术同样可得 |
| B. 驻留瘦身+按需重开发 | `AppState.original_image` 模型:保留 u16(367MB)或 CFA(122MB),编辑用现成 preview,导出/ROI 时释放编辑态后单次重开发(~1.4s@61MP) | 驻留 -367~-612MB;**不解决加载峰值** | ○ 中期架构项,需与 C/D 组合 |
| C. 低内存设备降分辨率编辑模式 | 编排层:复用现成 `fast_demosaic` 旋钮(`get_fast_demosaic_scale_factor`,`raw_processing.rs:274`,已有 0.5/0.25 档;0.5×线性=15.3MP→f32 184MB,加载峰值 ~0.3GB)+ 导出时全分辨率重开发 | 4GB 设备 61MP **可编辑、可全分辨率导出**,PSS ~1.0GB | ✓ 推荐先行:不动 rawler、不动下游类型,最小手术 |
| D. 加载前资源预检+可见降级提示 | 复用导出路径资源感知先例(`Batch Export: N cores, X GB free RAM -> M threads`) | 消灭"闪退"观感 | ✓ 配合 C,几十行 |

中间档(若必须全分辨率进低内存设备):校准/裁剪 in-place 化小 fork,~50-100 LOC,加载峰值 1.59→1.1GB,61MP 编辑会话 PSS 落 ~1.5-2.0GB——真机 4GB 大概率能过,x86 模拟器自身开销大、仍临界。此类 PR 可推回 rawler 上游,维护成本最低。

**4. 边界与结论**

- **换掉 CPU 侧 f32 救不了全分辨率导出**:45MP 导出死在 GPU 侧全分辨率纹理(`Creating new GPU Processor for dimensions up to 8448x5632` 后 `onAbilityDied`),与 CPU 缓冲类型无关;导出解法是**输入纹理分块**(GPU 渲染已有 TILE_SIZE=2048 分块,但输入上传仍是整图),独立工作项
- **f32 管线在真机不是缺陷**:8~16GB 设备 61MP 峰值 ~2.4GB 本来能过;4GB 模拟器低于上游设计基线(上游 README:16GB recommended)。C+D 是为低内存环境补的适配层,不是管线重写的理由
- **建议路径**:短期 C+D(编排层,零 fork,4GB 设备立刻可用)→ 中期 B + in-place 小 fork(可推回上游)→ A 明确不做

### 6.12 Phase 4 前置:模型下载"权限问题"根因与 AI 运行时管线打通(2026-10-04)

**问题**:模拟器实测触发 AI 功能,模型下载完整落盘前报权限错误(用户所见"权限问题")。根因不在网络也不在目录创建:`persist_downloaded_asset` 的 `fs::rename` 在 OHOS FUSE 用户文件挂载(app_data_dir 落于 `/storage/media/100/.../Docs/.local/share/<bundle>`)上返回 `Permission denied (os error 13)`——同目录 rename 也拒,且 shell 上下文 `mv` 却可用(FUSE 按会话管控)。SAM encoder 100,293,382 B 完整下载后死在 rename,临时文件 `.sam_*.download` 冻结。

**修复 1(ai_processing.rs `persist_downloaded_asset`)**:fsync 降级 best-effort(下载后 sha256 复验使其对正确性冗余)+ rename 失败回退纯字节拷贝(`File::create`+`io::copy`,只用已验证可行的读写路径,避开元数据操作)。验证:5 个模型(SAM enc/dec、u2net、skyseg、depth,合计 ~560MB)全部落盘,3 次 rename EACCES 均由拷贝回退救回(app.log WARN 铁证);skyseg 落盘时 rename 又恰好成功——FUSE 行为不稳定,回退必须常驻。

**修复 2(ORT 运行库从未进 HAP——连带缺口)**:`ohrs build --dist entry/libs` 会**清空目标 ABI 目录**再放入 cargo 产物,手动放置的 .so 活不过构建;而 ort 为 load-dynamic(运行时按裸 soname `libonnxruntime.so` dlopen,见 lib.rs ORT_DYLIB_PATH 设置)。症状:模型全部就绪后 `Session::builder` PANIC `Error loading shared library libc++_shared.so (needed by .../libs/x86_64/libonnxruntime.so)`。修复 = `build-ohos.ps1` 新增打包后注入阶段:从 `src-tauri/libs/ohos/<abi>/` 注入 libonnxruntime.so + NDK 对应 ABI 的 `libc++_shared.so`(ORT 的 DT_NEEDED;cmake 只给 arm64 出了一份);`-SkipBuild` 开关可单独重注入。x86_64 ORT v1.28.2 取自 csukuangfj/onnxruntime-libs "ohos" release(与 ort rc.10 绑定经 C API 版本协商向后兼容);build.rs 查找路径由硬编码 arm64-v8a 升级为按 ABI 映射(aarch64→arm64-v8a / arm→armeabi-v7a / x86_64→x86_64)。

**已知边界(非缺陷,与 6.10 同构)**:4GB 模拟器装不下 5 模型 AI 蒙版栈——SAM enc+dec+u2net+skyseg+depth 会话 ≈560MB + 编辑器基线,大图小图两次触发 LowMemoryKill(hilog 20:49:40 / 20:55:54);真机 8GB+ 在预算内。

**端到端实证(NIND AI 降噪,单模型路径)**:124MB NIND 从 hf-mirror 下载→FUSE 持久化→ORT dlopen(注入后无 PANIC)→分块推理→前后对比 UI→保存 `small_test_Denoised.png`(1.08MB)+ .rrdata 落盘,全程无崩溃。**AI 功能在模拟器经单模型路径全通;5 模型蒙版路径待真机**。

### 6.13 真机渲染全链路修复——四层根因逐一击破(2026-10-05)

**结论:真机(HAD-W32 平板 / Kirin + Maleoon 916 / HarmonyOS 7.0.0.17 log 版)编辑器全链路渲染打通**——Vulkan 后端 + 沙箱路径 + asset 协议三层修复后,24/33MP ARW 画布正常出图、库/胶片栏缩略图全真图,交互预览 ~290ms/趟,三轮装测零 faultlog。问题为四层嵌套,黑屏主因(第 3 层)与 GPU 无关。

**1. 四层根因与修复清单**

| # | 断点 | 证据 | 修复 |
|---|---|---|---|
| 1 | GLES 转译层崩溃:Maleoon `libhvgr_v210.so` 在 GPU 回读路径栈溢出 | faultlog cppcrash-24077/32612:`SIGSEGV@0x0` ← `__stack_chk_fail` ← libhvgr ← `wgpu_hal::gles::Queue::submit` ← `read_texture_data_roi`(gpu_processing.rs 的 `copy_texture_to_buffer`+`queue.submit`);同栈两次复现 100% | `gpu_processing.rs`:OHOS 无显式 `WGPU_BACKEND` 时默认后端改 `VULKAN \| GL`(真机命中 Vulkan,模拟器无 Vulkan 落 GL;显式用户设置仍最高优先) |
| 2 | Vulkan 加载不了:ash 上游在"非 android 的 Unix"分支找 `libvulkan.so.1`,OHOS 只装 `/system/lib64/libvulkan.so`(ICD `vulkan.hvgr_v210.so`,Vulkan 1.3.231,原生驱动健在) | app.log `Failed to find a wgpu adapter: ... vulkan drivers/libraries could not be loaded`;ash 0.38.0 entry.rs:74 cfg 链实锤(OHOS=`target_os=linux` 非 android → 命中 `.so.1` 分支) | **vendor ash**:`src-tauri/vendor/ash/`(0.38.0+1.3.281 原样拷贝,仅 entry.rs 增加 `all(unix, target_env="ohos")` → `libvulkan.so` 分支)+ `[patch.crates-io] ash = { path = "vendor/ash" }`;Cargo.lock 仅 ash 块 2 行变更;其他平台 cfg 零影响 |
| 3 | **黑屏主因(与 GPU 无关)**:fork 用桌面 XDG 语义解析 app 路径(`path/desktop.rs`,OHOS 命中 `not(target_os="android")` 分支)→ `app_data_dir` = `$HOME/.local/share/<bundle>` 落 FUSE 用户存储 `/storage/Users/currentUser/...` → 真机写入 **EPERM**(模拟器恰好可写,故模拟器全通)→ settings 载入失败 → 前端拿不到 `editorPreviewResolution` → `previewSize={0,0}` → **编辑器根本不创建 canvas**(useImageLoader.ts:75-76);后台索引/缩略图/LUT 列举同源阵亡 | app.log 三连:`Failed to load/save settings` + `Failed to start background indexing` + `Failed to list LUTs`(均 EPERM);CDP 页面内实测 `apply_adjustments` invoke 返回合法 JPEG(FF D8 FF E0)且 `createImageBitmap` 成功——GPU/IPC/解码全通,矛头锁定前端状态机;DOM 探针 0 个 `<img>`/canvas | **新建 `src-tauri/src/app_paths.rs`**:OHOS 直连应用沙箱(`files/appdata`、`files/config`、`files/logs`、`cache` 四个映射,基根 `/data/storage/el2/base`,返回前 `create_dir_all`),其余平台原样委托 tauri resolver;全仓 **20 处调用点**机械替换(lib.rs×8、file_management×6、lut_processing×3、app_settings/ai_processing/ohos_integration 各 1);`setup_logging` 的 OHOS 内联分支统一收编 |
| 4 | 缩略图写入沙箱后,前端 `convertFileSrc` 加载被 asset 协议拒绝(scope 只有 `$APPCACHE/thumbnails/*`,而 `$APPCACHE` 经 fork 解析仍指 FUSE 路径) | app.log `asset protocol not configured to allow the path: /data/storage/el2/base/cache/thumbnails/<hash>_small.jpg` | `tauri.conf.json` `assetProtocol.scope` 追加字面路径 `/data/storage/el2/base/cache/thumbnails/*`(其他平台该路径不存在,无副作用) |

**2. 真机终验数据(17:20 构建)**

- 适配器:`Using GPU adapter: Maleoon 916 (backend: Vulkan, type: IntegratedGpu)`;downlevel 仅缺 `SURFACE_VIEW_FORMATS`(本管线为无 surface 的 compute+回读,不受影响);loader 警告 ICD 接口 v3<5(LDP_DRIVER_7,非阻塞)
- 渲染:24MP(DSC0009)经 CDP 页面内实测 invoke→JPEG→解码 OK;33MP(DSC06677)编辑器画布出图;库 2×2 网格 + 胶片栏 4/4 真实缩略图;设置持久化/会话恢复正常(重装后"继续会话"页出现)
- 性能:RAW 解码 24MP 522ms(模拟器 5.97s 的 1/11);交互预览 1024x683 **286~353ms/趟(3.45~3.50 FPS)**;缩略图 1280x853 480~580ms;全预览作业 648ms;**安装后首趟 ~11s 为 WGSL 管线编译一次性开销(稳态亚秒;当日已由启动期管线预热解决,见遗留清单)**
- 稳定性:三轮构建装测零新 faultlog(最新崩溃档仍停在 GLES 时代 14:42);全会话仅 1 条良性 ERROR(重装后授权恢复尝试)

**3. 调试方法学(真机排障复用价值高)**

- **ArkWeb DevTools 直连(本轮定位黑屏的决定性手段)**:`hdc -t <key> fport tcp:9222 localabstract:webview_devtools_remote_<pid>`(socket 名 `cat /proc/net/unix` 可见;pid 用 `ps -ef | grep -i rapidraw`——comm 15 字符截断,`pidof` 全名查不到)→ Node ≥22 原生 WebSocket 走 CDP(`/json/list` 取页面 WS URL),`Runtime.evaluate` 在页面内实测 `__TAURI_INTERNALS__.invoke` 返回类型/JPEG 魔数/`createImageBitmap`/DOM 状态
- **uitest dumpLayout**:可暴露 webview 内 DOM(元素属性 `originalText` + `origBounds` `[x,y][x,y]` 两角格式),但**不稳定**(时有时无);PowerShell 5.1 读取须 `[IO.File]::ReadAllText(path, UTF8)`(默认 GBK 会乱码导致中文关键词搜不到);坐标提取正则须锚定 `origBounds` 紧邻 `originalText` 的属性顺序
- **像素扫描兜底**:系统原生对话框(非 webview)dump 抓不到时,用 System.Drawing 阈值扫描(暗面板中最宽纯白矩形 = 主按钮;底栏蓝色块 = 确认按钮)求中心;视觉模型坐标估计不可靠(同图两次估计可差 900px)
- **真机日志**:`/data/app/el2/100/base/<bundle>/files/logs/app.log`(`setup_logging` OHOS 直写沙箱,宿主 shell 可读;stdout/stderr 真机上接 /dev/null)
- **注意**:picker 目录授权在真机上默认为**进程会话级**(任何进程死亡即失效——`install -r`、force-stop、标题栏优雅关闭皆然;模拟器才跨重启持久),失效后"继续会话"由探针自动捕获并重弹 picker 自愈(见 6.14)。**深夜补充:声明 `FILE_ACCESS_PERSIST` 后 2in1/PC 形态经 v3 持久化跨 force-stop/重启存活(见 6.14 第 5 节)——本条会话级语义适用于 pad 形态与未声明权限的构建**

**4. 遗留与后续**

- ~~首趟 WGSL 管线编译 ~11s~~ → **已解决(2026-10-05 晚,启动期管线预热)**:`gpu_processing.rs` 新增 `warm_up_pipelines()`(OHOS 门控,初始化 GPU 上下文 + 创建 256×256 一次性 GpuProcessor 触发全部 5 条管线编译后丢弃——管线与处理器尺寸无关,真实处理器按需创建时命中驱动热缓存),`lib.rs` 启动时经 `pipeline-warmup` 后台线程调用。真机冷缓存(卸载重装)实测:预热 9.98s 在启动后台吸收,首个真实 GPU 作业 588ms、编辑器首开全预览 578ms(修复前 11.16s/11.49s),稳态 ~250ms/趟(4+ FPS);日志标记 `[pipeline-warmup] completed in Xs`
- ~~ArkWeb `window.localStorage` 为 null~~ → **已解决(2026-10-05 晚,见 6.14)**:新增 `webStorageShim.ts` 启动期以 `Object.defineProperty` 装内存版兜底(CDP 实证 ls/ss=object、读写往返 ok、`Object.keys` 不再抛);前端本身零 localStorage 使用(zustand 无 persist、i18next 无检测器),兜底面向未来代码与三方库
- 模型下载目标随 app_paths 迁至沙箱后,FUSE rename EACCES 场景消失,但 `persist_downloaded_asset` 的字节拷贝回退**必须保留**(AGENTS.md 纪律,对旧数据/异常环境冗余)
- ≥60MP 真机基线(6.10/6.11 方法论就绪,61MP 资产已推真机 Download)、ORT/AI 5 模型蒙版真机分级验证仍待跑
- 模拟器回归冒烟:x86_64 新构建重装验证一轮(沙箱路径模拟器同样可写,风险低)

### 6.14 会话恢复授权自愈 + ArkWeb 存储兜底 + 授权持久化 v3(2026-10-05)

**背景**:6.13 遗留清单的两项真机日常使用之痛——重装/重启后"继续会话"死路(空库且无提示),与 ArkWeb `localStorage` null 隐患。修复后真机四轮端到端验证全通(18:48 构建,18:49-19:12 实测)。

**1. FileKit 目录授权的真实语义(真机实证,修正 6.6 模拟器结论)**

- DocumentViewPicker(FOLDER 模式)授予的目录访问权在真机上是**进程会话级**:任何进程死亡都失效——标题栏 X 优雅关闭(19:10 实验:dumpLayout 定位 `EnhanceCloseBtn` → 点击 → `pidof` 确认进程退出 → 重启后启动预载与探针双双 EPERM)与 `aa force-stop`(18:57 实验)结果相同;模拟器(x86_64 标准镜像)才跨重启持久。**(本条为 v2 时代结论,系未声明 `FILE_ACCESS_PERSIST` 时的默认语义;声明该权限后 2in1/PC 形态经 v3 持久化,force-stop/优雅关闭/整机重启均不再失效——见第 5 节;pad 形态与未声明权限的构建仍适用本条)**
- 授权失效表现为**元数据可读 + `read_dir` EPERM**(`Operation not permitted (os error 1)`);而 `scan_dir_lazy`(file_management.rs)吞掉 `read_dir` 错误返回空列表 → 树/children 扫描命令对死根无感知(仍返回"空但合法"的树节点)——探针因此不能复用扫描命令

**2. 修复设计(探针 + 重授权流 + 双兜底)**

| 层 | 文件 | 内容 |
|---|---|---|
| Rust 探针 | `file_management.rs` 新增 `check_paths_readable(paths) -> Vec<bool>`(lib.rs 注册) | 裸 `read_dir().is_ok()`,权限失败必然传导。首轮方案用 `get_folder_children` 探活,因 `scan_dir_lazy` 吞 EPERM 把死根误判为活(教训:探针语义必须与扫描语义解耦) |
| 前端重授权流 | `useAppNavigation.ts` `handleContinueSession` | 恢复前 OHOS 门控探活;发现死根 → console.warn 留痕 + toast 提示 + 直弹 `pick_ohos_folder`;重选 → 合并根目录(同路径=授权刷新/不同路径=换根,前缀判定)+ **作废启动预载的陈旧空树 promise**(`preloadedDataRef.current = undefined`,否则同路径重选会消费已 resolve 的空树)+ 持久化 rootFolders → 走正常恢复;取消且无活根 → toast + 回欢迎页;部分活根 → 只恢复活的 |
| 预载兜底 | `useAppInitialization.ts` | 预载 images promise 加 `.catch(() => undefined)`:死根时该 promise 无人 await 即 reject,修复前每次启动产生 1 条 unhandled rejection ERROR 日志;消费方对 undefined 走全新加载,行为不变 |
| ArkWeb 兜底 | 新增 `src/utils/webStorageShim.ts`(main.tsx 首行安装) | ArkWeb 的 `localStorage`/`sessionStorage` 为 null,任何访问(含 `Object.keys`)抛 TypeError;shim 探测不可用后以 `Object.defineProperty(window, ...)` 装内存版(非持久) |

**3. 真机验证矩阵**

- **重授权主流程 ×3 全通**(三种触发:install -r 清授权 / force-stop 重启 / 优雅关闭重启):探针 console.warn 留痕(`Library roots unreadable (FileKit grant revoked?), requesting re-grant: [...]`)→ toast → FileKit picker 弹出 → 重选 Download → 库恢复(3 ARW + 1 PNG 缩略图全渲染、零错误)
- **取消分支**:picker 取消 → 回欢迎页不卡死,可再次触发(第二轮即恢复)
- **unhandled rejection**:修复前每次死根启动 1 条 ERROR,修复后 0 条
- ~~**已知边界**:**每次冷启动需一次重授权点击**(授权为进程会话级)~~ → **已由 v3 解决(2in1/PC 形态,同日深夜,见第 5 节:picker 授权经 `fileShare.persistPermission` 持久化 + 启动期 activate,跨 force-stop/整机重启直通恢复)**;pad 形态保留本边界(设备能力所限,v2 重授权流即其最终形态)

**4. 调试方法学补充(相对 6.13-3)**

- **dumpLayout 保存路径带时间戳**(`layout_<n>.json`),须解析命令输出里的实际文件名再 `file recv`——直接收 `layout.json` 会拿到陈旧文件
- **系统装饰栏按钮 id**:`EnhanceCloseBtn`/`EnhanceMinimizeBtn`/`EnhanceMaximizeBtn`(dumpLayout 精确定位;本日两次实证视觉模型坐标估计误差分别达 235px 与 1290px,系统 UI 点击一律 dumpLayout 取 bounds)
- **CDP 点击 React 按钮**(`querySelector` + `.click()`)零坐标误差,适用于 webview 内元素;系统模态对话框不在 webview 内,仍须 uitest + dumpLayout
- 截图体积特征可做快速状态判别(本机):欢迎页 ~346KB / 系统对话框开启 ~276KB / 库网格渲染 ~331KB

**5. v3:picker 授权持久化(2in1/PC 形态)——pad/PC 双形态定稿(2026-10-05 深夜)**

**需求**:同一 HAP 同时覆盖 pad 与 PC(2in1),按设备形态区别处理文件持久化权限——PC 免重授权直通恢复,pad 保留 v2 冷启动重授权(能力差异,非选择)。

- **设计(ArkTS-only,`EntryAbility.template.ets` 三方法,Rust/前端零改动)**:
  - `persistFolderGrant(uri, path)`:`pickFolder` select 成功回调调用(外层 try/catch 防同步抛跳过 `resolvePickFolder`);`deviceInfo.deviceType === '2in1'` 门控(非 2in1 留 console 痕迹后跳过);`fileShare.persistPermission([{uri, operationMode: READ_MODE | WRITE_MODE}])`——读写模式必须:`.rrdata` 侧车写在图库夹原图旁(parse_virtual_path)
  - `recordFolderGrant(uri, path)`:persist 成功后把 `{uri, path}` 追加进 `${this.context.filesDir}/ohos_grants.json`(JSON 数组按 uri 去重;openSync `READ_WRITE|CREATE|TRUNC`)
  - `activatePersistedGrants()`:`onCreate` 火忘重放(逐 URI try/catch 隔离;不 await——6.4 教训:生命周期内 await 辅助调用曾致 ability 终止)
  - 全链失败非致命 → 自动落回 v2 探针/重授权流(第 2 节);pad 形态即始终走 v2
- **权限与路径真相(实证,含两个推翻性结论)**:
  - **`FILE_ACCESS_PERSIST` 仅声明即可**:module.json5 声明后 persistPermission 即成功(hilog `[RapidRAW] persistPermission ok for file://docs/storage/Users/currentUser/Download`)——空 `allowed-acls` 调试档案**不拦**(他包档案普遍含该 ACL 曾误导出"须 AGC 重发档案"假说)——**官方根源:该权限 API 11 为 system_basic 受限级,API 12 起降为 normal 级 system_grant**(restricted-permissions.md 变更记录),无需 ACL 是制度性的、非调试档案宽容;`bm dump reqPermissionStates:[0,0]` 与 `atm dump grantStatus:0/flag:4(SYSTEM_FIXED)` **均不反映运行时拒绝**——校准法:INTERNET 同显 0 而 webview no-cors fetch 实通
  - **`UIAbilityContext.filesDir` 是 hap 级路径**:`/data/app/el2/100/base/<bundle>/haps/entry/files/`,**≠** Rust/Tauri 侧应用级 `files/`(app.log 所在)——grants 文件落在 hap 级路径;曾以应用级路径 cat 误判"记录失败"空追一轮
- **持久化语义(真机实证,20:29 构建)**:pick → persist ok → 记录落盘;**force-stop → 重启 Continue Session 直通恢复**(无 picker 无重授权);**整机 reboot(锁屏解锁后)→ 同样直通**——临时授权必死于重启,唯 persist+activate 链可活,故 reboot 通过即最深证明(判定证据 = app.log 无 re-grant WARN + 截图视觉确认库网格)。`install -r` 对已持久化授权的行为未测(历史杀授权案例均为未声明权限的构建;即便吊销,v2 流自愈)。**文档矛盾注**:官方 select-user-file.md 称 select() 返回"临时只读授权",与真机实证矛盾——本机 FOLDER 模式 READ|WRITE 持久化成功(hilog 实证)且历次会话 `.rrdata` 侧车原图旁写入正常,以行为为准
- **deviceType 门控实证**:`param dump` → `const.product.devicetype=2in1`(HAD-W32);UA `Mozilla/5.0 (PC; OpenHarmony 7.0) ...`;pad 按官方文档报 `tablet` → gate 走 v2(fail-safe:一切非 2in1 形态保守重授权);**官方旁证**:tablet 最小 syscap 集不含 FolderAuthorization(tablet-syscap-list),且 persistPermission 的"仅 2in1"措辞在 OpenHarmony 6.0/master 已放宽为 syscap 门控(5.1.0 尚存)——pad 无持久化能力系系统配置所限,未来 pad 机型若配备该 syscap 可升级为 `canIUse` 探测
- **双形态矩阵**:

| 形态 | deviceType | 持久化策略 | 冷启动行为 | 验证状态 |
|---|---|---|---|---|
| PC/2in1 | `2in1`(param 实证) | persistPermission(READ\|WRITE)+ hap 级记录 + onCreate activate | Continue Session 直通(零 picker) | 真机端到端:pick → force-stop → 整机 reboot 全通(20:30-21:23 实测) |
| pad/tablet | `tablet`(文档值) | gate 跳过(留痕日志) | v2:探针失败 → toast → 重授权 | v2 流真机 ×3(本机);真 pad 待验 |

- **gen/ 手动复制纪律(两处,`ohos init` 重跑后必查)**:① `cargo tauri ohos build` **不复制** `src-tauri/ohos/*.template.ets` 进 gen(仅 `ohos init` 复制且其会重置签名配置)——模板改动须手动 copy + 字节级 diff 校验;② module.json5 的 `FILE_ACCESS_PERSIST` 声明也只存在于 gitignored 的 gen/ ——init 重跑后须重加

**6. 调试方法学补充二(v3 排障沉淀,接第 4 节)**

- **dumpLayout 跨进程窗口盲区(重大,曾致连续三轮误诊)**:默认 `uitest dumpLayout` 只合并**本应用**窗口;系统 FileKit picker 渲染于独立窗口(com.huawei.hmos.filemanager),**须 `dumpLayout -i`(不合并模式)才可见**——"默认 dump 看不到 picker"≠ picker 没开;picker 元素提取须按 hostWindowId 过滤(库内脚本 `extract-picker.js` 模式)
- **hilog 与 ArkTS console**:app 域 tag 为 `A03D00/<bundle>/JSAPP`(info/error 均达 hilog);但**应用自身启动窗口的 JSAPP 行会被自身 chromium 洪流流控丢弃**(他进程 JSAPP 行可见,证明机制本身正常)——启动期证据以行为学为准(app.log + 截图);grep 模式必须对准实际日志文本(如 `persistPermission` 匹配不到 `recordFolderGrant failed` 行)
- **截图体积签判(补充第 4 节)**:全屏态:欢迎 ~346KB / picker ~277-280KB / 库 ~313-322KB / **锁屏 ~570KB(新增)**;**窗口非全屏或系统弹窗悬浮时整体偏移**(reboot 后窗口化 + "USB 连接方式"对话框悬浮 → 281K/258K)——跨重启场景先截图视觉定标再比字节
- **锁屏/reboot 测试 runbook**:reboot 后设备必锁屏;el2 用户存储解锁前不可访问(app.log `No such file` ≠ 文件丢失);锁屏期 `aa start` 拒启(**10106102**,developer mode 不自动解锁);**锁屏窗口对 dumpLayout 零节点暴露**(安全特性)→ 交互靠截图+视觉定位(本机:密码框中心 (1560,1750)、眼睛图标 [1718,1715][1758,1765]、圆形提交钮 [1800,1705][1872,1777] 中心 (1836,1741));**keyEvent 键码权威值(`@ohos.multimodalInput.keyCode`):数字 0-9 = 2000-2009(KEYCODE_1=2001)、字母 A-Z = 2017-2042、ENTER = 2054**——错误记忆 2019 实为字母 C,曾打出 "CCCCCC" 提交被拒;锁屏拒绝**无任何错误提示**(静默清空字段),靠"6 圆点→空字段+零报错"指纹定位

## 7. 移植路线图与进度清单

### Phase 0 — 技术验证(1~2 周)
- [x] 全量平台门控审计与修正(本分支)
- [x] `ohos_integration.rs` 骨架
- [x] 宿主平台 `cargo check` 无回归验证(Rust 1.99.0 MSVC + CMake 4.4.3 已安装)
- [x] `rustup target add aarch64-unknown-linux-ohos`(目标 std 已就绪)
- [x] OpenHarmony SDK 就绪(DevEco Studio 自带,API 26):clang 包装脚本(`~/.cargo/bin/aarch64-unknown-linux-ohos-*.cmd`)、本地链接器配置(`src-tauri/.cargo/config.toml`)、CMake 工具链文件(`src-tauri/ohos/ohos-toolchain.cmake`,已提交)全部就位
- [x] `cargo check --target aarch64-unknown-linux-ohos` 通过(核心依赖探针工程全量通过,详见 6.1;主仓本体被上游 tauri 桌面栈阻断,属预期的 Phase 1 边界)
- [x] 用 HAP 出包全流程验证(2026-10-03:已由本仓直接跑通端到端出包,见 6.3;tauri-demo 探路不再需要)

### Phase 1 — 构建管道(2~3 周)
- [x] 接入 tauri-cli `feat/open-harmony` 分支:`cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony`(v2.11.4,含 `ohos` 子命令)
- [x] `[patch.crates-io]` 指向 wry/tao/tauri 的 ohos 分支(实际方案:补丁直接提交进主 `src-tauri/Cargo.toml` + vendored openharmony-ability 入库——本仓即 OHOS 构建工作区,独立清单反而割裂;风险由 Cargo.lock 冻结与 6.2 风险登记控制)
- [x] 自备 OHOS 版 `libonnxruntime.so` 放入 `src-tauri/libs/ohos/arm64-v8a/`(v1.26.0;目录 gitignored,各构建机自备)
- [x] 主仓 `cargo check --target aarch64-unknown-linux-ohos` 全量通过 + 宿主无回归(见 6.2)
- [x] `cargo tauri ohos init` 生成 `gen/ohos` 工程(2026-10-03;产物 gitignored,未引入跟踪文件变更)
- [x] lensfun_db / resources 打包进 HAP(2026-10-03:include_dir 嵌入 .so 随 HAP 打包,镜像 Android 路径,commit `0b11a54a`;宿主+OHOS 双目标 cargo check 通过;HAP 体积已复验(2026-10-03 22:53 全链路重建 EXIT=0):52.4→57.4MB,+5.0MB 与 lensfun_db 4.99MB 载荷吻合,`<lensdatabase` 标记已在 .so 二进制内确证。`src-tauri/resources/` 仅含 gitignored 的 ORT 二进制,无需打包;OHOS 版 `libonnxruntime.so` 走 `libs/ohos/` 各机自备)
- [x] hvigor 出包:未签名 HAP 已产出(2026-10-03,52.4MB,见 6.3;自动化脚本 `scripts/build-ohos.ps1`)
- [x] 模拟器点亮应用窗口(2026-10-03 23:25,DevEco x86_64 模拟器:完整欢迎页 UI 渲染、Canvas 图片正常、`tauri` 自定义协议注册、无崩溃,超预期完成,见 6.4;真机仍待接入)

### Phase 2 — 平台集成(3~4 周)
- [x] FileKit 文件夹选择桥(2026-10-04,见 6.6):"打开文件夹"端到端可用——DocumentViewPicker 桥(oneshot+napi 回传)+ `isOhos` 探测 + `handleOpenFolder` OHOS 分支;`FileUri.path` 直出沙箱路径、Rust 可读(stat 实证)、~~跨重启持久~~ → 会话级修正(2026-10-05)→ **2in1 形态 v3 恢复跨重启持久(2026-10-05 深夜,fileShare.persistPermission + 启动期 activate,见 6.14 第 5 节;pad 形态走 v2 重授权自愈)**;photoAccessHelper 相册 URI 桥(单文件/相册)已由 6.7 覆盖(导入走 FileKit picker 可导航相册,导出走 showAssetsCreationDialog)
- [x] 相册导出 `save_image_bytes_to_ohos_gallery`(2026-10-04,见 6.7):showAssetsCreationDialog 弹窗授权 + 沙箱中转 + fd 拷贝端到端实证(`save_to_gallery ok: file://media/Photo/…`,图库「来自应用 RapidRAW」可见)
- [x] FileKit 文件选择桥 + 导入流(2026-10-04,见 6.7):`pick_ohos_files`(`collapse_suffix_filters` 规避 100 字符上限)+ 导入 FAB/图像选择器/LUT/预设消费点全接;Rust `std::fs` 直读媒体库路径实证
- [x] TLS 根证书策略已确认(2026-10-03):维持 reqwest rustls 捆绑 webpki 根——rustls-platform-verifier 无 OHOS 后端,捆绑根对 HF/ohpm 端点足够;真机 TLS 握手验证顺延至 Phase 3/4 有设备时
- [x] tauri-plugin-dialog 其余消费点接桥(2026-10-04,见 6.7):文件选择(LUT 导入/预设导入/图像选择器)走 `pick_ohos_files`,另存为走 `pick_ohos_save_file`(`save_file_as` 设备端验证顺延)——文件夹选择已由 6.6 桥替代;plugin-fs 无前端使用,无替代需求
- [x] 深色模式/安全区/返回手势等系统 UI 适配(2026-10-04,见 6.8):主题"系统"选项(14 locale)+ `resolveThemeId`/`isSystemDark` + `onConfigurationUpdate`→napi→事件链(深浅双向实时翻转实证);返回手势页面级 `onBackPress` 桥(编辑器退库/模态关闭优先/库退主页三分支实证,注入用 `uitest uiInput keyEvent Back`);avoid area 日志;EntryAbility/Index 模板固化入 `src-tauri/ohos/`
- [x] 窗口控制可用(2026-10-04,见 6.5):系统装饰栏原生提供 最小化/最大化/关闭;自绘标题栏保留(标题/拖动),拖动经 Rust↔ArkTS `startMoving` 桥(实测 +400px 精确);OHOS 隐藏自绘三按钮——系统装饰输入矩形残留发现,原"隐藏 decor 独占"方案(2026-10-03)已反转(历史与教训见 6.4)
- [x] 应用图标与 label 对齐其他平台:entry 字符串资源 + layered_image 双层图标 + startIcon,均取自 `src-tauri/icons/full_res_original.png`(2026-10-03,见 6.4)。**注:以上 EntryAbility/字符串/图标三处改动均位于 gitignored 的 `gen/ohos/`,`cargo tauri ohos init` 重跑后需按 6.4 重打**

### Phase 3 — 渲染验证(1~2 周)
- [x] compute-only + IPC 回读路径点亮编辑器(Android 同款,零新增风险)(2026-10-04,见 6.8):Mali-G77 GLES 后端实测点亮——GPU Processor/apply_adjustments/estimate_export_size/generate_thumbnail_data 全链路日志铁证,交互 ROI ~29 FPS,曝光编辑视觉生效;`Fake map` 为回读路径良性产物
- [x] 评估 wgpu GLES surface 直渲(XComponent + `OhosNdkWindowHandle`)提性能(2026-10-04,见 6.9):结论 **GO(有条件)**——上游 wgpu ≥25 原生支持 OHOS GLES(本仓 29.0.4 无需 fork),rwh 0.6.2 `OhosNdkWindowHandle`/tao fork/`tauri::ohos::APP` 链路就绪;风险 gfx-rs/wgpu #9158(GLES sRGB,OPEN)+ webview 层级合成;估算 1~2 周 / 300~600 LOC;建议真机基线后按数据决策立项
- [ ] 高像素 RAW(≥60MP)真机性能与内存基线(**模拟器基线已完成** 2026-10-04,见 6.10:61MP 编辑器 LowMemoryKill / 45MP 编辑器全通·交互 14~29 FPS·PSS 1.76GiB / 45MP 全分辨率导出进程死亡;交互 FPS 与源 MP 无关、GPU Processor 按预览尺寸创建为结构性结论;真机复测待硬件,资产与方法学已固化;**缓解方向评估已完成**,见 6.11——短期降分辨率编辑+预检 / 中期驻留瘦身+in-place 小 fork / 全管线 u16/f16 替换否决)

### Phase 4 — AI 与发布(2~3 周)
- [ ] ORT 动态加载真机验证;AI 蒙版/降噪功能分级测试(**模拟器部分已完成** 2026-10-04,见 6.12:NIND AI 降噪端到端全通——下载/持久化/ORT dlopen/推理/保存;模型下载 FUSE rename EACCES 已由拷贝回退修复;5 模型蒙版栈被 4GB 模拟器 LMK 阻塞待真机;x86_64 ORT v1.28.2 经 build-ohos.ps1 注入 HAP)
- [ ] (可选)MindSpore Lite / NNRt NPU 路径评估(**前置调研已完成** 2026-10-04,见 `docs/MINDSPORE_LITE_NPU_EVAL.md`:结论 GO 基础上分模型——系统 MindSpore Lite Kit(`libmindspore_lite_ndk.z.so`,`OH_AI_*` C API,NNRT+CPU 逐算子回退)为推荐路径,Rust 绑定需手写(无现成 crate);converter_lite 2.10.0 离线转换;U2Net/skyseg/NIND 低风险、ViT 系需重导出、**LaMa 受 FFT 阻塞**;**全部验证需真机**——模拟器无 Kit/NNRT 支持,与 Phase 3-3 同一硬件阻塞)
- [ ] AGC 签名、AppGallery 上架(摄影类目)、版本通道(**签名已打通** 2026-10-05:sign-app/verify-app 全通、signed.hap 已产出;命令、证书材料与坑位记录在**未入库**的 `.csr/SIGNING.md`——签名材料含私钥,永不入库,`.csr/` 与 `*.p12/*.p7b/*.jks` 已加 .gitignore;上架待干净 aarch64 release 包重建 + 真机安装验证)

## 8. 环境搭建速查(Phase 1 参考)

```bash
# 1. Rust 目标
rustup target add aarch64-unknown-linux-ohos

# 2. OHOS SDK:从 OpenHarmony 发布页下载“标准系统公共 SDK”,解压后创建 clang 包装脚本
#    (见 https://doc.rust-lang.org/stable/rustc/platform-support/openharmony.html)
#    ~/.cargo/config.toml:
#    [target.aarch64-unknown-linux-ohos]
#    ar = ".../ohos-sdk/linux/native/llvm/bin/llvm-ar"
#    linker = ".../aarch64-unknown-linux-ohos-clang.sh"

# 3. Tauri ohos CLI(fork 分支)
cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony
cargo install ohrs

# 4. 初始化与构建
cargo tauri ohos init
cargo tauri ohos build -t aarch64
```

Windows 主机注意:OHOS NDK 的 clang 是 Unix shell 脚本,需手写 `.cmd` 包装(已完成,见 6.1);Rust 交叉编译、ohrs 构建与 hvigor HAP 打包均已在 Windows 主机全量打通(见 6.3)。

**Windows 主机 HAP 出包环境(已验证,2026-10-03;契约与陷阱详见 6.3;DevEco Studio 自带 ohpm/hvigor/jbr,系统 Node 即可)**:

```powershell
$dev = 'C:\Program Files\Huawei\DevEco Studio'
$repo = '<仓库路径>'   # 如 D:\workspace\RapidRAW-hmos
# 一次性:无空格 SDK 别名 + 伪根补 default(让 env.rs 的 parent³ 落点成为合法 SDK 根,见 6.3-1)
New-Item -ItemType Directory -Path "$env:USERPROFILE\ohos-devstudio" -Force | Out-Null
if (-not (Test-Path "$env:USERPROFILE\ohos-devstudio\sdk")) { New-Item -ItemType Junction -Path "$env:USERPROFILE\ohos-devstudio\sdk" -Value "$dev\sdk" | Out-Null }
if (-not (Test-Path "$env:USERPROFILE\ohos-devstudio\default")) { New-Item -ItemType Junction -Path "$env:USERPROFILE\ohos-devstudio\default" -Value "$dev\sdk\default" | Out-Null }
[Environment]::SetEnvironmentVariable('DEVECO_SDK_HOME', "$env:USERPROFILE\ohos-devstudio\sdk", 'User')
# 环境变量(OHOS_HOME 必须是 openharmony 目录——见 6.3 契约;DEVECO_SDK_HOME 供独立工具/DevEco 用)
$env:OHOS_HOME = "$env:USERPROFILE\ohos-devstudio\sdk\default\openharmony"
$env:OHOS_NDK_HOME = "$env:USERPROFILE\ohos-devstudio\sdk\default\openharmony\native"
$env:JAVA_HOME = "$dev\jbr"    # PackageHap 需要 java(hvigor spawn 裸命令,见 6.3-5)
$vscmake = 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake'
$env:Path = "$vscmake\CMake\bin;$vscmake\Ninja;$dev\tools\ohpm\bin;$dev\tools\hvigor\bin;$dev\jbr\bin;$env:USERPROFILE\.cargo\bin;C:\Program Files\nodejs;" + $env:Path
$env:CMAKE_TOOLCHAIN_FILE = "$repo\src-tauri\ohos\ohos-toolchain.cmake"
$env:CMAKE_GENERATOR = 'Ninja'
$env:ORT_SKIP_DOWNLOAD = '1'; $env:ORT_DYLIB_PATH = 'libonnxruntime.so'
# cc-rs(ring 等)必需——PATH 上的三连名包装脚本不够,须显式 env 变量:
$env:CC_aarch64_unknown_linux_ohos = "$env:USERPROFILE\.cargo\bin\aarch64-unknown-linux-ohos-clang.cmd"
$env:CXX_aarch64_unknown_linux_ohos = "$env:USERPROFILE\.cargo\bin\aarch64-unknown-linux-ohos-clang++.cmd"
$env:AR_aarch64_unknown_linux_ohos = "$env:USERPROFILE\.cargo\bin\aarch64-unknown-linux-ohos-ar.cmd"
# CN 网络:rustup 直连 static.rust-lang.org 极慢,走 USTC 镜像
$env:RUSTUP_DIST_SERVER = 'https://mirrors.ustc.edu.cn/rust-static'
rustup target add aarch64-unknown-linux-ohos armv7-unknown-linux-ohos x86_64-unknown-linux-ohos --toolchain 1.98  # cargo tauri ohos init 会装全套 OHOS 目标,慢网下先手动装
npm.cmd install                              # @tauri-apps/api 已在 package.json 锁 2.11,无需再手动对齐
# 在仓库根执行(勿在 src-tauri/ 下;beforeBuildCommand 在 CLI cwd 找 package.json):
cargo tauri ohos build -d -t aarch64
# 端到端直通(独立 hvigorw 不可行,WS 脐带,见 6.3-4);产物:
#   src-tauri/gen/ohos/entry/build/default/outputs/default/entry-default-unsigned.hap
```

**一键构建**:`scripts/build-ohos.ps1`(**入库版**,2026-10-05 起;仓库根由脚本位置推导,机器本地前置同上述环境;自包含上述环境;fail-fast 预检、看门狗超时、心跳进度、daemon 清理;用法见脚本头注释)。

## 9. 未完成工作盘点(2026-10-05)

> 全量盘点 §6/§7 的未勾选项、顺延项与已知限制。~~最大单一阻塞:无真机硬件~~ → **真机已到位且渲染全链路打通(6.13,2026-10-05)**,§9.1/9.2 的真机验证项不再被硬件阻塞;不依赖硬件、建议先行的工程项见 9.3。

### 9.1 路线图未勾选项(Phase 3 余 1 项、Phase 4 全部 3 项)

| 项 | 已完成部分 | 阻塞点 |
|---|---|---|
| ≥60MP 真机性能/内存基线(6.10/6.11) | 模拟器基线 + 缓解方向评估;**渲染管线真机已通(6.13:24/33MP 出图,Vulkan ~290ms/趟)**,61MP 资产已在真机 Download | 跑基线即可(方法学已固化于 6.10) |
| ORT 真机验证 + AI 蒙版/降噪分级测试(6.12) | NIND 降噪模拟器单模型端到端全通 | 待真机执行(23GB 内存充裕);注意模型下载目标已随 app_paths 迁至沙箱(6.13) |
| MindSpore Lite / NNRt NPU 路径(可选) | 前置调研完成(`MINDSPORE_LITE_NPU_EVAL.md`) | 全部验证需真机(模拟器无 Kit/NNRT);LaMa 受 FFT 阻塞、ViT 系需重导出 |
| AGC 签名、AppGallery 上架、版本通道 | **签名已打通**(2026-10-05:sign-app/verify-app 全通、signed.hap 产出,材料见未入库 `.csr/SIGNING.md`) | 待干净 aarch64 release 包重建 + 真机安装验证 |

### 9.2 被"无真机"阻塞的散布验证项

- 真机 TLS 握手验证(§7 Phase 2 备注:rustls 捆绑 webpki 根策略已定)→ 真机已到位待跑
- mimalloc OHOS 启用评估(§5 风险 8:已回退系统分配器)→ 真机已到位待跑
- `save_file_as`(.cube LUT / 预设另存为)设备端验证(6.7)→ 真机已到位待跑
- 模拟器 GPU 为宿主透传,性能绝对值不可外推(6.10 §5)→ **真机数据已补**(6.13:Vulkan 交互 ~290ms/趟)

### 9.3 不依赖真机、可立即推进

1. **低内存适配 C+D(6.11 推荐先行,零 fork)**:降分辨率编辑模式(复用 `fast_demosaic` 旋钮)+ 加载前资源预检 + 可见降级提示——4GB 设备 61MP 可编辑、可全分辨率导出的关键路径
2. **发布包瘦身(6.4-3)**:双 ABI .so 同包(strip 后 120.5MB),出真机/release 包前清 `entry/libs` 或配 abiFilters
3. 直方图 canvas 无选中时噪点/空白帧排查(6.7,疑似 GPU readback 或未初始化缓冲)
4. 批量导出逐张弹系统对话框 → 评估 `MediaAssetChangeRequest` 批量授权或收集式 UX(6.7)
5. 库视图导出成功无 toast;编辑器视图导出未复测(6.7)
6. 全分辨率导出输入纹理分块(6.11 §4:GPU 侧整图上传是 45MP 导出死因,独立工作项)
7. 中期 B:驻留瘦身 + rawler 校准/裁剪 in-place 小 fork(6.11,~50-100 LOC,可推回上游)
8. ~~`app_cache_dir()` OHOS 指向用户公共存储区的长期方案~~ → **已解决(6.13)**:`app_paths.rs` 将全部 app 路径 OHOS 直连 `/data/storage/el2/base/` 沙箱,不再经过用户存储 FUSE(rename EACCES 场景随之消失);ArkTS 沙箱中转现仅导出桥仍在用

### 9.4 条件性 / 低优先级 / 非本仓

- wgpu GLES surface 直渲 PoC(6.9):结论 GO(有条件),**真机数据已到(6.13)**——Vulkan 回读路径全帧 ~290ms@1024x683(ROI 交互真机未测,模拟器上限 ~29 FPS 参考),远低于 60 FPS 目标,按既定规则 PoC 可立项;**注意真机 GLES 不可用(转译层必崩),直渲 surface 必须走 Vulkan**;风险 wgpu #9158 sRGB + webview 层级合成
- 外观瑕疵(6.5):浅色主题系统条视觉断层、系统条应用名与自绘标题重复
- tao-ohos 窗口操作 stub(fork 级);`feat/open-harmony` 等上游合并后摘 `[patch]` 表(见 `Cargo.toml` 注释)
