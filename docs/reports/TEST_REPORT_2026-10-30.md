# LightRaw 鸿蒙移植 —— 2026-10-30 改动面全量回归测试报告

> 报告编号：TEST-20261030-01
> 执行日期：2026-10-30 21:40 ~ 22:20（北京时间）
> 被测版本：`v1.6.7` @ `8b063bde`（工作区含 2 处本地改动，见 §2）
> 用例来源：`docs/TEST_CASES.md`（本次按"今日改动面"取子集，并补充发布包与双机验证）
> 设备：X90 `3XB0124617000013`（HAD-W32 / MateBook Pro）、XE90 `7QB0126509000017`（MOR-M1 / MateBook Pro S）
> 结论：**通过**（1 项需修复的静态门禁缺陷已修；2 项未执行项见 §7）

---

## 1. 本次被测改动

| 提交 | 内容 | 本次覆盖方式 |
|---|---|---|
| `f9968706` | 启动中断（plugin-http 走桌面 XDG 路径 → EPERM → `PluginInitialization("http")` 中止）；AI 模型/日志改走 `crate::app_paths` 沙箱；`@tauri-apps/plugin-http` 2.7.0 对齐 | 双机全新安装 + 冷启动 + 沙箱路径实证 |
| `ba733105` | 外链改走 `open_external_url` → ArkTS `viewData`；文件夹授权 `revoke_ohos_folder_grants`；隐私撤销回读验证、按钮禁用 | 双机点链 + 合成授权记录撤销 |
| `d294b3c6` | ONNX Runtime 改为源码编译（`scripts/build-ort-ohos.ps1` + `vendor/onnxruntime`） | `-Release` 全量出包 + 装机 .so SHA256 比对 |
| `99bc29c6` ~ `8b063bde` | 文档 / AI 模型清单（无运行时改动，除 `ai_processing.rs` 注释与 `ohos_integration.rs` 注释） | 编译门禁覆盖 |
| **本地改动 1** | `src/components/panel/MainLibrary.tsx`：首页「Contribute on GitHub」→ `https://github.com/Saberlion/LightRaw` | 双机点击实测 |
| **本地改动 2** | `src-tauri/src/ai_processing.rs`：删除今日遗留的 `use tauri::Manager;`（§6 D-01） | 宿主 clippy 复跑 |

---

## 2. 环境

| 角色 | 设备 | 系统 / API | 内存 | 备注 |
|---|---|---|---|---|
| HOST | Windows 11 · Rust 1.98 · Node 24 · DevEco SDK API 26 | — | — | 构建与静态门禁 |
| DEV-A | X90 `3XB0124617000013` · HUAWEI MateBook Pro（HAD-W32） | HarmonyOS 7.0.0.17(SP20C00E1R13P3log) · API 26 | 24.4 GB | 全新卸载重装 |
| DEV-B | XE90 `7QB0126509000017` · HUAWEI MateBook Pro S（MOR-M1） | HarmonyOS 7.0.0.115(C00E101R9P2log) · API 26 | 24.4 GB | 全新卸载重装 |

产物：`src-tauri/gen/ohos/entry/build/default/outputs/default/entry-default-signed.hap`（52.81 MB，`-Release`）
装机包：同内容 **诊断变体**（`module.json` 权限降为 INTERNET-only + `debugKey` 调试材料重签，`.csr/SIGNING.md` §7 配方），因为 release profile 签名包直装被拒（9568322）、7 权限含 ACL 被拒（9568289）。

---

## 3. 静态门禁（TC-REG-01/02/03/04/05/06、TC-BUILD-17）

| 门禁 | 结果 | 说明 |
|---|---|---|
| `cargo fmt -p RapidRAW -- --check` | ✅ | EXIT=0 |
| `cargo clippy --all-targets -- -D warnings`（宿主） | ✅ | 修复 D-01 后 EXIT=0 |
| `cargo clippy --all-targets --all-features -- -D warnings` | ⚠️ 不适用 | Windows 无 `libgphoto2`，`camera_tethering.rs` 18 处 `E0433: gphoto2`；文档 §6.19 已注明宿主侧不得带 `--all-features` |
| `cargo check --target aarch64-unknown-linux-ohos` | ✅（等价） | 未单独跑 check：`-Release` 全量构建完成编译 + 链接 + 打包，强于 check |
| `npm run typecheck` | 🟡 基线 | 65 条 TS 错误，全部为既有基线；改动文件 0 命中 |
| `npm run lint` | 🟡 基线 | 849 error / 45 warning（既有基线，权威判定为 advisory）；`MainLibrary.tsx` 问题均为 `no-explicit-any` 既有项，与改动行无关 |
| `npm run format:check` | 🟡 基线 | 6 个文件既有格式差异；`MainLibrary.tsx` 在改动前即不通过（已 `git stash` 对照） |
| `npm run i18n:check` | ✅ | 15 locale、1628 复数解析、27488 占位符全部通过，零键漂移 |

---

## 4. 构建与产物（TC-BUILD-05/08/09/10/11/12）

| 检查项 | 结果 |
|---|---|
| `scripts/build-ohos.ps1 -Release` | ✅ hvigor `BUILD SUCCESSFUL in 7m 20s`（首轮因 D-02 失败，清理后重跑；ORT 复用 10-30 18:xx 源码构建产物） |
| 产物 | `entry-default-signed.hap` 52.81 MB（54 条目） |
| `librapidraw_lib.so` | 44.03 MB（release，strip 后；debug 版为 388 MB 级） |
| 打包 `.so` 调试信息 | 42 个 `.so` 中 **0** 个含 `.debug_*` / `.symtab`（`llvm-readelf -S`） |
| `ets/sourceMaps.map` | 不存在 |
| ORT 注入 | `libs/arm64-v8a/libonnxruntime.so` 14 435 296 B，`onnxruntime-source.txt` = `v1.22.0-from-source`；`libc++_shared.so` 一并注入 |
| CANN 注入 | 38 个 `libai_*` / 依赖 `.so` 全量注入 |
| 权限集 | 7 项（INTERNET、FILE_ACCESS_PERSIST、WRITE_IMAGEVIDEO、READ_WRITE_DOWNLOAD/DOCUMENTS/DESKTOP_DIRECTORY、MEDIA_LOCATION） |
| 新 napi 桥导出 | `resolveOhosOpenUrl` / `resolveOhosRevokeGrants` / `open_url` / `revoke_folder_grants` 在 strip 后 `librapidraw_lib.so` 中均存在 |

**装机侧复核（双机一致）**：`/data/app/el1/bundle/public/io.github.CyberTimon.RapidRAW/libs/arm64/` 下 `libonnxruntime.so` = 14 435 296 B，SHA256 `2B4C3AFD…19736`，与本机 `src-tauri/libs/ohos/arm64-v8a/libonnxruntime.so` **完全相同**；`libc++_shared.so` 1 262 504 B；`librapidraw_lib.so` 46 168 088 B。

---

## 5. 真机执行

| 用例 | 场景 | DEV-A X90 | DEV-B XE90 | 证据 |
|---|---|---|---|---|
| TC-BUILD-07 | 诊断包安装 | ✅ `install bundle successfully` | ✅ 同左 | `hdc install -r` |
| TC-UI-09 / 启动修复 | 冷启动不中止 | ✅ `aa start` → pid 存活，欢迎页渲染 | ✅ 同左 | `pidof`；`app.log` 无 `PluginInitialization` |
| TC-EDGE/启动 | 启动崩溃回归 | ✅ 装机后（21:58 起）无新 cppcrash；faultlog 仅 18:29/18:30/18:33 三条**修复前**记录 | ✅ faultlog 为空 | `/data/log/faultlog/faultlogger` |
| 渲染链路 | GPU 后端 | ✅ `Maleoon 916 (Vulkan)`，`pipeline-warmup 21.57s` | ✅ `pipeline-warmup 3.29s` | `app.log` |
| TC-PATH-01/02/06 | 沙箱目录 | ✅ `files/{logs/app.log,config,appdata}`，`appdata/` 内 `settings.json`、`albums/`、`luts/` | ✅ 同左 | `ls`/`stat`（属主 20020196 / 20020177） |
| TC-PATH-05/08 | 设置读写无 EPERM | ✅ `app.log` 中 error/panic 计数 0 | ✅ 同左 | `grep -ic` |
| TC-UI-06.19-a | 首页 GitHub 外链（**本次改动**） | ✅ 系统浏览器打开，地址栏 `github.com/Saberlion/LightRaw` | ✅ 同左 | `uitest dumpLayout -i` 地址栏文本 |
| TC-UI-06.19-b | 关于 → 隐私政策 | ✅ 浏览器地址 `agreement-drcn.hispace.dbankcloud.cn/index.html?lang=en&agreementId=2068880948149116608`（新 agreementId 生效） | 未点（机制同源） | 同上 |
| TC-UI-06.19-c | 外链无 shell 报错 | ✅ `Scoped shell IO error` / `Failed to open external URL` / `Unhandled promise rejection` 计数 **0** | ✅ 计数 0 | `app.log` |
| TC-UI-06.19-d | 关于 → 源代码（**增补改动**，23:02 包） | ✅ 浏览器地址 `https://github.com/Saberlion/LightRaw` | ✅ 同左 | 布局 dump 地址栏文本；`app.log` 链接错误 0 |
| TC-PRIV-06 | 未接托管时降级 | ✅ 状态 `Not enabled`；打开文件夹照常 | ✅ 同左 | 关于页布局 dump |
| TC-PRIV-04（6.20-a） | 无托管时按钮禁用 | ✅ 「Request consent」「Revoke consent」`enabled=false` | ✅ 同左 | 布局 dump `enabled` 属性 |
| TC-PRIV（6.20-b） | 移除根目录 → 收回授权 | ✅ hilog `revoke_folder_grants: 0 revoked, 1 error(s)`，`ohos_grants.json` → `[]` | ✅ 同左（`0 revoked, 1 error(s)`，记录 → `[]`） | hilog + 记录文件 |
| TC-PRIV（6.20-c） | 记录不复活 | ✅ 重启后无 `activated … persisted folder grant(s)`，记录仍 `[]` | ✅ 同左 | 重启后 hilog/记录 |
| TC-LIB / TC-EDIT | 图库 + 编辑器 | ✅ 3 张图（`.tiff`/`.fff`/`.jpg`）缩略图与 EXIF 渲染；双击进入编辑器 | 🟡 仅空目录（本机无样例图），图库空态正常 | 布局 dump |
| TC-AI-03（ORT 就位） | ORT 随包安装 | ✅ 装机 lib 目录 SHA256 与源码构建产物一致 | ✅ 同左 | `sha256sum` |

说明：folder-grant 用例用**合成记录**驱动（诊断包无 `FILE_ACCESS_PERSIST` ACL，`persistPermission` 无法成功，与文档 §6.20 的既有做法一致）；`0 revoked, 1 error(s)` 中 1 个错误即 `deactivatePermission` 对不存在的持久授权报错，符合预期。

---

## 6. 缺陷

| 编号 | 现象 | 严重度 | 处置 |
|---|---|---|---|
| D-01 | `src-tauri/src/ai_processing.rs:25` 残留 `use tauri::Manager;`（`f9968706` 改掉唯一使用点 `.path()` 后未删），宿主 `clippy -D warnings` 报 `unused import`，**CI 硬门禁会红** | P1 | **已修复**（本次删除该 import，clippy 复跑 EXIT=0） |
| D-02 | `-Release` 首轮失败：`aws-lc-sys` 的 `CMakeCache.txt` 仍指向旧仓库路径 `c:/workspace/RapidRAW-hmos`（仓库改名遗留），`CMake Error … directory is different` | P2（环境） | **已处理**：删除该 triple 下的 `aws-lc-*` build 目录后重建成功。建议把"改仓库路径后清理 `target/<triple>/release/build`"写入文档 |
| D-03 | `.csr/decrypt-passwords.ps1` 内硬编码的密文与当前 `build-profile.json5` 的 debug material 不一致，解密得到错误口令 → `sign-app` 报 `11014003 keystore password was incorrect` | P2（工具） | 本次以 `build-profile.json5` 实际密文重新解密得到口令。建议修正脚本或改为从 `build-profile.json5` 读取 |
| D-04 | 前端质量门基线漂移：`typecheck` 65 错误、`lint` 849 错误、`prettier` 6 文件（TEST_CASES 记录为 typecheck 70 / lint 23） | P2（基线） | 非本次引入；`MainLibrary.tsx` 已 stash 对照确认为既有问题。建议另行清理并把基线写回 `TEST_CASES.md` |

---

## 7. 未执行 / 边界

| 项 | 原因 |
|---|---|
| AI 推理端到端（深度/前景/降噪/天空）在 **release 包** 上 | 时间盒内未触发（编辑器右侧面板需长滚动到 AI 区，且首个模型为 95 MB+ 下载）。今日 `f9968706` 已在 **debug 包**上完成 `generate_ai_depth_mask` 全链路（`docs/AI_MODELS_V1.6.7.md` §6）；本次已在产物层证明 ORT 为源码构建同一文件（SHA256 一致）、桥导出完整 |
| TC-PERF 性能/内存基线 | 需固定样张与 PSS 采样脚本，本轮未做 |
| TC-REG-07/08 桌面 / Android 回归 | 本轮仅鸿蒙双机 |
| TC-SIGN / `.app` 上架包 | 本轮只出 HAP，未跑 `.csr/pack-app.ps1` |
| 关于页「开源许可」链接 | 需滚动后点击（文档 §6.19 已知坐标陷阱）；同一 `openExternalUrl` 机制已由首页链接、隐私政策链接与「源代码」链接覆盖 |

---

## 8. 判定

- **今日改动面**：启动修复、沙箱路径、外链、协议/授权撤销、ORT 源码构建 —— 在 X90 与 XE90 上**全部通过**。
- **首页 GitHub 链接**：两机实测均拉起系统浏览器并落在 `github.com/Saberlion/LightRaw`。
- **发布包**：release 产物无调试信息、单 ABI、ORT/CANN 注入完整，可交给 AGC 提交流程。
- **待办**：D-03 修正解密脚本；D-04 前端基线收敛；AI 模型在 release 包上补一轮推理基准（§7）。

---

## 9. 增补（2026-10-30 23:02 重建复测）

追加改动：设置页「关于 → 源代码」链接 `https://github.com/Saberlion/RapidRAW-hmos` → `https://github.com/Saberlion/LightRaw`（`src/components/panel/SettingsPanel.tsx:2666`），与首页「Contribute on GitHub」保持一致。

- 复跑 `-Release` 全量出包：hvigor `BUILD SUCCESSFUL in 9 min 13 s`，产物 `entry-default-signed.hap` 52.81 MB；42 个 `.so` 零 `.debug_*`/`.symtab`、无 `sourceMaps.map`（与 §4 一致）。
- 重装诊断包（INTERNET-only + `debugKey`）到 X90 / XE90，均安装成功（`libs/arm64` 落地时间 23:03）。
- 真机实测（两机）：关于页滚动后点「GitHub」→ 系统浏览器地址栏 `https://github.com/Saberlion/LightRaw`；`app.log` 外链错误计数 0；关于页 `Consent status = Not enabled`、两个授权按钮仍 `enabled=false`（6.20 行为未回归）。
