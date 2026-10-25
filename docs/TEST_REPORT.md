# LightRaw(原 RapidRAW)鸿蒙移植 —— 测试报告

> 报告日期:2026-10-25 · 适用分支:`feature/harmonyos-port`
> 与用例集对应:`docs/TEST_CASES.md`(162 条)
> **报告性质:基于现有验证证据的汇总报告。** 本报告不新增执行,只把 `docs/HARMONYOS_PORTING.md` 中已记录的实证结果、脚本实测结果与静态门禁结果,按用例编号归集为「通过 / 部分通过 / 已知边界 / 未执行」。**未执行"的用例不含结果,后续执行请用 `docs/TEST_REPORT_TEMPLATE.md`。**

---

## 1. 结论(摘要)

| 项 | 结论 |
|---|---|
| **总体判定** | **鸿蒙移植主链路可用**:构建打包、沙箱路径、文件桥、导入/导出、窗口与系统 UI、GPU 渲染、AI(CANN/ORT/NPU)均已真机端到端打通 |
| **发布就绪度** | **基本就绪,差最后一步**:提审包已产出并全链路校验通过;AGC 正式提审未完成(清单见 §5) |
| **最大剩余风险** | ①真机 ≥60MP 基线与 AI 分级测试未跑 ②AGC 权限/隐私一致性实际审核未走完 ③直渲 PoC 未立项(交互性能低于 60 FPS 目标) |
| **稳定性** | 真机多轮构建装测**零新 faultlog**;唯一崩溃档为 GLES 时代历史问题,已由 Vulkan 后端绕行 |

### 1.1 用例执行统计

| 状态 | 数量 | 占比 |
|---|---|---|
| ✅ 通过(有实证证据) | 117 | 72.2% |
| 🟡 部分通过(链路已实现/部分场景已验) | 12 | 7.4% |
| ⚫ 已知边界(已观察并接受,记入缺陷/限制清单) | 6 | 3.7% |
| ⬜ 未执行(待跑,含"实现已完成但设备端未验") | 27 | 16.7% |
| **合计** | **162** | 100% |

> 说明:"通过"中含 26 条 HOST 静态/构建类门禁(CI 可复现)、其余为模拟器与真机实证;真机实证集中于 PATH/FILE/EXPORT/UI/EDIT/AI/SIGN 模块(见各模块证据列)。

### 1.2 模块状态一览

| 模块 | 用例 | ✅ | 🟡 | ⚫ | ⬜ | 模块判定 |
|---|---|---|---|---|---|---|
| TC-BUILD 构建与打包 | 18 | 15 | 1 | 0 | 2 | 通过 |
| TC-PLAT 平台判定与 cfg | 7 | 7 | 0 | 0 | 0 | 通过 |
| TC-PATH 沙箱路径 | 8 | 8 | 0 | 0 | 0 | 通过 |
| TC-FILE FileKit 文件桥 | 15 | 12 | 0 | 0 | 3 | 通过(另存为待验) |
| TC-IMPORT 导入链 | 7 | 3 | 3 | 0 | 1 | 通过 |
| TC-EXPORT 导出链 | 15 | 11 | 0 | 2 | 2 | 通过(含 2 项已知限制) |
| TC-WIN 窗口控制 | 7 | 3 | 1 | 1 | 2 | 通过 |
| TC-UI 系统 UI 适配 | 12 | 10 | 0 | 0 | 2 | 通过 |
| TC-LIB 图库 | 8 | 6 | 1 | 0 | 1 | 通过 |
| TC-EDIT 编辑器 GPU 管线 | 12 | 11 | 0 | 0 | 1 | 通过(直渲待立项) |
| TC-ADJ 调整项 | 12 | 4 | 3 | 1 | 4 | 部分通过(逐项回归未全跑) |
| TC-AI AI 蒙版与降噪 | 15 | 10 | 2 | 0 | 3 | 通过(分级测试未跑) |
| TC-PERF 性能与内存 | 7 | 5 | 0 | 0 | 2 | 部分通过(真机基线未跑) |
| TC-SIGN 签名与上架 | 10 | 6 | 1 | 2 | 1 | 部分通过(提审未完成) |
| TC-REG 回归与静态门禁 | 9 | 6 | 0 | 0 | 3 | 静态全绿;跨平台回归未跑 |

---

## 2. 执行环境与证据来源

| 环境 | 本报告中涉及的实证 |
|---|---|
| **HOST** Windows 11 · Rust 1.98/1.99 · DevEco SDK API 26 | 宿主 + aarch64 + x86_64 目标 `cargo check`;`cargo fmt`/`clippy`;tsc/eslint/prettier/i18n;全量出包(§6.1–6.3、§6.15、§6.16 验证状态) |
| **SIM** DevEco x86_64 模拟器(4 GB · Mali-G77 GLES) | 首亮、FileKit 目录/文件桥、导入/导出、深浅模式、返回手势、GPU 回读路径、NIND 降噪单模型端到端、45/61MP 内存基线 |
| **DEV-A** 真机 HAD-W32(2in1 · Maleoon 916 · 23.3 GiB) | 沙箱路径修复、渲染全链路、缩略图、管线预热、导出直写、持久化 v3、重授权自愈、CANN u2net 端到端、协议门控 |
| **DEV-B** 真机 MateBook Pro S(Maleoon 935 · ≥11776 纹理 · ≥12 GiB) | 模糊 pass 跳过、4096 分块导出、101.9MP 导出 |
| **DESK/AND** | **仅代码等价性核对,未做实机回归冒烟**(见 §4 未执行) |

证据形态:命令原文(§6 各节) · `app.log`(Rust fern) · hilog · faultlog · 截图字节数签判 · CDP 页面内实测 · 导出字节/SHA256 · PSS 采样。

---

## 3. 用例执行明细

### 3.1 TC-BUILD 构建与打包

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-BUILD-01 | ✅ | §6.2 验证记录:宿主检查 EXIT=0(23.26s) | Rust 1.98.1 MSVC |
| TC-BUILD-02 | ✅ | §6.2:OHOS 全量检查 EXIT=0;零 gtk/rfd 泄漏;archmage 三件套未漂移 | 增量复验 12~21s |
| TC-BUILD-03 | ✅ | §6.4 x86_64 交叉链路表;`entry/libs/x86_64` 产物 | — |
| TC-BUILD-04 | ✅ | §6.3:33 个 hvigor 任务全过 → `entry-default-unsigned.hap`(52.4MB) | 收到 ORT 前基线 |
| TC-BUILD-05 | ✅ | §6.15 `-Release` 关键坑与修复;§6.17 release `.so` 40.51MB vs dev 56.44MB | ohrs 阶段需 `-- --release` 透传 |
| TC-BUILD-06 | ✅ | §6.4 模拟器点亮;脚本 `-Target x86_64` | — |
| TC-BUILD-07 | ✅ | §6.12/§6.16 `-SkipBuild` 用于 ORT/CANN 重注入 + 重签装机 | — |
| TC-BUILD-08 | ✅ | §6.12 修复 2;§6.15 单 ABI 清理 | build.rs 按 ABI 映射 |
| TC-BUILD-09 | ✅ | §6.12:补注入 `libc++_shared.so` 后 PANIC 消失 | ORT 的 DT_NEEDED |
| TC-BUILD-10 | ✅ | §6.16:38 库注入,HAP 内压缩 ~2.7MB,`_impl` 链不打包 | — |
| TC-BUILD-11 | ✅ | §6.17:ORT `llvm-strip` 28.4→17.38MB,dynsym 完好;42 库零 DWARF/symtab | — |
| TC-BUILD-12 | ✅ | §6.17:`ets/sourceMaps.map` 确定性删除,0 map 文件 | — |
| TC-BUILD-13 | 🟡 | §6.4-3(双 ABI 120.5MB,已列待办);§6.15 已做单 ABI/清残留 | 通用出包瘦身未默认开启 |
| TC-BUILD-14 | ⬜ | 脚本设计目标 1(未单独实测破坏前置场景) | — |
| TC-BUILD-15 | ✅ | §6.3:看门狗/心跳/成功与失败路径均已实测 | — |
| TC-BUILD-16 | ✅ | §6.7-5 daemon OOM 修复;脚本启动自清 stale daemon | — |
| TC-BUILD-17 | ✅ | §6.7-4/§6.16/§6.17:fmt + clippy `-D warnings` 全绿 | CI 硬门禁 |
| TC-BUILD-18 | ⬜ | build.rs OHOS 分支已实现(缺失仅告警) | 未做"删除 ORT 出包"实测 |

### 3.2 TC-PLAT 平台判定与 cfg 门控

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-PLAT-01 | ✅ | 全仓前端门控(`isOhos`)在真机/模拟器生效 | `ohos_integration.rs:100` |
| TC-PLAT-02 | ✅ | 非 OHOS 构建走原路径;桥命令返回 Err | 代码级确认 |
| TC-PLAT-03 | ✅ | §6.2 坑 1:依赖段 / `lib.rs` / capability 三处一致;OHOS 树零 rfd/gtk | — |
| TC-PLAT-04 | ✅ | §6 不变性保证 + §6.2 宿主无回归 | 双目标 check 佐证 |
| TC-PLAT-05 | ✅ | §6.3 npm 对齐(`@tauri-apps/api` 2.11 ↔ tauri 2.11.5) | — |
| TC-PLAT-06 | ✅ | §6.6-2:`is_ohos_build` 替代 `platform()` | — |
| TC-PLAT-07 | ✅ | §6.17:`i18n:check ✓ 零键漂移`(15 locale) | — |

### 3.3 TC-PATH 沙箱路径

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-PATH-01 | ✅ | §6.13-3 第 3 层根因与修复;`app_paths.rs` 四映射 | 真机 EPERM 修复 |
| TC-PATH-02 | ✅ | §6.13-3;日志落沙箱实证 | — |
| TC-PATH-03 | ✅ | `app_paths.rs` 返回前 `create_dir_all`;真机首启无 EPERM | — |
| TC-PATH-04 | ✅ | `app_paths.rs` 非 OHOS 分支委托 tauri resolver | — |
| TC-PATH-05 | ✅ | §6.13-2 修复后:设置持久化/会话恢复正常 | 全仓 20 处调用点替换 |
| TC-PATH-06 | ✅ | §6.13-3 方法学:真机读 `/data/app/el2/.../files/logs/app.log` | — |
| TC-PATH-07 | ✅ | §6.13-4:scope 追加沙箱缩略图路径,asset 拒绝消失 | — |
| TC-PATH-08 | ✅ | §6.13-3:三连日志(`Failed to load settings`/indexing/LUT)消失 | — |

### 3.4 TC-FILE FileKit 文件桥

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-FILE-01 | ✅ | §6.6-3 hilog(`pick_folder raw uri` → `path` → `stat ok`) | SIM + DEV-A |
| TC-FILE-02 | ✅ | §6.6/§6.14-3 取消分支(不卡死、可重触发) | — |
| TC-FILE-03 | ⬜ | `ohos_integration.rs:78-81` 错误分支已实现 | 未构造缺补丁构建实测 |
| TC-FILE-04 | ✅ | §6.7-1 requestId 注册表;批量导出多请求并发 | — |
| TC-FILE-05 | ✅ | §6.7-2 `collapse_suffix_filters`;>96 字符整组放弃 | — |
| TC-FILE-06 | ⬜ | §6.7-6:"`save_file_as` 路径已实现,设备端验证顺延" | 见 §4 |
| TC-FILE-07 | ✅ | §6.6-3/§6.7-2:`fs.statSync` + Rust `std::fs` 直读;扫描链无错 | — |
| TC-FILE-08 | ✅ | §6.14-5:pick → force-stop → 整机 reboot 全直通 | 2in1 + `FILE_ACCESS_PERSIST` |
| TC-FILE-09 | ⬜ | §6.14-5 双形态矩阵:pad 走 v2 gate | "真 pad 待验" |
| TC-FILE-10 | ✅ | §6.14-2 `check_paths_readable`;探针与扫描语义解耦 | — |
| TC-FILE-11 | ✅ | §6.14-3:三种触发 × 3 全通(toast → picker → 库恢复) | 真机实证 |
| TC-FILE-12 | ✅ | §6.14-3:取消 → 回欢迎页不卡死,第二轮恢复 | — |
| TC-FILE-13 | ✅ | §6.14-2:部分活根只恢复活的 | — |
| TC-FILE-14 | ✅ | §6.14-3:修复前 1 条/启动 → 修复后 0 条 | — |
| TC-FILE-15 | ✅ | §6.14-5:`ohos_grants.json` 按 uri 去重;onCreate activate | — |

### 3.5 TC-IMPORT 导入链

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-IMPORT-01 | ✅ | `MainLibrary.tsx:624` 门控;真机/模拟器导入 FAB 可用 | — |
| TC-IMPORT-02 | ✅ | §6.7-4:`pick_files -> 1 file(s)` → 缩略图 +「导入完成!」 | SIM |
| TC-IMPORT-03 | 🟡 | 前端 validFiles 兜底为设计;未构造越界扩展名实测 | — |
| TC-IMPORT-04 | ⬜ | `ImagePicker.tsx:22-25` 分支已实现 | 未单独设备端验证 |
| TC-IMPORT-05 | 🟡 | `LUTControl.tsx:149` 接桥;picker 主流程已通 | LUT 专项未单独取证 |
| TC-IMPORT-06 | 🟡 | `PresetsPanel.tsx:1040` 接桥 | 预设专项未单独取证 |
| TC-IMPORT-07 | ✅ | §6.7-2:媒体库路径(`/storage/media/...`)Rust 直读实证 | — |

### 3.6 TC-EXPORT 导出链

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-EXPORT-01 | ✅ | §6.7 成果:`save_to_gallery ok: file://media/Photo/…`;图库「来自应用」可见 | SIM |
| TC-EXPORT-02 | ✅ | §7 Phase 2(-730):直写公共目录优先,侧车同树写入 | DEV-A |
| TC-EXPORT-03 | ✅ | §7 Phase 2(-730):"目录被记忆、二次导出免选" | — |
| TC-EXPORT-04 | ✅ | §7 Phase 2(-730):Rust 临时目录改 `export_temp` 后 13900002 消失 | — |
| TC-EXPORT-05 | ✅ | §6.7-3:13900015 根因(`mkdirSync` 非幂等)+ `fs.accessSync` 探测修复 | — |
| TC-EXPORT-06 | ✅ | §6.7-3:14000011 根因(URI 不规范)+ 沙箱中转修复 | 系统侧日志取证 |
| TC-EXPORT-07 | ⬜ | §6.7-3 错误码语义已实现(-3006/-2004/-203;空数组=拒绝) | 未构造拒绝场景实测 |
| TC-EXPORT-08 | ⬜ | §6.7-3:蒙版 alpha PNG 走保存 picker(实现) | 未设备端取证 |
| TC-EXPORT-09 | ⬜ | §6.7-6:顺延 | 同 TC-FILE-06 |
| TC-EXPORT-10 | ⚫ | §7 Phase 3(-729/-741):>8192px 走 CPU 旁路丢弃调整(**设备纹理上限差异,预存在**) | Maleoon 916;935 不受影响 |
| TC-EXPORT-11 | ✅ | §7 Phase 2(-730):`8736x11656 (101.9MP)` JPEG 直落目标目录 | DEV-B |
| TC-EXPORT-12 | ✅ | §7 Phase 3(-742):tile 边界三重验证 + 916 真机复核(数值分析排除接缝) | DEV-B/DEV-A |
| TC-EXPORT-13 | ⚫ | §6.7-6:每张各弹一次对话框(`save_request_blocking` 串行) | **已知限制** |
| TC-EXPORT-14 | ⚫ | §6.7-6:库视图导出成功无 toast;编辑器视图未复测 | **已知缺陷** |
| TC-EXPORT-15 | ✅ | §6.10-2/§6.11:`Batch Export: 4 cores, 1.0 GB free RAM -> 1 threads` | 自适应生效 |

### 3.7 TC-WIN 窗口控制

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-WIN-01 | ✅ | §6.5-5 表:系统 最大化/最小化/关闭/还原 逐项 ✓ | SIM |
| TC-WIN-02 | ✅ | §6.5-5:swipe (1200,405)→(1600,405) 窗口左缘 +400px(WMS 日志) | SIM |
| TC-WIN-03 | ✅ | §6.5-4:`{isLinux && !isOhos && …}`;真机观察系统条 + 自绘标题 | — |
| TC-WIN-04 | 🟡 | `ohos_integration.rs:109` 走 `terminateSelf`(实现) | 未单独做异常死亡对比取证 |
| TC-WIN-05 | ⬜ | 非 OHOS 返回 Err(代码) | 未实测 |
| TC-WIN-06 | ⬜ | §6.5-4 休眠保留(设计) | 未单独验证 |
| TC-WIN-07 | ⚫ | §6.5-7:浅色主题系统条断层;系统条应用名与自绘标题重复 | **已知瑕疵** |

### 3.8 TC-UI 系统 UI 适配

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-UI-01 | ✅ | §6.8-3:主题下拉出现「系统」,i18n 生效 | SIM |
| TC-UI-02 | ✅ | §6.8-3:`dark=true/false` 日志 + UI 全域双向翻转 | SIM |
| TC-UI-03 | ✅ | §6.8-1 splash 主题解析 + `GetOhosColorMode` | SIM |
| TC-UI-04 | ⬜ | `prefers-color-scheme` 兜底(实现) | 未构造桥失效场景 |
| TC-UI-05 | ✅ | §6.8-3:编辑器 → 图库 | SIM(`uitest uiInput keyEvent Back`) |
| TC-UI-06 | ✅ | §6.8-3:模态关闭优先,不退出图库 | SIM |
| TC-UI-07 | ✅ | §6.8-3:图库 → 主页(Escape 链) | SIM |
| TC-UI-08 | ✅ | §6.8-1:avoid area 日志 | SIM |
| TC-UI-09 | ✅ | §6.17:`bm clean -d` 冷启 ×2 零弹窗直进欢迎页 | DEV-A |
| TC-UI-10 | ⬜ | §6.17:"桌面/Android 门控行为未重测" | 见 §4 |
| TC-UI-11 | ✅ | §6.13/§6.14:CDP 实证 ls/ss=object、读写往返、`Object.keys` 不抛 | SIM |
| TC-UI-12 | ✅ | §6.4:仅「添加到桌面」常规提示,无崩溃/误弹窗 | SIM |

### 3.9 TC-LIB 图库

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-LIB-01 | ✅ | §6.6-3:GetFolderTree/ListImagesInDir 无错;树与网格渲染 | SIM + DEV-A |
| TC-LIB-02 | ✅ | §6.13-2:库 2×2 网格 + 胶片栏 4/4 真实缩略图 | DEV-A |
| TC-LIB-03 | ✅ | §6.13-2:重装后「继续会话」页出现;设置/会话恢复 | DEV-A |
| TC-LIB-04 | ✅ | §6.6-4:路径 truncate 为显示假象,树高亮为准 | SIM |
| TC-LIB-05 | 🟡 | EXIF/`.rrcache` 链路随库缩略图跑通;元数据面板未单独取证 | — |
| TC-LIB-06 | ✅ | §6.10-3:61MP ARW 走内嵌预览,无去马赛克日志行 | SIM |
| TC-LIB-07 | ✅ | §6.10-3:45MP DNG 无内嵌预览 → 全管线正常 | SIM |
| TC-LIB-08 | ⬜ | 缩略图 worker/队列实现;未见专项取证 | — |

### 3.10 TC-EDIT 编辑器 GPU 管线

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-EDIT-01 | ✅ | §6.8-3 `Using GPU adapter: Mali-G77 (backend: Gl, …)`;§6.13-2 `Maleoon 916 (backend: Vulkan, …)` | — |
| TC-EDIT-02 | ✅ | §6.8-3:`Creating new GPU Processor` + `[apply_adjustments] … on GPU in …ms` | — |
| TC-EDIT-03 | ✅ | §6.8-3 交互 28.71 FPS;§6.10-3 14.2~29.2 FPS;§6.13-2 286~353ms/趟 | — |
| TC-EDIT-04 | ✅ | §6.8-3:曝光 4.25 → 月亮过曝泛白(全链路) | SIM |
| TC-EDIT-05 | ✅ | §6.10-3 首帧 244~320ms;§6.13-2 578~648ms | — |
| TC-EDIT-06 | ✅ | §6.13-4:预热 9.98s 后台吸收;首作业 588ms(修复前 11.16s) | DEV-A 冷缓存 |
| TC-EDIT-07 | ✅ | §6.8-3:`[ERROR] Fake map` 良性,渲染正常 | — |
| TC-EDIT-08 | ✅ | §6.13-1 修复:`VULKAN \| GL` 默认后端,真机命中 Vulkan | — |
| TC-EDIT-09 | ✅ | §6.13-1/2:修复后三轮装测零新 faultlog | 历史崩溃档停在 GLES 时代 |
| TC-EDIT-10 | ✅ | §6.13-2:vendor ash 分支;Vulkan loader 加载成功 | — |
| TC-EDIT-11 | ⬜ | §6.9 GO(有条件);§9.4 未立项 | 见 §4 |
| TC-EDIT-12 | ✅ | §6.7-4 / §6.16:编辑器内导出/蒙版链路可用 | — |

### 3.11 TC-ADJ 调整项

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-ADJ-01 | 🟡 | §6.7-4 导出含 AgX/曝光 0.63/对比度 16/阴影 22;§6.8-3 曝光 | 未逐项回归 |
| TC-ADJ-02 | ✅ | §6.7-4 编辑含曲线并成功导出 | — |
| TC-ADJ-03 | ⬜ | HSL/色轮/白平衡未见专项取证 | — |
| TC-ADJ-04 | ⬜ | 锐化/传统降噪以模糊 pass 形式被 §7(-741)覆盖(shader 零值门限) | 单独视觉未取证 |
| TC-ADJ-05 | ✅ | §7 Phase 3(-741/-742):tonal+clarity 真机生效并被精确对照 | — |
| TC-ADJ-06 | ✅ | §7 Phase 3(-741):跳过 sharpness+structure,**导出字节数完全一致**(77,103,473)+ 视觉无差异 | 强等价证据 |
| TC-ADJ-07 | ✅ | §7 Phase 3(-741/-742):蒙版逐像素混入 t_*;全零蒙版不消耗模糊 | — |
| TC-ADJ-08 | 🟡 | §7 Phase 1:lensfun_db 4.99MB 已打包(`<lensdatabase` 二进制确证) | 校正视觉未专项取证 |
| TC-ADJ-09 | 🟡 | 导出实测含几何/裁剪链路(§6.7-4、§7-742 子区域裁剪数学) | 引导透视 UI 未专项取证 |
| TC-ADJ-10 | ⬜ | 未取证 | — |
| TC-ADJ-11 | ⚫ | §6.7-6:调整面板直方图 canvas 无选中时渲染噪点/空白帧 | **已知缺陷,待查** |
| TC-ADJ-12 | ⬜ | 复制粘贴模态在 §6.8-3 作为返回手势分支被间接触及 | 功能本身未取证 |

### 3.12 TC-AI AI 蒙版与降噪

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-AI-01 | ✅ | §6.12:5 模型 ~560MB 全部落盘 | SIM |
| TC-AI-02 | ✅ | §6.12-1:3 次 rename EACCES 均由拷贝回退救回(app.log WARN);§6.13 保留回退 | 必须常驻 |
| TC-AI-03 | ✅ | §6.12 修复 2:注入后 `Session::builder` 无 PANIC | — |
| TC-AI-04 | ✅ | §6.12:分块推理 → 对比 UI → `small_test_Denoised.png` + `.rrdata` | SIM 单模型 |
| TC-AI-05 | ⬜ | §9.1:5 模型蒙版真机分级测试待执行(23GiB 内存充裕) | 见 §4 |
| TC-AI-06 | ✅ | §6.16:T1–T7 每轮失败均优雅回退、正常出蒙版、无崩溃 | DEV-A 6 轮 |
| TC-AI-07 | ✅ | §6.16 T8:触发 → 蒙版落盘 4.85s;NPU 时钟/CPU 分区/ORT 零回退三重实证 | DEV-A |
| TC-AI-08 | ⬜ | §6.16 残留⑤:nind `.om` 路径真机未测 | 见 §4 |
| TC-AI-09 | ✅ | §6.16 路由:契约校验失败 → 回退 ORT(T1–T7 实证) | — |
| TC-AI-10 | ✅ | §6.16:38 库定版,无跨 ns REJECT;`_impl` 链不打包 | — |
| TC-AI-11 | ✅ | `om::init_options` setter/Get 复核;T8 日志 perf 4 | — |
| TC-AI-12 | ✅ | §6.16:9202×12122 8-bit 灰度 PNG,deflate 41.2:1;UI 出现蒙版组件 | — |
| TC-AI-13 | 🟡 | build.rs 缺失 ORT 仅告警(设计);ORT 缺失时 AI 降级 | 未单独构造无 ORT 装机 |
| TC-AI-14 | 🟡 | §6.16:CLIP(`.ms`)/SAM3/LaMa(ORT CPU)不在本批;ORT 回退路径已被 6 轮验证 | 见 §4 |
| TC-AI-15 | ⬜ | `cancel_ai_task` 未见设备端取证 | — |

### 3.13 TC-PERF 性能与内存基线

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-PERF-01 | ✅ | §6.10-3:45MP 全通(Raw enhancing 983ms、PSS 1.76GiB、交互 14.2~29.2 FPS) | SIM |
| TC-PERF-02 | ✅ | §6.10-3:61MP 编辑器 LowMemoryKill(日志止于 crop);§6.11-2 峰值解剖 1.59GB | SIM 4GB 天花板 |
| TC-PERF-03 | ✅ | §6.10-4:GPU Processor 按预览/ROI 尺寸创建 → 交互 FPS 与源 MP 无关 | 结构性结论 |
| TC-PERF-04 | ⬜ | §9.1:资产已在真机 Download,方法学已固化 | **主阻塞项** |
| TC-PERF-05 | ⬜ | §6.11-3 C+D 方案(未实现) | 见 §4 |
| TC-PERF-06 | ✅ | §6.13-2:缩略图 1280×853 480~580ms;胶片栏全真图 | DEV-A |
| TC-PERF-07 | ✅ | §6.13-4:`[pipeline-warmup] completed in Xs`(真机 9.98s) | — |

### 3.14 TC-SIGN 签名与上架

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-SIGN-01 | ✅ | §7 Phase 4(-747):sign-app/verify-app 全通;signed.hap 已产出 | — |
| TC-SIGN-02 | ✅ | §6.15:`.app` 双层签名 + `Find Hap Signing Block success` + `Digest verify result: true` | — |
| TC-SIGN-03 | ✅ | §6.15 AGC 991 因 2:条目名严格 `entry-default.hap` | — |
| TC-SIGN-04 | ✅ | §6.15:三因全破,通过 AGC 包级校验 | — |
| TC-SIGN-05 | 🟡 | §6.17:7 权限定版 + `.agc-backup`(SHA256 `43E9CB4B…`)+ 全链路复核 | **AGC 实际审核未完成** |
| TC-SIGN-06 | ⚫ | §7 Phase 4(-747):`9568322 not trusted app source`(复现) | 迭代用调试签名 |
| TC-SIGN-07 | ⚫ | §6.17:`9568289 grant request permissions failed: READ_IMAGEVIDEO`(复现) | 本地用 INTERNET-only 变体 |
| TC-SIGN-08 | ✅ | §6.17 提审包 v3:`entry-default.app` 26.17MB,SHA256 `986387BE…BDA0` | 42 库零 DWARF/0 map |
| TC-SIGN-09 | ⬜ | §6.17 注:AGC 控制台名称/图标须同步改 LightRaw | 待办 |
| TC-SIGN-10 | ✅ | §6.15:`deviceTypes` 收敛 `["2in1"]` | gen/ 本地修改 |

### 3.15 TC-REG 回归与静态门禁

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| TC-REG-01 | ✅ | §6.16/§6.17:fmt `--check` ✓ | — |
| TC-REG-02 | ✅ | §6.16:clippy `-D warnings` ✓ 零新告警 | Windows 宿主 tethering 不可编译为预存 |
| TC-REG-03 | ✅ | §6.17:改动文件 0 错误(全仓 70 条为预存基线) | — |
| TC-REG-04 | ✅ | §6.17:23 条 eslint 全预存,改动行号区间零命中 | — |
| TC-REG-05 | ✅ | §6.8-4/§6.7-4:prettier 零新增 | — |
| TC-REG-06 | ✅ | §6.17:`i18n:check ✓ 零键漂移` | — |
| TC-REG-07 | ⬜ | §6.17:"桌面/Android 门控行为未重测(需动本机桌面应用数据)" | **建议尽快补** |
| TC-REG-08 | ⬜ | 同上(Android 未回归) | 建议补 |
| TC-REG-09 | ⬜ | `bench/README.md` 方法就绪,未见本轮记录 | — |

---

## 4. 未执行用例清单(27 条)与执行建议

| 优先级 | 用例 | 阻塞 / 说明 | 建议 |
|---|---|---|---|
| P0 | TC-FILE-06、TC-EXPORT-09(另存为/`save_file_as`) | §6.7-6 顺延 | 设备就绪即跑;同模式复制,预计一次通过 |
| P0 | TC-SIGN-05(权限/隐私一致性实际审核) | 依赖 AGC 审核 | 上架前置;按 §6.15 三方对齐核对后提交 |
| P1 | TC-AI-05(5 模型蒙版分级)、TC-AI-08(nind `.om`) | §9.1/§6.16 残留⑤ | DEV-A 内存充裕,一次真机 session 可覆盖 |
| P1 | TC-PERF-04(≥60MP 真机基线) | 资产已推送,方法学已固化 | 用 §6.10 命令序列复测,不依赖开发 |
| P1 | TC-REG-07 / TC-REG-08(桌面/Android 回归冒烟) | 需本机桌面应用数据 | 与 TC-UI-10 合并一轮跑,验证协议门控与 dialog 未受影响 |
| P1 | TC-IMPORT-04/05/06、TC-EXPORT-07/08、TC-LIB-05/08、TC-ADJ-03/04/09 | 链路已实现但无专项取证 | 一至两轮真机定向取证即可闭合 |
| P2 | TC-BUILD-14/18、TC-FILE-03/09、TC-WIN-05/06、TC-UI-04、TC-AI-15、TC-ADJ-10/12、TC-SIGN-09、TC-REG-09、TC-PERF-05、TC-EDIT-11 | 设计/方案已就绪或需立项 | 随相应功能 Iteration 顺带执行 |

---

## 5. 缺陷与限制清单

> 分级:P0 阻塞发布 / P1 重要 / P2 次要。状态为"截至报告日"。

| 编号 | 描述 | 关联用例 | 严重度 | 状态 |
|---|---|---|---|---|
| D-01 | 调整面板直方图 canvas 无选中时渲染噪点/空白帧(疑似 GPU readback 或未初始化缓冲) | TC-ADJ-11 | P1 | **打开**(待查,非导出链引入) |
| D-02 | 库视图导出成功无 toast;编辑器视图导出未复测 | TC-EXPORT-14 | P2 | 打开 |
| D-03 | 批量导出每张各弹一次系统对话框(需评估批量授权或收集式 UX) | TC-EXPORT-13 | P2 | 打开(设计限制) |
| D-04 | 浅色主题下系统装饰条与深色应用视觉断层;系统条应用名与自绘标题重复 | TC-WIN-07 | P2 | 打开(外观) |
| D-05 | 4GB 模拟器:61MP 编辑器 LMK、45MP 全分辨率导出进程死亡(低于上游 16GB 推荐基线) | TC-PERF-02 | P1 | 已知边界;**真机待验**(预期可过) |
| D-06 | >8192px 导出走 CPU 旁路丢弃调整(Maleoon 916 `max_texture_dimension_2d=8192`) | TC-EXPORT-10 | P2 | 已知边界(设备纹理上限,预存在) |
| D-07 | 真机 GLES 回读路径 100% 崩溃(Maleoon `libhvgr` 栈溢出) | TC-EDIT-09 | P1 | **已绕行**(默认 `VULKAN\|GL`);GLES 直渲不可用 |
| D-08 | release HAP 真机直装被拒 `9568322`(仅 AGC 渠道信任) | TC-SIGN-06 | P2 | 已知边界(迭代用调试签名) |
| D-09 | 带 ACL 受限权限包调试装机被拒 `9568289` | TC-SIGN-07 | P2 | 已知边界(本地用 INTERNET-only 变体) |
| D-10 | tao-ohos 窗口操作仍为 stub(`set_minimized`/`set_maximized`/`set_fullscreen`/`drag_window`) | TC-WIN-06 | P2 | fork 级待解(应用层已用桥绕过) |
| D-11 | 双 ABI `.so` 同包导致 HAP 偏大(strip 后 120.5MB) | TC-BUILD-13 | P2 | 打开(发布前瘦身) |
| D-12 | pad 形态每次冷启动需一次重授权点击(设备能力所限) | TC-FILE-09 | P2 | 已知边界(v2 即最终形态) |
| D-13 | `.om` 逐像素精度未验证(fp16 有效精度;内容 sanity 已过) | TC-AI-12 | P2 | 打开(低风险) |
| D-14 | `load_api` 的 hcl 仍走 `indirect/` 绝对路径;bash MUSL-LDSO 良性警告 | TC-AI-10 | P2 | 良性,可清理 |
| D-15 | `gen/ohos/` 手工补丁(EntryAbility/Index.ets/module.json5/签名配置/权限声明)在 `cargo tauri ohos init` 重跑后丢失 | TC-BUILD-04/06、TC-SIGN-10 | P1 | **流程风险**;模板已固化 `src-tauri/ohos/*.template.ets`,其余需手动重打 |
| D-16 | 升级安装会重置导出面板质量设置为默认 q95(普通重启不重置) | TC-EXPORT-03 | P2 | 打开 |

## 6. 剩余风险登记(未构成缺陷)

| 风险 | 说明 | 缓解 |
|---|---|---|
| Fork 依赖 | Tauri `feat/open-harmony` 未合入主干,未升级到 2.12 | 锁 commit;备选 ArkWeb+NAPI 路线 |
| Cargo.lock 冻结 | archmage 三件套 / `windows 0.62.2` 手锚,误 `cargo update` 会破坏 aarch64 构建 | `AGENTS.md` 纪律 + 定向包名更新 + 双目标复核 |
| 交互性能 | 真机回读路径 ~290ms/趟,远低于 60 FPS 目标 | 直渲 PoC(TC-EDIT-11)已评估 GO,待立项 |
| AI 模型分发 | `.om` 发布形态未定(HAP 内置 150MB / AGC 通道 / 检测下载) | 待定;38 库 ~5.6MB 为固定载荷 |
| 真机覆盖面 | 目前仅在 2in1 形态充分验证;pad 形态与 61MP 未跑 | 见 §4 |

---

## 7. 报告签署(待填)

| 角色 | 姓名 | 日期 | 结论 |
|---|---|---|---|
| 测试执行 | | | |
| 开发确认 | | | |
| 发布决策 | | | |

> 后续每一轮完整执行请使用 `docs/TEST_REPORT_TEMPLATE.md`,并将本轮结果归档为 `docs/reports/TEST_REPORT_<yyyy-mm-dd>.md`。
