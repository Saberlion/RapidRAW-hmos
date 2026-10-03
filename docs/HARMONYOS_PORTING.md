# RapidRAW 鸿蒙(HarmonyOS / OpenHarmony)移植报告

> 分析日期:2026-10 · 分支:`feature/harmonyos-port`
> 状态:Phase 0 脚手架已落地(见文末进度清单)

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

**已知限制**:开发机暂无 Rust 工具链,本分支变更未经 `cargo check` 编译验证,待 Phase 1 环境就绪后第一时间补验。

## 7. 移植路线图与进度清单

### Phase 0 — 技术验证(1~2 周)
- [x] 全量平台门控审计与修正(本分支)
- [x] `ohos_integration.rs` 骨架
- [ ] 安装 Rust 工具链 + `rustup target add aarch64-unknown-linux-ohos`
- [ ] 获取 OpenHarmony SDK(标准系统公共 SDK),配置 clang 包装脚本与 `~/.cargo/config.toml` 链接器
- [ ] `cargo check --target aarch64-unknown-linux-ohos` 通过(核心库,不含 tauri fork)
- [ ] 用 [richerfu/tauri-demo](https://github.com/richerfu/tauri-demo) 跑通 HAP 出包全流程

### Phase 1 — 构建管道(2~3 周)
- [ ] 接入 tauri-cli `feat/open-harmony` 分支:`cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony`
- [ ] `cargo tauri ohos init` 生成 `gen/ohos` 工程;`[patch.crates-io]` 指向 wry/tao/tauri 的 ohos 分支(注意:patch 是全局的,建议独立构建工作区或 CI 专用清单,勿提交进主 Cargo.toml)
- [ ] lensfun_db / resources 打包进 HAP
- [ ] 自备 OHOS 版 `libonnxruntime.so` 放入 `src-tauri/libs/ohos/arm64-v8a/`(来源:sherpa-onnx OHOS 构建或 [csukuangfj/onnxruntime-libs](https://github.com/csukuangfj/onnxruntime-libs))
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

Windows 主机注意:OHOS NDK 的 clang 是 Unix shell 脚本,需手写 `.cmd` 包装;建议 Linux/macOS 主机出包。
