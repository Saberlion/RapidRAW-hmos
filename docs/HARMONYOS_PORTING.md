# RapidRAW 鸿蒙(HarmonyOS / OpenHarmony)移植报告

> 分析日期:2026-10 · 分支:`feature/harmonyos-port`
> 状态:Phase 1 构建管道进行中 —— fork 补丁接入完成,主仓 OHOS 交叉检查与宿主回归双通过(2026-10-03,见 6.2);下一步 HAP 出包

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
| wgpu on OHOS | ✅ GLES 后端已进上游(PR #7085,2025-02,含 CI);Vulkan 经社区验证可用(需 ash fork,仅 aarch64/x86_64) | [wgpu#7085](https://github.com/gfx-rs/wgpu/pull/7085) |
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
7. **手机端性能(低,已验证)**:Android 已趟过高像素 RAW 的回读路径;华为 Maleoon GPU 走 GLES 预计同级。
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

**成果**:Rust 交叉编译全链路(vite 前端 → ohrs Rust 构建 → ohpm install)打通,`librapidraw_lib.so`(dev profile,399MB)落位 `gen/ohos/entry/libs/arm64-v8a/`。当前唯一阻断:hvigor 打包(见文末)。

**OHOS_HOME 契约**:cargo-mobile2 fork 的 `env.rs` 把 `OHOS_NDK_HOME = OHOS_HOME` 原样下发给子进程;fork 的构建把 Rust 交叉编译委托给 `ohrs build`,而 ohrs 以 `<OHOS_NDK_HOME>/native/llvm` 推导链接器/工具、`<OHOS_NDK_HOME>/native/sysroot` 推导 sysroot。故 **`OHOS_HOME` 必须是 SDK 根(`.../sdk/default/openharmony`),不是 native 目录**(两种指法曾分别导致链接器路径双 `native` 与 aws-lc-sys 找不到 clang)。`ohos-toolchain.cmake` 已改为容错两种 `OHOS_NDK_HOME` 约定(检测 `llvm/` 位置归一到 NDK 根),与 6.1 手工交叉检查共用一份工具链文件;ohrs 检测到 `CMAKE_TOOLCHAIN_FILE`/`CMAKE_GENERATOR` 已设置时会尊重现有值。

**空格陷阱**:ohrs 在 `<OHOS_NDK_HOME>/../hms` 存在时向 CFLAGS 注入 HMS include。DevEco 装于 `C:\Program Files\...`,cc-rs 按空白切分 CFLAGS,带空格的 include 被切成残参,ring/aws-lc-sys 全灭(clang: no such file or directory)。修复:为 SDK 建无空格 junction 别名(`$env:USERPROFILE\ohos-devstudio\sdk` → `$dev\sdk`),`OHOS_HOME` 指向 junction 路径,HMS include 变为无害单 token。

**npm 对齐**:tauri CLI 版本门禁要求 Rust tauri(2.11.5)与 `@tauri-apps/api` 同 major.minor,`package.json` 固定 `"@tauri-apps/api": "2.11"`(实装 2.11.1)。

**构建姿势**:`cargo tauri ohos build` 必须在**仓库根**执行(beforeBuildCommand 在 CLI cwd 找 `package.json`,在 src-tauri/ 下会报 Missing script: build);PowerShell 直调 npm 需 `npm.cmd`(执行策略拦截 npm.ps1);debug 构建命令 `cargo tauri ohos build -d -t aarch64`,完整环境配方见 §8。

**hvigor 打包阻断(排查中)**:

1. `env.rs` 强制 `DEVECO_SDK_HOME = parent³(OHOS_HOME)`,按 SDK 根契约算得 junction 伪根(比 hvigor 期望的 `...\sdk` 少一级)→ 00303312 "Cannot find the corresponding SDK version";
2. `gen/ohos/build-profile.json5` 的 `compatibleSdkVersion: "5.0.0(12)"`(init 写死)与本机 SDK(HarmonyOS 26.0.0 / API 26,见 `sdk/default/sdk-pkg.json`)不匹配,DevEco 自带模板同为旧值,需实测修正;
3. 附带坑:hvigor 6 失败后遗留 node+java daemon 持有重定向管道句柄,PowerShell `*>` 重定向假死——迭代用 `Start-Process` + 文件重定向,或先杀 daemon。

**下一步**:以独立 `hvigorw assembleHap`(正确 `DEVECO_SDK_HOME` + 修正 compatibleSdkVersion)先迭代出 HAP,再定持久方案(gen/ohos 固定 `local.properties` 的 sdk.dir,或 junction 增设版本层使 parent³ 恰落在 sdk 根)。

## 7. 移植路线图与进度清单

### Phase 0 — 技术验证(1~2 周)
- [x] 全量平台门控审计与修正(本分支)
- [x] `ohos_integration.rs` 骨架
- [x] 宿主平台 `cargo check` 无回归验证(Rust 1.99.0 MSVC + CMake 4.4.3 已安装)
- [x] `rustup target add aarch64-unknown-linux-ohos`(目标 std 已就绪)
- [x] OpenHarmony SDK 就绪(DevEco Studio 自带,API 26):clang 包装脚本(`~/.cargo/bin/aarch64-unknown-linux-ohos-*.cmd`)、本地链接器配置(`src-tauri/.cargo/config.toml`)、CMake 工具链文件(`src-tauri/ohos/ohos-toolchain.cmake`,已提交)全部就位
- [x] `cargo check --target aarch64-unknown-linux-ohos` 通过(核心依赖探针工程全量通过,详见 6.1;主仓本体被上游 tauri 桌面栈阻断,属预期的 Phase 1 边界)
- [ ] 用 [richerfu/tauri-demo](https://github.com/richerfu/tauri-demo) 跑通 HAP 出包全流程

### Phase 1 — 构建管道(2~3 周)
- [x] 接入 tauri-cli `feat/open-harmony` 分支:`cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony`(v2.11.4,含 `ohos` 子命令)
- [x] `[patch.crates-io]` 指向 wry/tao/tauri 的 ohos 分支(实际方案:补丁直接提交进主 `src-tauri/Cargo.toml` + vendored openharmony-ability 入库——本仓即 OHOS 构建工作区,独立清单反而割裂;风险由 Cargo.lock 冻结与 6.2 风险登记控制)
- [x] 自备 OHOS 版 `libonnxruntime.so` 放入 `src-tauri/libs/ohos/arm64-v8a/`(v1.26.0;目录 gitignored,各构建机自备)
- [x] 主仓 `cargo check --target aarch64-unknown-linux-ohos` 全量通过 + 宿主无回归(见 6.2)
- [x] `cargo tauri ohos init` 生成 `gen/ohos` 工程(2026-10-03;产物 gitignored,未引入跟踪文件变更)
- [ ] lensfun_db / resources 打包进 HAP
- [ ] DevEco Studio / hvigor 出包,真机(或模拟器)点亮空白窗口

### Phase 2 — 平台集成(3~4 周)
- [ ] `ohos_integration.rs`:FileKit/photoAccessHelper URI 文件桥(参照 `android_integration.rs` 模式,经 `@ohos-rs/ability` NAPI)
- [ ] 相册导出 `save_image_bytes_to_ohos_gallery`(替代 Android MediaStore 路径)
- [ ] TLS 根证书策略确认(reqwest rustls 目前用捆绑 webpki 根;rustls-platform-verifier 无 OHOS 后端)
- [ ] tauri-plugin-dialog / fs 的 OHOS 实现或前端替代
- [ ] 深色模式/安全区/返回手势等系统 UI 适配

### Phase 3 — 渲染验证(1~2 周)
- [ ] compute-only + IPC 回读路径点亮编辑器(Android 同款,零新增风险)
- [ ] 评估 wgpu GLES surface 直渲(XComponent + `OhosNdkWindowHandle`)提性能
- [ ] 高像素 RAW(≥60MP)真机性能与内存基线

### Phase 4 — AI 与发布(2~3 周)
- [ ] ORT 动态加载真机验证;AI 蒙版/降噪功能分级测试
- [ ] (可选)MindSpore Lite / NNRt NPU 路径评估
- [ ] AGC 签名、AppGallery 上架(摄影类目)、版本通道

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

Windows 主机注意:OHOS NDK 的 clang 是 Unix shell 脚本,需手写 `.cmd` 包装(已完成,见 6.1);Rust 交叉编译与 ohrs 构建已在 Windows 主机全量打通(见 6.3),HAP 打包推进至 hvigor 一步。

**Windows 主机 HAP 出包环境**(已验证至 hvigor 打包一步;契约与陷阱详见 6.3;DevEco Studio 自带 ohpm/hvigor,系统 Node 即可):

```powershell
$dev = 'C:\Program Files\Huawei\DevEco Studio'
# 一次性:无空格 SDK 别名(ohrs 的 HMS include 注入 + cc-rs 空白切分要求无空格路径)
New-Item -ItemType Directory -Path "$env:USERPROFILE\ohos-devstudio" -Force | Out-Null
if (-not (Test-Path "$env:USERPROFILE\ohos-devstudio\sdk")) { New-Item -ItemType Junction -Path "$env:USERPROFILE\ohos-devstudio\sdk" -Value "$dev\sdk" | Out-Null }
# 环境变量(OHOS_HOME 必须是 SDK 根,即 openharmony 目录——见 6.3 契约)
$env:OHOS_HOME = "$env:USERPROFILE\ohos-devstudio\sdk\default\openharmony"
$repo = 'C:\workspace\RapidRAW'
$ninja = "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja"
$env:Path = "C:\Program Files\CMake\bin;$ninja;$dev\tools\ohpm\bin;$dev\tools\hvigor\bin;$env:USERPROFILE\.cargo\bin;" + $env:Path
$env:CMAKE_TOOLCHAIN_FILE = "$repo\src-tauri\ohos\ohos-toolchain.cmake"
$env:CMAKE_GENERATOR = 'Ninja'
$env:ORT_SKIP_DOWNLOAD = '1'; $env:ORT_DYLIB_PATH = 'libonnxruntime.so'
npm.cmd install "@tauri-apps/api@2.11"    # 对齐 Rust tauri 2.11.5(版本门禁)
# 在仓库根执行(勿在 src-tauri/ 下;beforeBuildCommand 在 CLI cwd 找 package.json):
cargo tauri ohos build -d -t aarch64      # .so → src-tauri/gen/ohos/entry/libs/arm64-v8a/
# hvigor 打包(DEVECO_SDK_HOME / compatibleSdkVersion 打通后):
#   cd src-tauri\gen\ohos; ohpm install; hvigorw assembleHap   # 产物 entry-default-unsigned.hap
```
