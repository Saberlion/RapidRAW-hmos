# LightRaw(原 RapidRAW)鸿蒙移植 —— 测试用例文档

> 生成日期:2026-10-25 · 适用分支:`feature/harmonyos-port`
> 用例来源:**从代码与已有验证记录反推**(`src/`、`src-tauri/src/`、`scripts/`、`docs/HARMONYOS_PORTING.md`),非既有测试套件。
> 本仓**无自动化测试套件**(无前端测试、无 Rust 单测);验证 = 静态门禁 + 手动/脚本化实机验证。执行结果见 `docs/TEST_REPORT.md`;空白执行模板见 `docs/TEST_REPORT_TEMPLATE.md`。

---

## 1. 文档说明

### 1.1 目的与范围

本用例集覆盖两部分:

1. **鸿蒙移植专项**(主要):构建打包、平台门控、沙箱路径、FileKit 文件桥、导入/导出链、窗口控制、系统 UI 适配、ArkWeb 存储兜底、签名上架、**AppGallery 隐私管理服务对接**;
2. **核心功能回归**(次要):图库、编辑器 GPU 管线、调整项、AI 蒙版/降噪、性能与内存基线,以及桌面/Android 的"零回归"等价性验证。

不在范围内:桌面三平台的全功能验收(仅覆盖 cfg 等价性冒烟)、相机联机拍摄(`tethering`,OHOS 不构建该 feature)、云服务/社区页后端。

### 1.2 参考文档

| 文档 | 用途 |
|---|---|
| `docs/HARMONYOS_PORTING.md` | 移植主文档:已完成项、坑位、验证证据(**用例依据的主要来源**) |
| `docs/AI_BACKEND_PLACEMENT.md` / `docs/AI_DENOISE_HW_ACCELERATION.md` | AI 后端与硬件加速设计 |
| `docs/MINDSPORE_LITE_NPU_EVAL.md` | NPU(MS Lite / CANN)评估与设备验证配方 |
| `bench/README.md` | UI 平滑度脚本化对比方法 |
| `AGENTS.md` | 纪律:构建门禁、禁止 `cargo update`、cfg 词汇表 |

### 1.3 用例编号与优先级

- 编号:`TC-<模块>-<序号>`,模块见 §2 索引。
- 优先级:
  - **P0** = 阻塞发布 / 核心链路,失败即不可用;
  - **P1** = 重要功能或主要用户体验路径;
  - **P2** = 次要路径、外观、运维便利项。
- 用例性质:`[功能]` 功能验证 / `[回归]` 既有平台等价性 / `[性能]` 量化指标 / `[静态]` 门禁命令 / `[探索]` 未定论项。

### 1.4 执行环境矩阵

| 代号 | 环境 | 关键特征 | 用途 |
|---|---|---|---|
| **HOST** | Windows 11 开发机 | Rust 1.98/1.99 MSVC · CMake+Ninja · DevEco SDK API 26 | 宿主构建、静态门禁、出包脚本 |
| **SIM** | DevEco x86_64 模拟器 | 4 GB RAM · Mali-G77 GLES 透传 · 无 Vulkan/NNRT/CANN | 快速功能验证(性能绝对值不可外推) |
| **DEV-A** | 真机 HAD-W32(2in1 平板) | Kirin + Maleoon 916 · HarmonyOS 7.0.0.17 log · `deviceType=2in1` · 23.3 GiB RAM · `max_texture_dimension_2d=8192` | 主验证机:全链路 + 持久化 + CANN |
| **DEV-B** | 真机 MateBook Pro S | Maleoon 935 · `max_texture_dimension_2d ≥ 11776` · ≥12 GiB RAM | 大尺寸 GPU 导出色块/分块验证 |
| **DEV-C** | 真机 pad 形态 | `deviceType=tablet` · 无 `FILE_ACCESS_PERSIST` 能力(预期) | 双形态持久化矩阵(v2 回退路径) |
| **DESK/AND** | 桌面 / Android 构建 | Windows/macOS/Linux/Android | 回归等价性冒烟 |

> 注:OHOS 目标满足 `target_os = "linux"` + `target_env = "ohos"`,测试时必须用 `is_ohos_build`(而非 `platform()`)判定平台。

### 1.5 测试工具与取证方法(执行前必读)

| 能力 | 命令 / 手段 | 备注 |
|---|---|---|
| 安装/启动 | `hdc install -r <hap>` / `hdc shell aa start -a EntryAbility -b io.github.CyberTimon.RapidRAW` | 未签名 HAP 模拟器可直装;release HAP 真机直装被拒(见 TC-SIGN) |
| 截屏 | `hdc shell snapshot_display -f /data/local/tmp/x.jpeg` + `hdc file recv` | 用**截图字节数**快速判别界面状态(欢迎/图库/picker/锁屏) |
| 触摸/按键注入 | `hdc shell uinput -T -m x y x y 120`(带位移即 swipe);`hdc shell "uitest uiInput keyEvent Back"` | `uinput` 触摸注入会**会话级死亡**;`reboot` 恢复 |
| UI 元素定位 | `hdc shell uitest dumpLayout`(**加 `-i` 才能看到系统 picker 窗口**) | 系统对话框不在 webview 内,必须 dumpLayout 取 bounds |
| Webview 内调试 | `hdc -t <key> fport tcp:9222 localabstract:webview_devtools_remote_<pid>` + CDP `Runtime.evaluate` | 页面内直测 invoke 返回值、DOM 状态;点击 React 元素零坐标误差 |
| 应用日志 | `/data/app/el2/100/base/<bundle>/files/logs/app.log`(Rust `fern`) | 关键路径证据源(fast mode);hilog 环形缓冲轮转快,**事件后 2~4s 内抓取** |
| 系统日志 | `hdc shell "hilog -x \| grep -E 'LowMemoryKill\|onAbilityDied\|CppCrash'"` | 崩溃/查杀归因;系统侧(CommonSaveAbility)日志才是导出类问题的根因现场 |
| 内存 | `hdc shell "hidumper --mem <pid>"` | PSS 分解(native heap / mmap) |
| 出包 | `scripts/build-ohos.ps1 [-Target x86_64] [-Release] [-SkipBuild]` | 自包含环境 + fail-fast + 看门狗 |
| 平滑度 | `bench/replay.js` + `bench/analyze.mjs` | **仅同机前后可对比** |

### 1.6 静态门禁命令(CI 硬门禁,必须全绿)

```powershell
# 前端(仓库根)
npm run typecheck; npm run lint; npm run format:check; npm run i18n:check
# Rust(src-tauri/,硬门禁)
cargo fmt -p RapidRAW -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check                                   # 宿主
cargo check --target aarch64-unknown-linux-ohos   # OHOS(需 §6.1 环境变量)
```

---

## 2. 用例索引

| 模块 | 编号前缀 | 用例数 | 关注点 |
|---|---|---|---|
| 构建与打包 | `TC-BUILD` | 18 | 双目标编译、出包脚本、ORT/CANN 注入、strip |
| 平台判定与 cfg 门控 | `TC-PLAT` | 7 | `is_ohos_build`、dialog 排除、cfg 等价性 |
| 沙箱路径 | `TC-PATH` | 8 | `app_paths` 四映射、设置/日志/asset 协议 |
| FileKit 文件桥 | `TC-FILE` | 15 | 目录/文件/另存为桥、授权持久化、重授权自愈 |
| 导入链 | `TC-IMPORT` | 7 | FAB/选择器/LUT/预设消费点 |
| 导出链 | `TC-EXPORT` | 15 | 直写目录、相册桥、13900002/14000011、大尺寸 |
| 窗口控制 | `TC-WIN` | 7 | 系统装饰栏、拖动桥、exit_app |
| 系统 UI 适配 | `TC-UI` | 12 | 深浅模式、返回手势、协议门控、存储兜底 |
| 图库 | `TC-LIB` | 8 | 扫描、缩略图、会话恢复、元数据 |
| 编辑器 GPU 管线 | `TC-EDIT` | 12 | 后端选择、回读路径、预热、直渲评估 |
| 调整项 | `TC-ADJ` | 12 | 基础/曲线/色彩/细节/效果/蒙版/镜头 |
| AI 蒙版与降噪 | `TC-AI` | 15 | 模型下载持久化、ORT/NNRT/CANN、回退 |
| 性能与内存 | `TC-PERF` | 7 | 高像素基线、交互 FPS、预热 |
| 签名与上架 | `TC-SIGN` | 10 | HAP/.app 双层签名、AGC 991/权限对齐 |
| 回归与静态门禁 | `TC-REG` | 9 | fmt/clippy/tsc/eslint/i18n、桌面回归 |
| AppGallery 隐私管理服务 | `TC-PRIV` | 13 | 隐私链接、签署状态、主动请求/撤销、签署事件、取权门控、未上架预置 |
| **合计** | | **175** | |

---

## 3. 用例明细

### 3.1 TC-BUILD —— 构建与打包

> 代码:`src-tauri/Cargo.toml`(`[patch.crates-io]` + OHOS 段)、`src-tauri/build.rs`(ORT 供给)、`scripts/build-ohos.ps1`;坑位见文档 §6.1–6.3、§6.12、§6.15、§6.16。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-BUILD-01 | 宿主编译无回归 | [HOST] 依赖已拉取 | `cd src-tauri` → `cargo check` | EXIT=0,无 OHOS 相关新增错误 | P0 | §6.1/§6.2 验证记录 |
| TC-BUILD-02 | OHOS aarch64 交叉编译 | ①§6.1 环境变量齐(`OHOS_NDK_HOME`/`CMAKE_TOOLCHAIN_FILE`/`CMAKE_GENERATOR`/`ORT_SKIP_DOWNLOAD`/`ORT_DYLIB_PATH`)② `.cargo/config.toml` 链接器段 | `cargo check --target aarch64-unknown-linux-ohos` | EXIT=0;仅 OHOS 门控死代码警告;依赖树零 gtk/rfd 泄漏 | P0 | §6.2;`tc` 词汇表 |
| TC-BUILD-03 | x86_64 OHOS 目标可编译 | 同上 + x86_64 clang 包装脚本 | `cargo check --target x86_64-unknown-linux-ohos` | EXIT=0(模拟器出包前置) | P1 | §6.4 交叉链路表 |
| TC-BUILD-04 | 默认(debug)出包直通 | [HOST] 一次性 junction/SDK 契约就绪 | 仓库根执行 `scripts\build-ohos.ps1` | 全链路 EXIT=0;产出 `gen/ohos/entry/build/default/outputs/default/entry-default-unsigned.hap` | P0 | §6.3;脚本头注释 |
| TC-BUILD-05 | `-Release` 真正透传 release | 同上 | `scripts\build-ohos.ps1 -Release` | HAP 内 `.so` 为 release(strip 后体积显著小于 dev);无调试信息 | P0 | §6.15(-Release 关键坑);§6.17 |
| TC-BUILD-06 | `-Target x86_64` 出模拟器包 | 同上 | `scripts\build-ohos.ps1 -Target x86_64` | 产物 ABI 为 x86_64,HAP 可在 [SIM] 安装启动 | P1 | §6.4;脚本参数 |
| TC-BUILD-07 | `-SkipBuild` 仅重注入 | 已存在 HAP | `scripts\build-ohos.ps1 -SkipBuild -Target aarch64` | 不触发 cargo 阶段;仅注入 ORT/CANN 并重签;结果 HAP 可用 | P2 | §6.12/脚本注释 |
| TC-BUILD-08 | ORT 按 ABI 注入 + 异构目录清理 | `src-tauri/libs/ohos/<abi>/libonnxruntime.so` 存在 | 出包后检查 HAP 内 `entry/libs/<abi>/` | 仅目标 ABI 含 `libonnxruntime.so`;异构 ABI 残留被清除 | P0 | §6.12/§6.15 |
| TC-BUILD-09 | `libc++_shared.so` 注入 | 同上 | 检查 HAP 内 libs | ORT 的 DT_NEEDED 满足:`libc++_shared.so` 同 ABI 存在 | P0 | §6.12(PANIC 根因) |
| TC-BUILD-10 | CANN 38 库注入 | CANN 库已从设备拉取到 `libs/ohos/arm64-v8a/` | 出包后核对 HAP 条目 | 38 个库全部进入代码签名块(HAP 内压缩后显著小于原始 ~5.6MB);**`_impl` 链不打包** | P1 | §6.16(命名空间机制④/注入通道) |
| TC-BUILD-11 | ORT 已 strip | [HOST] `llvm-strip` 可用 | 出包后 `llvm-readelf --sections`/`nm` 检查 HAP 内 ORT | 零 `.debug_*` 段、零 symtab;dynsym 完好 | P0 | §6.17(问题②三因) |
| TC-BUILD-12 | sourcemap 不进包 | 同 TC-BUILD-05 | 解包检查 `ets/sourceMaps.map` | 文件不存在 | P0 | §6.17 |
| TC-BUILD-13 | 单 ABI 瘦身 | 出真机/release 包 | 清 `entry/libs` 非目标 ABI 或配 `abiFilters` | HAP 体积回落到单 ABI 量级(不再双 .so 各 ~380MB) | P2 | §6.4-3;§9.3-2 |
| TC-BUILD-14 | fail-fast 预检 | 故意破坏一项前置(如清 `OHOS_HOME`) | 运行脚本 | 在耗时阶段前报错退出,打印缺失项 | P2 | 脚本设计目标 1 |
| TC-BUILD-15 | 看门狗与心跳 | 长构建场景 | 运行脚本并观察 stdout | 每 ~10s 输出日志增长;超时则杀进程树并 EXIT=124 | P2 | 脚本设计目标 2 |
| TC-BUILD-16 | hvigor daemon 卫生 | 连续多次构建后 | 再次运行脚本 | 启动时自动清理 stale hvigor daemon;不因 OOM 假死 | P2 | §6.7-5;§6.3-3 |
| TC-BUILD-17 | Rust 静态门禁 | [HOST] | `cargo fmt -p RapidRAW -- --check` 与 `cargo clippy --all-targets --all-features -- -D warnings` | 双绿(CI 硬门禁) | P0 | `AGENTS.md` CI |
| TC-BUILD-18 | ORT 缺失时降级不失败 | 删除 `libs/ohos/arm64-v8a/libonnxruntime.so` | 出包 | build.rs 告警但构建继续;运行时 AI 功能优雅降级 | P1 | §6 代码变更清单(build.rs) |

### 3.2 TC-PLAT —— 平台判定与 cfg 门控

> 代码:`ohos_integration.rs`(`is_ohos_build`)、`Cargo.toml`、`capabilities/dialog.json`、`package.json`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-PLAT-01 | OHOS 构建平台判定为真 | [DEV-A/SIM] | 前端调用 `is_ohos_build`(或观察 OHOS 专属 UI 分支) | 返回 `true`;OHOS 分支生效 | P0 | `ohos_integration.rs:100` |
| TC-PLAT-02 | 非 OHOS 平台判定为假 | [DESK/AND] | 同上 | 返回 `false`;`pick_ohos_*`/`ohos_window_control` 返回 Err | P1 | `ohos_integration.rs:405+` |
| TC-PLAT-03 | dialog 插件 OHOS 三处排除 | 静态检查 + 出包 | ①`Cargo.toml` 依赖段 ②`lib.rs` 注册 ③`capabilities/dialog.json` 平台作用域 | 三处一致;OHOS 构建树无 rfd/gtk;`capabilities/default.json` 无 `dialog:default` | P0 | §6.2 坑 1 |
| TC-PLAT-04 | cfg 门控对既有平台语义等价 | [HOST]+桌面/Android 构建 | 逐条核对 `not(target_env="ohos")` 改写点;跑宿主 + 桌面冒烟 | 非 OHOS 目标上表达式取值与修改前一致;桌面/Android 行为无差异 | P0 | §6 不变性保证 |
| TC-PLAT-05 | CLI 与前端 API 版本对齐 | [HOST] | 比对 Rust `tauri` 版本(fork 2.11.5)与 `package.json` `@tauri-apps/api` | 同 major.minor,构建无版本门禁报错 | P1 | §6.3 npm 对齐 |
| TC-PLAT-06 | OS 探测不得依赖 `platform()` | 代码审查 + [DEV-A] | 检查所有 OHOS 判定点 | 全部走 `is_ohos_build`(tauri-plugin-os 在 OHOS 返回 "linux",不可用) | P1 | §6.6-2 |
| TC-PLAT-07 | i18n 键全 locale 一致 | [HOST] | `npm run i18n:check` | 15 locale 键零漂移(含 `system` 主题键) | P1 | `AGENTS.md`;§6.8-1 |

### 3.3 TC-PATH —— 沙箱路径(app_paths.rs)

> 代码:`src-tauri/src/app_paths.rs`(OHOS 直连 `/data/storage/el2/base/*`);根因见 §6.13 第 3 层。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-PATH-01 | appdata 路径映射 | [DEV-A] 全新安装 | 触发设置读写,查 app.log / 沙箱 | `app_data_dir` = `/data/storage/el2/base/files/appdata`,**不落** FUSE 用户存储 | P0 | §6.13-3 |
| TC-PATH-02 | config/logs/cache 三映射 | 同上 | 检查启动日志与缓存目录 | `files/config`、`files/logs`、`cache` 均指向沙箱对应目录 | P0 | `app_paths.rs:25-75` |
| TC-PATH-03 | 目录自动创建 | 全新安装首次启动 | 观察启动流程 | 返回路径前已 `create_dir_all`,首次启动零 EACCES/EPERM | P1 | `app_paths.rs` 注释 |
| TC-PATH-04 | 非 OHOS 委托原解析器 | [DESK] | 对照改动前后路径 | 桌面路径解析行为不变 | P1 | `app_paths.rs:33+` |
| TC-PATH-05 | 设置持久化成功 | [DEV-A] | 改任一设置 → 重启 → 复查;查 app.log | 无 `Failed to load/save settings`;设置生效并持久 | P0 | §6.13-3 |
| TC-PATH-06 | 日志落沙箱 | [DEV-A] | 读 `/data/app/el2/100/base/<bundle>/files/logs/app.log` | 文件存在且持续写入 | P1 | §6.13-3 方法学 |
| TC-PATH-07 | asset 协议允许沙箱缩略图 | [DEV-A] | 触发缩略图加载,观察 app.log | 无 `asset protocol not configured to allow the path`;缩略图正常显示 | P0 | §6.13-4(`assetProtocol.scope`) |
| TC-PATH-08 | 后台索引/LUT 列举不再 EPERM | [DEV-A] | 首次启动后查 app.log | 无 `Failed to start background indexing` / `Failed to list LUTs`;LUT 列表渲染 | P1 | §6.13-3 |

### 3.4 TC-FILE —— FileKit 文件桥

> 代码:`ohos_integration.rs` `file_bridge`、`useAppNavigation.ts`(`handleOpenFolder`/`handleContinueSession`)、`file_management.rs::check_paths_readable`、`EntryAbility.template.ets`(`persistFolderGrant`/`recordFolderGrant`/`activatePersistedGrants`)。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-FILE-01 | 目录选择返回沙箱路径 | [SIM/DEV-A] | 欢迎页/图库「添加文件夹」→ 系统 picker 选目录 | 返回 `FileUri.path` 直出应用可读路径(如 `/storage/Users/...`);成为图库根 | P0 | §6.6-3 |
| TC-FILE-02 | 目录选择取消 | 同上 | picker 内取消返回 | 返回 `null`/空;不卡死、无异常;可再次触发 | P0 | §6.6;§6.14-3 |
| TC-FILE-03 | 桥未注册时的错误可辨识 | EntryAbility 补丁缺失的构建 | 触发任一 `pick_ohos_*` | 返回可辨识错误(如 `window controller not registered`),不 panic | P2 | `ohos_integration.rs:78-81` |
| TC-FILE-04 | 并发多请求(requestId 注册表) | [DEV-A] | 批量导出/连续触发桥请求 | 各请求按 id 正确配对;无串扰、无阻塞 | P1 | §6.7-1 |
| TC-FILE-05 | 多选文件 + 后缀过滤折叠 | 同上 | 触发导入;检查所选扩展名集合 | 扩展名去重后单元素;整组 ≤96 字符;超限时放弃过滤(前端兜底校验) | P0 | §6.7-2(`collapse_suffix_filters`) |
| TC-FILE-06 | 另存为(保存 picker) | 同上 | LUT/预设「另存为」 | picker 创建文件并返回路径,调用方写入成功;取消 → 明确取消结果 | P1 | §6.7-1/§6.7-6 |
| TC-FILE-07 | Rust 可直读所选路径 | 同上 | 选定后触发树扫描/缩略图 | `read_dir`/`stat` 成功;GetFolderTree、ListImagesInDir 无错 | P0 | §6.6-3;§6.7-2 |
| TC-FILE-08 | 授权跨重启持久(2in1) | [DEV-A] `deviceType=2in1` + 声明 `FILE_ACCESS_PERSIST` | 选目录 → force-stop → 重启 → Continue Session;再整机 `reboot` 复测 | 均直通恢复,零 picker、零重授权;app.log 无 re-grant WARN | P0 | §6.14-5(实证) |
| TC-FILE-09 | pad 形态门控回退 v2 | [DEV-C] `deviceType=tablet` | 同上 | `persistFolderGrant` 门控跳过(留痕日志);走探针 + 重授权流 | P1 | §6.14-5 双形态矩阵 |
| TC-FILE-10 | 授权失效探针 | [DEV-A] 授权已失效(如重装) | 触发探针;查 app.log | `check_paths_readable` 返回 false;warning 记录死根列表 | P0 | §6.14-2(探针语义解耦) |
| TC-FILE-11 | 重授权自愈主流程 | 三种触发:install -r / force-stop / 优雅关闭 | 冷启 → Continue Session | toast 提示 → FileKit picker → 重选 → 图库恢复(缩略图全渲染、零错误) | P0 | §6.14-3(×3 实证) |
| TC-FILE-12 | 重授权取消分支 | 同上 | picker 内取消 | 回欢迎页不卡死;可再次触发;第二轮可恢复 | P1 | §6.14-3 |
| TC-FILE-13 | 部分活根只恢复活根 | 多根目录、部分失效 | 冷启恢复 | 仅恢复可读根;死根不进入库 | P2 | §6.14-2 |
| TC-FILE-14 | 预载 promise 无 unhandled rejection | 死根启动 | 查 app.log | 修复后 0 条 unhandled rejection ERROR | P2 | §6.14-2/3 |
| TC-FILE-15 | 授权记录文件去重 | [DEV-A] 多次选同目录 | 读 `${filesDir}/ohos_grants.json` | 按 uri 去重;force-stop 后 `activatePersistedGrants` 可重放 | P2 | §6.14-5 |

### 3.5 TC-IMPORT —— 导入链

> 代码:`MainLibrary.tsx`(导入 FAB 门控)、`useFileOperations.ts`、`ImagePicker.tsx`、`LUTControl.tsx`、`PresetsPanel.tsx`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-IMPORT-01 | 导入入口在 OHOS 可见 | [DEV-A] | 打开图库,查看导入 FAB | FAB 显示(`isAndroid \|\| isOhos` 门控) | P1 | `MainLibrary.tsx:624` |
| TC-IMPORT-02 | picker → 缩略图渲染 | 设备存储有图片 | 导入 FAB → 相册导航 → 选中 → 确认 | `pick_files -> N file(s)`;库内新增缩略图;「导入完成!」提示 | P0 | §6.7-4 |
| TC-IMPORT-03 | 扩展名客户端兜底校验 | 同上 | 选非支持格式(若 picker 未过滤) | 前端拒绝并提示;不产生损坏条目 | P1 | §6.7-2 |
| TC-IMPORT-04 | 图像选择器 OHOS 分支 | 编辑器内替换图片场景 | 触发 ImagePicker | 走 `PickOhosFiles`,选中后正确替换 | P1 | `ImagePicker.tsx:22-25` |
| TC-IMPORT-05 | LUT 导入 | 准备 `.cube` 文件 | LUT 控件 → 导入 | 走 `PickOhosFiles`;LUT 应用生效 | P1 | `LUTControl.tsx:149` |
| TC-IMPORT-06 | 预设导入 | 准备预设文件 | 预设面板 → 导入 | 走 `PickOhosFiles`;预设入库 | P1 | `PresetsPanel.tsx:1040` |
| TC-IMPORT-07 | 媒体库路径直读 | 从相册(`/storage/media/...`)选择 | 观察 Rust 侧读取 | `std::fs` 直读成功,无需中转 | P1 | §6.7-2 |

### 3.6 TC-EXPORT —— 导出链

> 代码:`export_processing.rs`、`ohos_integration.rs`(`ohos_export_temp_file`/`save_image_bytes_to_ohos_gallery`/`save_file_bytes_to_ohos_picker`)、`ExportPanel.tsx`(`customFolder`/lastExportPath)。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-EXPORT-01 | 编辑器导出落图库 | [DEV-A] 打开 RAW 并编辑 | 导出 → 系统对话框「允许保存」→ 允许 | `save_to_gallery ok: file://media/Photo/...`;图库 picker「来自应用」可见 | P0 | §6.7 成果 |
| TC-EXPORT-02 | 直写公共目录优先 | [DEV-A/SIM] | 导出到源目录或选定目录 | 文件直落目标目录;`.rrdata` sidecar 同树写入 | P0 | §7 Phase 2(-730) |
| TC-EXPORT-03 | 导出目录记忆 | 同上 | 首次选目录导出 → 二次导出 | 目录被记忆,二次免选 | P2 | 同上(lastExportPath) |
| TC-EXPORT-04 | 无 13900002 自拷贝 | 同上 | 导出后查 app.log | 无 `staging failed: {"code":13900002}`(Rust 临时目录为 `export_temp`,区别于 ArkTS `export_bridge`) | P0 | §7 条目(-730) |
| TC-EXPORT-05 | mkdir 幂等处理 | 连续两次导出到同路径 | 观察 | 无 13900015「File exists」;`fs.accessSync` 探测后按需创建 | P1 | §6.7-3 |
| TC-EXPORT-06 | 对话框 URI 规范 | 同上 | 导出观察对话框 | 无 14000011;ArkTS 先中转沙箱再转规范 URI | P1 | §6.7-3 |
| TC-EXPORT-07 | 用户拒绝/批量错误码 | 同上 | 拒绝授权 | 返回空数组=拒绝;非 URI 项按批量错误码识别(-3006/-2004/-203) | P1 | §6.7-3 |
| TC-EXPORT-08 | 蒙版 alpha PNG 导出 | 有蒙版 | 导出(含 alpha) | 走 picker 路径,PNG alpha 正确 | P1 | §6.7-3 |
| TC-EXPORT-09 | LUT/预设另存为 | — | `.cube`/预设「另存为」 | `save_file_as` 路径写入成功 | P1 | §6.7-6(设备端验证顺延) |
| TC-EXPORT-10 | 超大尺寸 CPU 旁路 | [DEV-A] 图 >8192px | 调整后导出 | 走 CPU 旁路(受 `max_texture_dimension_2d=8192` 限制);**已知行为非回归** | P1 | §7 Phase 3(-729/-741) |
| TC-EXPORT-11 | 101.9MP 全分辨率导出 | [DEV-B] 8736×11656 | 编辑 → 导出 | JPEG 直落目标目录;无 OOM/崩溃 | P1 | §7 Phase 2(-730) |
| TC-EXPORT-12 | 4096 分块导出无接缝 | [DEV-B/C] RAM ≥12GiB | 导出后放大检查 tile 边界 | 无接缝/色带/重影;子区域回读正确 | P1 | §7 Phase 3(-742) |
| TC-EXPORT-13 | 批量导出对话粒度 | 多选图片导出 | 观察系统对话框次数 | 每张各弹一次(**已知限制**,记入待改进) | P2 | §6.7-6 |
| TC-EXPORT-14 | 导出成功反馈 | [DEV-A] | 库视图导出 | 库视图无 toast(**已知缺陷**,记入缺陷清单) | P2 | §6.7-6 |
| TC-EXPORT-15 | 资源感知线程数 | [SIM] 内存紧张 | 批量导出观察日志 | `Batch Export: N cores, X GB free RAM -> M threads`,自适应不 OOM | P2 | §6.10-2/§6.11 |

### 3.7 TC-WIN —— 窗口控制

> 代码:`ohos_integration.rs` `window_controller` + `ohos_window_control`/`exit_app`、`TitleBar.tsx`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-WIN-01 | 系统装饰栏三按钮 | [SIM/DEV-A] | 分别点 最小化/最大化/关闭 | 所见即所点;最小化到 dock 后可 `aa start` 恢复;关闭后进程退出可重启 | P0 | §6.5-5 表 |
| TC-WIN-02 | 拖动桥位移精确 | 同上 | 自绘标题栏 swipe +400px | 窗口左缘精确 +400px | P0 | §6.5-5 |
| TC-WIN-03 | OHOS 隐藏自绘三按钮 | [DEV-A] | 观察标题栏 | 三按钮隐藏,标题文字/拖动区保留(`isLinux && !isOhos`) | P1 | §6.5-4(`TitleBar.tsx:182`) |
| TC-WIN-04 | 退出走 terminateSelf | [DEV-A] | 触发应用退出 | 无异常 ability 死亡;优雅退出 | P1 | `ohos_integration.rs:109-122` |
| TC-WIN-05 | 非 OHOS 桥命令返回 Err | [DESK] | 调 `ohos_window_control` | 返回明确 Err,不 panic | P2 | `ohos_integration.rs:552-564` |
| TC-WIN-06 | 休眠保留的 min/max 桥 | — | 代码审查 + 手工调用桥 | 桥命令仍可用,便于将来恢复自绘按钮 | P2 | §6.5-4 |
| TC-WIN-07 | 系统条外观 | [DEV-A] 浅色主题 | 观察系统条与自绘栏 | 视觉断层/名称重复(**已知瑕疵**) | P2 | §6.5-7 |

### 3.8 TC-UI —— 系统 UI 适配

> 代码:`ohos_integration.rs` `system_bridge`、`themes.ts`/`useSettingsStore.ts`(`isSystemDark`)、`useAppInitialization.ts`、`useAndroidBackHandler.ts`、`App.tsx`(协议门控)、`webStorageShim.ts`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-UI-01 | 主题「系统」选项 | [DEV-A] | 打开 设置 → 主题下拉 | 出现「系统」选项;i18n 文案正确 | P1 | §6.8-1/§6.8-3 |
| TC-UI-02 | 深浅模式双向实时 | 同上 | 主题选「系统」→ 控制中心切深色/浅色 | app.log `OHOS system color mode changed: dark=...`;UI 全域双向翻转 | P0 | §6.8-3 |
| TC-UI-03 | 启动查询当前系统色 | 冷启动、系统为深色 | 观察启动首屏 | `get_ohos_color_mode` 返回正确值;首屏无浅→深闪帧 | P1 | `useAppInitialization.ts:152` |
| TC-UI-04 | prefers-color-scheme 兜底 | 桥不可用时 | 观察 | 回退 `prefers-color-scheme`;不崩 | P2 | §6.8-1 |
| TC-UI-05 | 返回手势:编辑器→图库 | [DEV-A] 打开编辑器 | `uitest uiInput keyEvent Back` | 返回图库(非退出应用) | P0 | §6.8-3 |
| TC-UI-06 | 返回手势:模态优先 | 打开复制粘贴设置模态 | 同上 | 模态关闭,优先于"退库" | P0 | §6.8-3(优先级) |
| TC-UI-07 | 返回手势:图库→主页 | 在图库 | 同上 | 回主页(Escape 链 `handleGoHome`) | P0 | §6.8-3 |
| TC-UI-08 | 安全区日志 | [DEV-A] | 观察启动日志 | avoid area 信息被记录 | P2 | §6.8-1 |
| TC-UI-09 | OHOS 不弹自建协议门 | [DEV-A] `bm clean -d` 后冷启 | 观察首启 | 零自建协议弹窗,直进欢迎页(平台托管弹窗即首启同意) | P0 | §6.17;§6.17 协议门控复测 |
| TC-UI-10 | 桌面/Android 协议门不变 | [DESK/AND] | 清数据冷启 | 仍显示自建协议门(行为不变、无闪帧) | P1 | §6.17(**未重测**,需执行) |
| TC-UI-11 | ArkWeb 存储兜底 | [SIM/DEV-A] | CDP 检查 `typeof localStorage`;读写往返;`Object.keys` | 为 object;读写正常;`Object.keys` 不抛 | P2 | §6.13/§6.14(CDP 实证) |
| TC-UI-12 | 首次运行系统提示不阻塞 | 全新安装 | 启动 | 仅 OHOS「添加到桌面」常规提示;无崩溃/误弹窗 | P2 | §6.4 |

### 3.9 TC-LIB —— 图库

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-LIB-01 | 图库扫描无错 | [DEV-A] 已授权目录 | 打开图库/展开 Sources 树 | GetFolderTree/ListImagesInDir 成功;树与网格渲染 | P0 | §6.6-3 |
| TC-LIB-02 | 缩略图生成与缓存 | 同上 | 观察缩略图逐步出现 | 缩略图落沙箱 cache;经 asset 协议加载;无 404/协议拒绝 | P0 | §6.13-4 |
| TC-LIB-03 | 会话恢复 | 已有会话 | 重启 → Continue Session | 恢复上次库与选中项(注意 FUSE 慢,模拟器 90~120s) | P0 | §6.6-4;§6.13-2 |
| TC-LIB-04 | 路径显示假象判定 | 窄中列 | 观察库头部路径显示 | 显示 truncate 属显示假象;以 Sources 树行高亮为判据 | P2 | §6.6-4 |
| TC-LIB-05 | EXIF/元数据缓存 | 有 RAW | 打开元数据面板 | `.rrcache` 读写正常;EXIF 显示 | P1 | `exif_processing.rs` |
| TC-LIB-06 | 61MP ARW 缩略图 | [SIM] 61MP ARW | 库扫描 | 走内嵌预览提取(无去马赛克日志),正常出图 | P1 | §6.10-3 |
| TC-LIB-07 | 45MP DNG 全管线缩略图 | [SIM] 合成 45MP DNG | 库扫描 | 无内嵌预览 → 全管线正常 | P2 | §6.10-3 |
| TC-LIB-08 | 缩略图队列并发 | 大量图片 | 连续滚动库 | 队列有序推进;进度事件正确;无死锁 | P2 | `file_management.rs` workers |

### 3.10 TC-EDIT —— 编辑器 GPU 管线

> 代码:`gpu_processing.rs`(`get_or_init_gpu_context`、`warm_up_pipelines`、`read_texture_data_roi`、OHOS 后端选择)、`Editor.tsx`/`ImageCanvas.tsx`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-EDIT-01 | 适配器日志正确 | [SIM/DEV-A] | 打开编辑器,查 app.log | `Using GPU adapter: ... (backend: ..., type: ...)` | P1 | §6.8-1 |
| TC-EDIT-02 | compute+回读路径点亮 | [SIM/DEV-A] | 打开 RAW 编辑器 | `Creating new GPU Processor ...`;`[apply_adjustments] ... on GPU in ...ms` | P0 | §6.8-3 |
| TC-EDIT-03 | 交互 ROI FPS 记录 | 同上 | 拖动曝光滑条,采集日志 | 交互 ROI 有可测 FPS(模拟器 ~14~29;[DEV-A] ~290ms/趟) | P1 | §6.8-3;§6.10-3;§6.13-2 |
| TC-EDIT-04 | 编辑视觉生效 | 同上 | 曝光调至显著值 | 画布实时反映(参数→GPU→回读→IPC→Canvas 全链路) | P0 | §6.8-3 |
| TC-EDIT-05 | 全预览首帧耗时 | 同上 | 打开大图,记录首帧 | 日志可量化(模拟器 244~320ms;[DEV-A] 648ms/578ms) | P1 | §6.10-3;§6.13-2 |
| TC-EDIT-06 | 启动期管线预热 | [DEV-A] 冷缓存(卸载重装) | 启动后查日志 | `[pipeline-warmup] completed in Xs` 在后台吸收;首个真实作业 <1s(修复前 11s+) | P0 | §6.13-4 |
| TC-EDIT-07 | Fake map 良性 | — | 查日志 | `[ERROR] Fake map` 出现但渲染正常(已知良性) | P2 | §6.8-3 |
| TC-EDIT-08 | 真机命中 Vulkan | [DEV-A] 未显式设 `WGPU_BACKEND` | 查 app.log 后端 | `backend: Vulkan`(OHOS 默认 `VULKAN\|GL`) | P0 | §6.13-1 |
| TC-EDIT-09 | GLES 回读崩溃防护 | [DEV-A](Maleoon 转译层) | 打开编辑器导出/回读路径 | 不出现 `__stack_chk_fail` ← `queue.submit` ← `read_texture_data_roi` 崩溃栈 | P0 | §6.13-1(faultlog) |
| TC-EDIT-10 | ash 定位 `libvulkan.so` | [DEV-A] | 查 app.log Vulkan 加载 | 无 `drivers/libraries could not be loaded`(vendor ash 分支生效) | P1 | §6.13-2 |
| TC-EDIT-11 | GLES surface 直渲 PoC | [DEV-A] Vulkan | 立项后执行(surface 创建→单帧 present→接入) | 交互性能显著提升,仍保留回读 fallback | P2 | §6.9(结论 GO);§9.4(未立项) |
| TC-EDIT-12 | 编辑器内全链路 | [DEV-A] | 蒙版生成 → 降噪 → 导出 各跑一次 | 三链路在编辑器内均可用,无崩溃 | P1 | §6.7-4;§6.16 |

### 3.11 TC-ADJ —— 调整项(核心功能回归)

> 说明:调整项由 Rust `gpu_processing.rs` shader 与 `image_processing.rs` 实现;OHOS 侧需验证"经回读路径后调整仍正确"。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-ADJ-01 | 基础调整 | [DEV-A] 打开 RAW | 依次调 曝光/对比度/高光/阴影/白色/黑色 | 每项视觉生效;无错帧 | P1 | `gpu_processing.rs` shader |
| TC-ADJ-02 | 曲线 | 同上 | 调整 RGB/单通道曲线 | 曲线正确应用 | P1 | `Curves.tsx` |
| TC-ADJ-03 | 色彩 | 同上 | HSL/色轮/色温色调 | 视觉正确 | P1 | `Color.tsx`/`white_balance.rs` |
| TC-ADJ-04 | 细节 | 同上 | 锐化/降噪(传统)/结构 | 视觉正确 | P1 | `gpu_processing.rs` |
| TC-ADJ-05 | 效果(tonal/clarity/structure/去薄雾/glow/halation) | 同上 | 逐项调整 | 视觉正确 | P1 | §7 Phase 3(-741) |
| TC-ADJ-06 | 模糊 pass 跳过正确性 | [DEV-B] 101.9MP | 仅设 shadows+clarity,余模糊项为 0 | 跳过 sharpness/structure;导出与旧构建**字节一致**且视觉无差异 | P1 | §7 Phase 3(-741,-45.8%) |
| TC-ADJ-07 | 蒙版调整 | 同上 | 画 径向/线性/画笔蒙版 + 反转/扩展 | 蒙版局部生效;逐蒙版保守探测不误跳过 | P1 | §7 Phase 3(-741) |
| TC-ADJ-08 | 镜头校正 | 有镜头 EXIF | 启用镜头校正 | lensfun 数据从 HAP 内加载;校正生效 | P1 | §7 Phase 1(lensfun_db 打包) |
| TC-ADJ-09 | 裁剪/旋转/几何/引导透视 | 同上 | 逐项操作 | 视觉正确;缓存 hash 正确 | P1 | `cache_utils.rs`/`guided_perspective.rs` |
| TC-ADJ-10 | 白平衡/灰点 | 同上 | 吸管取灰点 | 白平衡正确 | P2 | `white_balance.rs` |
| TC-ADJ-11 | 直方图/波形 | 有选中图 | 观察直方图面板 | **已知缺陷**:无选中时噪点/空白帧 | P2 | §6.7-6(待查) |
| TC-ADJ-12 | 复制粘贴调整 | 两张图 | 复制调整 → 粘贴到目标 | 参数正确迁移(含模态优先级) | P2 | `CopyPasteSettingsModal.tsx` |

### 3.12 TC-AI —— AI 蒙版与降噪

> 代码:`ai_processing.rs`(`persist_downloaded_asset`、`DenoiseSession`、`MaskSession`、`run_*`)、`ohos_integration.rs`(`ms_lite`、`om`);配方见 §6.12/§6.16 与 `MINDSPORE_LITE_NPU_EVAL.md §8.4`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-AI-01 | 模型下载与校验 | 联网(CN 走 hf-mirror) | 触发 AI 功能,观察下载 | 完整落盘至沙箱 models 目录;sha256 校验通过 | P0 | §6.12 修复 1 |
| TC-AI-02 | 持久化字节拷贝回退 | FUSE 环境 / 旧数据 | 触发下载,查 app.log | rename 失败时拷贝回退成功;WARN 可辨;回退常驻 | P0 | §6.12-1/§6.13 遗留 |
| TC-AI-03 | ORT 动态加载 | ORT 已注入 HAP | 触发任一 AI 推理 | 无 `Error loading shared library libc++_shared.so` PANIC;Session 构建成功 | P0 | §6.12 修复 2 |
| TC-AI-04 | NIND 降噪端到端(单模型) | 模型已下载 | 触发 AI 降噪 → 保存 | 分块推理 → 前后对比 → 保存 `.png` + `.rrdata`;无崩溃 | P0 | §6.12 实证 |
| TC-AI-05 | 5 模型蒙版栈分级 | [DEV-A] 23GiB RAM | SAM/u2net/skyseg/depth 逐个触发 | 各功能可用,内存不触发 LMK;记录耗时 | P1 | §9.1(待执行) |
| TC-AI-06 | ORT CPU 回退完整性 | CANN 不可用场景 | 触发蒙版(强制回退) | 6 轮回退均完整出蒙版、无崩溃 | P0 | §6.16(反向证据链) |
| TC-AI-07 | CANN u2net 前景端到端 | [DEV-A] `u2net.om` 已侧载 | 触发 AI 前景蒙版 | NPU 推理 → 蒙版落盘;触发到 `.rrdata` 保存 ~4.85s;蒙版尺寸正确 | P0 | §6.16(T8) |
| TC-AI-08 | CANN nind 降噪 tile 循环 | `nind_v2.om` 已侧载 + 后端=npu | 触发 AI 降噪 | `.om` NCHW I/O 直喂,无 NHWC 转置;正常出图 | P1 | §6.16(残留⑤待测) |
| TC-AI-09 | IO 契约校验与回退 | `.om` 与预期不符 | 触发 | 契约校验失败 → 回退 ORT;不 crash | P1 | §6.16(路由/契约) |
| TC-AI-10 | 命名空间库闭包 | [DEV-A] 38 库随 HAP | 触发 CANN 路径 | 无跨 ns REJECT;`_impl` 链不打包;模型上板成功 | P1 | §6.16(T1-T8) |
| TC-AI-11 | perfmode 设置与复核 | 同上 | 查日志 | 请求 4、Get 复核一致(不一致仅 WARN 继续) | P2 | `ohos_integration.rs` init_options |
| TC-AI-12 | 蒙版内容 sanity | 同上 | 检查落盘蒙版 | 全分辨率 8-bit 灰度 PNG;结构化(deflate 比例合理);UI 出现蒙版组件 | P2 | §6.16 |
| TC-AI-13 | 无模型/无 ORT 优雅降级 | 删除模型或 ORT | 触发 AI 功能 | 明确错误提示;不崩溃;非 AI 功能不受影响 | P1 | `AGENTS.md`;§6 清单 |
| TC-AI-14 | CLIP / LaMa 路径 | — | 触发 CLIP 相关 / LaMa 修补 | ORT CPU 正常;`.ms`/`.om` 未覆盖部分如实记录 | P2 | §6.16(不在本批) |
| TC-AI-15 | AI 任务取消 | 推理进行中 | 取消任务 | `cancel_ai_task` 生效;资源释放;可重试 | P2 | `ai_commands.rs:20` |

### 3.13 TC-PERF —— 性能与内存基线

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-PERF-01 | 45MP 编辑器全通基线 | [SIM] 45MP DNG | 打开编辑 + 交互 | 全通;记录 enhancing/downscale/首帧/PSS | P1 | §6.10-3 |
| TC-PERF-02 | 61MP 编辑器内存天花板 | [SIM] 61MP ARW | 打开编辑 | 复现 LMK(日志止于 crop 行);确认属资源边界 | P1 | §6.10-3;§6.11-2 |
| TC-PERF-03 | 交互 FPS 与源 MP 无关 | [SIM] 45MP vs 小图 | 对比交互 FPS | GPU Processor 按预览/ROI 尺寸创建 → FPS 同级 | P2 | §6.10-4 |
| TC-PERF-04 | ≥60MP 真机基线 | [DEV-A/B] 61MP 资产已推送 | 按 §6.10 方法学复测 | 落定真机内存/耗时曲线(8~16GB 预期可过) | P1 | §9.1(待执行) |
| TC-PERF-05 | 加载前资源预检+降级提示 | — | 低内存设备打开大图 | 可见降级提示,消灭闪退观感(C+D 方案) | P2 | §6.11-3(C/D,**未实现**) |
| TC-PERF-06 | 库/胶片栏性能 | [DEV-A] 多图 | 滚动 + 打开 | 缩略图 ~480~580ms;胶片栏全真图 | P2 | §6.13-2 |
| TC-PERF-07 | 预热耗时记录 | 冷缓存 | 启动 | 记录预热耗时(真机 9.98s),确认在后台 | P2 | §6.13-4 |

### 3.14 TC-SIGN —— 签名与上架

> 材料与口令在**未入库**的 `.csr/SIGNING.md`;流程脚本 `.csr/pack-app.ps1`。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-SIGN-01 | HAP 签名与验签 | 签名材料就绪 | `hap-sign-tool sign-app` → `verify-app` | `Find Hap Signing Block success` + `Digest verify result: true` | P0 | §7 Phase 4(-747) |
| TC-SIGN-02 | `.app` 双层签名 | 同上 | `pack-app.ps1` 全流程 | `.app` 级签名块存在且验签通过;内层 HAP 字节校验通过 | P0 | §6.15 |
| TC-SIGN-03 | `.app` 条目名规范化 | 同上 | 检查 `.app` 内条目名 vs pack.info | 条目名严格等于 `packages[].name + ".hap"`(`entry-default.hap`) | P0 | §6.15 AGC 991 因 2 |
| TC-SIGN-04 | AGC 991 三因排除 | 提审包 | 上传前逐项核对 | ①HAP 未拆包丢签名块 ②条目名规范 ③`.app` 带 app 级签名块 | P1 | §6.15 |
| TC-SIGN-05 | 权限与隐私逐项一致 | 提审包(7 权限) | 对比 module.json5 / 隐私说明 / Profile `allowed-acls` | 逐项一致;ACL 受限权限被 Profile 覆盖;无 `READ_IMAGEVIDEO` | P1 | §6.17(问题③) |
| TC-SIGN-06 | release HAP 真机直装被拒 | [DEV-A] release 签名的 HAP | `hdc install -r` | 被拒 `9568322 not trusted app source`(**已知**,迭代用调试签名) | P2 | §7 Phase 4(-747) |
| TC-SIGN-07 | ACL 权限包调试装机被拒 | 带 ACL 权限 + 调试 profile | `hdc install -r` | `9568289 grant request permissions failed`(**已知**,本地验证用 INTERNET-only 变体) | P2 | §6.17 |
| TC-SIGN-08 | 提审包产物校验 | `.app` 已产出 | 校验体积/SHA256/无调试信息 | 与记录一致(如 v3:26.17MB,SHA256 `986387BE…BDA0`) | P1 | §6.17(提审包 v3) |
| TC-SIGN-09 | AGC 控制台名称/图标同步 | AGC 控制台 | 更新为 LightRaw | 与包内一致,避免审核不一致 | P2 | §6.17 注 |
| TC-SIGN-10 | deviceTypes 收敛 | 提审包 | 检查 deviceTypes | 首发为 `["2in1"]`;AGC「支持设备」只勾 2in1 | P2 | §6.15/§6.17 |

### 3.15 TC-REG —— 回归与静态门禁

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-REG-01 | cargo fmt | [HOST] | `cargo fmt -p RapidRAW -- --check` | 通过 | P0 | CI 硬门禁 |
| TC-REG-02 | cargo clippy | [HOST] | `cargo clippy --all-targets --all-features -- -D warnings` | 零警告(Windows 宿主 tethering 不可编译为预存问题) | P0 | CI 硬门禁 |
| TC-REG-03 | tsc | 仓库根 | `npm run typecheck` | 零新增(全仓 70 条为预存基线) | P1 | `AGENTS.md` |
| TC-REG-04 | eslint | 同上 | `npm run lint` | 改动行零命中(23 条为预存) | P1 | `AGENTS.md` |
| TC-REG-05 | prettier | 同上 | `npm run format:check` | 通过 | P1 | `AGENTS.md` |
| TC-REG-06 | i18n | 同上 | `npm run i18n:check` | 通过,零键漂移 | P1 | `AGENTS.md` |
| TC-REG-07 | 桌面应用回归冒烟 | [DESK] 可运行桌面构建 | 启动 → 打开文件夹 → 编辑 → 导出 | 全链路正常(含协议门控、dialog 可用) | P1 | §6.17(**未重测**) |
| TC-REG-08 | Android 回归 | [AND] | 同上 + SAF 路径 | 行为不变(移动端路径、图库导出) | P2 | §6 不变性保证 |
| TC-REG-09 | UI 平滑度对比 | [HOST/桌面] | `bench/replay.js` 前后各一轮 + `analyze.mjs` | 同机前后可对比;viewport 一致;无回归 | P2 | `bench/README.md` |

### 3.16 TC-PRIV —— AppGallery 隐私管理服务

> 代码:`ohos_integration.rs` `privacy` 模块 + 3 个 tauri 命令、`EntryAbility.template.ets`(privacyManager + `PRIVACY_STATE_CHANGED`)、`src/utils/ohosPrivacy.ts`、`SettingsPanel.tsx`(关于页 OHOS 变体);背景与配方见 §6.18。
> **前置**:该服务**不支持模拟器**,须真机;且需已接入标准化隐私声明托管服务,或用 `module.json5` 预置 metadata(§6.18 第 4 节)。

| ID | 名称 | 前置条件 | 步骤 | 预期结果 | 优先级 | 依据 |
|---|---|---|---|---|---|---|
| TC-PRIV-01 | 平台隐私链接查询 | [DEV-A] 已接入/已预置;**首装态需先 `hdc uninstall`** | 打开 设置 → 关于 | app.log `privacy state: {...}` 含 `links[]`;链接行按平台下发的 URL 渲染(隐私政策/用户协议);平台未下发时隐私政策行回退到 AGC 托管声明链接(§6.18 第 4 节) | P1 | §6.18-1/3/4 |
| TC-PRIV-02 | 签署状态查询 | 同上 | ①首装未签署 → ②同意后 → ③撤销后,分别看关于页状态行 | ①「未同意」(托管在用:`links` 非空但 `results` 为空)②「已同意」③「未同意」(`result:0`);重启后状态保持 | P1 | §6.18-3/6.2-3(`ohosConsentOf`)|
| TC-PRIV-03 | 主动拉起标准化隐私弹窗 | 同上 | 关于页 →「重新请求授权」 | 拉起平台弹窗;同意后状态转为「已同意」 | P0 | §6.18-1 |
| TC-PRIV-04 | 撤销同意 | 同上,已同意 | 关于页 →「撤销同意」→ 确认 | `disableService` 成功;状态即时显示「未同意」;下次启动重新弹窗 | P1 | §6.18-1/8 |
| TC-PRIV-05 | 签署结果事件链 | 同上 | 在平台弹窗上同意/拒绝,观察前端 | 无需重进设置页,状态自动刷新(`ohos-privacy-state` 事件 → 重新查询) | P1 | §6.18-1/3 |
| TC-PRIV-06 | 未接入时优雅降级 | 无 metadata、未接入托管的构建 | 打开关于页;再点「打开文件夹」 | 状态显示「未启用」(`1006700003`);取文件夹**仍可用**(fail-open),不崩溃 | P0 | §6.18-2/8 |
| TC-PRIV-07 | 取权前同意门控 | [DEV-A] 已撤销同意/首装未签署 | 点「打开文件夹」/ 导入 | **先**弹平台标准化弹窗;同意后 picker 正常打开(门控放行后原操作继续) | P0 | §6.18-2 |
| TC-PRIV-08 | 拒绝同意后不放行 | 同上 | 在门控弹窗中选择拒绝 | picker **不打开**。**实测行为**:平台直接终止应用进程(hilog `AG/AppGallery: [DisposeAlertAbility]:privacy dialog closed unexpectedly, now kill bundle`),无崩溃栈 —— 属平台策略,排障时勿误判为应用崩溃 | P1 | §6.18-2/6.2-2 |
| TC-PRIV-09 | 门控缓存热路径 | 同上,已同意 | 连续两次取文件夹 | 第二次不再弹窗(缓存),无额外桥往返 | P2 | §6.18-1(`CONSENT_GRANTED`) |
| TC-PRIV-10 | 未上架预置 metadata 生效 | debug 构建 + `hdc install` + 设备可访问应用市场 | 按 §6.18 第 4 节配置后安装,首启 | 弹出统一隐私弹框;接口可查链接/状态 | P1 | §6.18-4 |
| TC-PRIV-11 | 关于页以平台文本取代本地文本 | [DEV-A] | 对比 OHOS 与桌面/Android 的关于页 | OHOS 展示平台链接(不渲染本地法务文本查看器);桌面/Android 不变 | P2 | §6.18-3 |
| TC-PRIV-12 | 桥缺失时不 panic | 缺 EntryAbility 补丁的构建 | 触发任一隐私命令 | 返回 `ok:false` + 错误信息;不 panic、不挂起(请求必被 resolve 一次) | P2 | §6.18-1 |
| TC-PRIV-13 | 非 OHOS 平台返回 Err | [DESK] | 调用三个隐私命令 | 返回明确 Err,不 panic | P2 | `ohos_integration.rs` |

---

## 4. 未覆盖 / 不在本次范围

| 项 | 原因 |
|---|---|
| 相机联机拍摄(tethering) | OHOS 构建不含该 feature(依赖 libgphoto2);桌面专属 |
| 云服务 / 社区页后端 | 非移植范围;仅覆盖 UI 可达性 |
| 桌面三平台全功能验收 | 仅覆盖 cfg 等价性冒烟(TC-REG-07/08) |
| AGC 正式上架流程 | 依赖外部平台审核,人工流程;仅覆盖到提审包校验 |
| 直渲 PoC 性能目标(60 FPS) | 尚未立项,见 TC-EDIT-11 |
| 自动化回归(CI headless) | 仓内无测试框架;Tauri WebDriver 不覆盖 macOS,已明确不做 |

## 5. 用例维护约定

- 新增 OHOS 集成代码须同步补用例;涉及 `gen/ohos/`(gitignored)手工补丁的(EntryAbility / Index.ets / module.json5 / 签名配置 / **`appgallery_privacy_*` 隐私 metadata**),在用例"前置条件"中说明**重打步骤**,并注明 `cargo tauri ohos init` 重跑后失效。
- 涉及 Cargo.lock 冻结的改动,用例须附**双目标复核**(宿主 + `aarch64-unknown-linux-ohos`),见 `AGENTS.md`。
- 证据留存格式:命令原文 + 输出片段 + 截图字节数(界面状态)+ app.log 摘录;证据出处回填到 `TEST_REPORT.md`。
