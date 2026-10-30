# AI 模型后端归属判定:CANN(NPU)vs CPU

> 判定日期:2026-10-25 · 分支:`feature/harmonyos-port` · 服务对象:OHOS 侧 AI 模型的运行位置决策(哪些上 NPU、哪些留 CPU)
> 证据来源:①代码(`src-tauri/src/ai_processing.rs`、`src-tauri/src/ohos_integration.rs`、`src-tauri/src/tagging.rs`);②真机实测(`MINDSPORE_LITE_NPU_EVAL.md` §8.2/§8.3/§8.4、`AI_DENOISE_HW_ACCELERATION.md` §5.5/§5.6,设备 Kirin X90 / API 26)
> 姊妹文档:`docs/MINDSPORE_LITE_NPU_EVAL.md`(NPU 三路线调研 + 全模型真机战役)、`docs/AI_DENOISE_HW_ACCELERATION.md`(降噪硬件加速)、`docs/HARMONYOS_PORTING.md`(§6.16 CANN 应用集成 + 命名空间闭包定版)

---

> **2026-10-30 更新(v1.6.7 合并后)**:上游已用 **BiRefNet** 替换 u2net、用 **EfficientSAM3** 替换 skyseg,并新增 MediaPipe 人脸与 MoGe-2 法线模型。
> 本文件 §1 / §4.2 / §4.3 关于 u2net / skyseg / `MaskSession` 的内容仅保留为**历史记录**,当前模型清单、SHA256 与 OHOS 适配判定以 **`docs/AI_MODELS_V1.6.7.md`** 为准。

## 1. 结论速览

| 模型 / 功能 | 模型文件 | 当前代码运行位置 | CANN(`.om`)可行性 | 实测最佳路线 |
|---|---|---|---|---|
| NIND 降噪 | `nind_denoise_utnet_684` | `.om`(设 npu 时)→`.ms`→ORT CPU | ✅ 已接入 | **CANN `.om` + perfmode 4**:126–131ms/tile,9.3× CPU |
| u2net 前景遮罩 | `u2net.onnx` | `.om`(存在即用)→ORT CPU | ✅ 已接入 | **CANN `.om` + perfmode 4**:49–51ms,11.6× CPU |
| skyseg 天空遮罩 | `skyseg_u2net.onnx` | `.om`(存在即用)→ORT CPU | ✅ 已接入 | **CANN `.om` + perfmode 4**:~50ms,~8.5× CPU |
| CLIP 自动打标 | `clip_model.onnx` | ORT CPU | ⚠️ `.om` 受阻 / `.ms` 可用 | **留 CPU**(或 `.ms` 150ms,3.0×;收益有限、集成代价高) |
| SAM 编码器 + 解码器 | `sam_vit_b_01ec64_*` | ORT CPU | ❌ 已证伪(NPU 慢 6.2×) | **ORT CPU**(HF 量化原件直用,零手术) |
| DepthAnything V2 | `depth_anything_v2_vits.onnx` | ORT CPU | ❌ 转换期即死 | **ORT CPU**(解除条件 = 上游规范重导出) |
| LaMa 局部修补 | `lama_fp16.onnx` | ORT CPU | ❌ 可跑但更慢 | **ORT CPU**:7.3–7.8s vs NPU 10.2s |

**一句话结论**:值得上 CANN 的只有 3 个纯 CNN 小模型,且均已接入并真机验通;其余 4 个(SAM / DepthAnything / LaMa / CLIP)在 NPU 上要么更慢、要么转换不可行,**不应再投入**。

---

## 2. 后端全景:只有"两条本地 + 一条远端"

| 后端 | 实现 | 平台 | 代码位置 |
|---|---|---|---|
| **CPU** | ONNX Runtime(`ort` crate,**零 Execution Provider 注册**) | 全平台(桌面/Android/OHOS) | `ai_processing.rs` 5 个会话创建点(641/706-708/886/955/1018) |
| **CANN(NPU)** | 华为 HiAI Foundation `HIAI_MR_*` C API;`dlopen` `libai_fmk_tensor.so` / `libai_fmk_model_runtime_core.so` / `libai_fmk_hcl_model_runtime.so`;加载离线 `.om` | **仅 OHOS**(`cfg(target_env = "ohos")`) | `ohos_integration.rs:826-1400`(`om` 模块 / `OmModel`) |
| **MindSpore Lite NNRT(NPU)** | 系统 Kit `libmindspore_lite_ndk.z.so`,`OH_AI_*` C API + NNRT 加速器;加载 `.ms` | **仅 OHOS** | `ohos_integration.rs:583-825`(`ms_lite` 模块 / `MsModel`) |
| 远端(非本机) | AI Connector / 云端 | 全平台 | `ai_connector.rs`、`inpainting.rs:459,583` |

关键事实:**桌面/Android 侧没有任何 EP 注册**——`docs/AI_DENOISE_HW_ACCELERATION.md` §4.4 实测确认随包 dll 为 CPU-only(`is_available(DirectML/CUDA) = false`)。因此**非 OHOS 平台 = 全部 ORT CPU**。

> 注:应用的图像调整管线(曝光/色彩/模糊/LUT/光晕等)跑在 wgpu(Vulkan/GL,GPU compute + readback),那是图形 GPU 而非 NPU;`focus_stacking.rs`、`lens_blur.rs` 的引导滤波为 CPU+rayon 纯算法。严格意义的"AI 模型推理"只有上表内容。

---

## 3. 判定依据:CANN `.om` 的执行契约(代码硬约束)

`OmModel`(`ohos_integration.rs:1055-1387`)决定了"什么模型能接 CANN":

| 约束 | 代码证据 | 影响 |
|---|---|---|
| **IO 只能 f32** | 只有 `write_input_f32`(`:1323`)/`read_output_f32`(`:1371`),且要求 `byte_size/4` 精确等于元素数 | int64/int32/uint8 输入无法喂 |
| **形状必须静态** | dims 读取 `take_while(\|d\| d >= 0)`(`:1237-1240`) | 动态维(-1)拿不到可用 buffer |
| **必须 ION/DMA 后备** | fd < 0 直接拒跑(`:1253-1263`) | 宿主内存 buffer 会在 Run 内崩 |
| 多输入输出**理论支持** | `MAX_TENSORS = 16`(`:870`)、`create_io_slots`(`:1155`) | 但无多输入辅助 API,槽位管理需自写 |
| perfmode 可配 | `OmModel::new(path, perf_mode)`(`:1077`);`OM_PERF_MODE_CNN = 4`(`ai_processing.rs:79`);传负值保持默认 | CNN 用 4,transformer/FFC 用默认 2 |

接入模板已有两套:`DenoiseSession`(Ort/Cann/Nnrt,`ai_processing.rs:120-126`)与 `MaskSession`(Ort/Cann,`:132-136`),各配 IO 契约校验(`denoise_om_contract_matches` / `u2net_om_contract_matches`,`:560`/`:576`)。**新增模型 = 再加一套 enum + 契约校验 + 预处理**。

---

## 4. 逐模型判定

### 4.1 NIND 降噪 —— CANN 最优(已接入)

- 路由:`get_or_init_denoise_model` 在 `ai_denoise_backend == "npu"` 时按 **`.om`(`nind_v2.om`)→ `.ms` → ORT CPU** 三级回退(`ai_processing.rs:789-887`)。
- 实测:`.om` 126–131ms/tile(perfmode 4)vs `.ms` NNRT 257ms vs ORT CPU 1215ms → **9.3× CPU**;NCHW I/O,免 `.ms` 的 NHWC 转置(`MINDSPORE_LITE_NPU_EVAL.md` §8.4 line 388/500)。
- 前置:`.om` 为**侧载产物**(不在运行时下载),由 `scripts/convert-nind-ms.ps1` + Pad→Slice 手术 + `omg` 复现,哈希钉死。

### 4.2 u2net / skyseg —— CANN 最优(已接入)

- `build_mask_session`(`ai_processing.rs:599-643`):`.om`(`u2net.om` / `skyseg_u2net.om`)存在且 IO 契约匹配即用,**且跳过 onnx 下载**;否则走 ORT。
- 实测:perfmode 4 下 ~50ms vs `420–580ms` CPU → **8.5–11.6×**;`.om` 反超 `.ms`(65.9ms)(§8.4 line 499/505)。
- 真机端到端:应用进程内触发 → NPU 推理 → 全分辨率蒙版落盘 **4.85s**(`HARMONYOS_PORTING.md` §6.16)。

### 4.3 CLIP 自动打标 —— 唯一"可讨论"的候选,收益有限

- 代码:`ClipModels { model: Mutex<Session>, tokenizer }`(`ai_processing.rs:99-102`),推理输入为 **int64** token ids / attention mask(`tagging.rs:177,182,206`)。
- `.ms` 路线可通:静态 `--inputShape` 后 150ms vs CPU 448ms(**3.0×**),但**文本 batch 维被锁死**——app 现按 tag 数动态,须 padding 到固定值或按形状缓存多份 `.ms`(§8.3 line 332)。
- `.om` 路线死:**NPUCL Cast kernel 未注册**(int32→int64,kernelId 589827),须全图 int64→int32 手术(§8.4 line 389)。
- 判定:150ms 级对交互体验影响极小,**不值得**为它引入新的会话枚举 + 固定 batch 策略。

### 4.4 SAM(ViT-B 编码器 + 解码器)—— NPU 明确输

- 同芯片实测:全链 **CPU(8t)≈9.6s vs NPU 60s,CPU 快 6.2×**;mask IoU 0.982(CPU)> 0.978(NPU),精度不劣反优(§8.4 line 524/534)。
- 根因:量化 transformer + X90 20 核 int8 dotprod NEON 极强;NPU 侧被"子图派发 + `four2five` 布局转换 + fp16 dtype 转换税"主导,且 **omg 拒收量化模型**(`DynamicQuantizeLinear/MatMulInteger`)→ 被迫 fp32 重建(965MB vs 498MB)。
- 代码侧本就不满足 CANN 契约:编码器输入是 **u8** `[1,3,1024,1024]`(`ai_processing.rs:1609`);解码器 6 输入 + **动态点数维** `(1, pc_len, 2)`(`:1687`)与 `(1,1,256,256)` 低分 mask(`:1672`)。
- 判定:**留 ORT CPU,零改动**。

### 4.5 DepthAnything V2 —— 转换期判死

- 官方 ONNX 为**非规范导出**:ATen 方言算子(`aten_add/aten_expand/aten_relu/aten_select`)+ torch 模块路径自定义算子块 → `.ms`/`.om` 都进不去(§8.3 line 308、§8.4 line 392)。
- 即便重导出,ViT + DPT 头在 NPU 上回退比例高、收益有限。
- 判定:**留 CPU**;解除条件 = 上游对 `depth_anything_v2_vits` 出规范(未量化、标准算子)重导出。

### 4.6 LaMa 局部修补 —— 能上 NPU 但更慢

- `.om`(需 onnxsim + dim_param 算术求值 + batch 折叠 + `--weight_data_type=FP32` 三连手术)10.2s,而 ORT CPU 7.3–7.8s → **NPU 慢 1.3×,perfmode 4 无益**(§8.4 line 387/502)。
- `.ms` 在 Kit 2.7.0 runtime 全灭(8 变体实测)。
- 判定:**留 ORT CPU**;§8.4 line 433 战略判定"不值得再投入"。

---

## 5. 现状与目标架构

```
OHOS 侧 AI 推理
├─ CANN (.om + HIAI_MR, perfmode 4)   ← u2net / skyseg / nind     [已接入·已验通]
├─ MindSpore NNRT (.ms)                 ← (兜底) nind;潜在 CLIP    [备选]
└─ ORT CPU (intra_threads 未配置)       ← SAM / Depth / LaMa / CLIP [默认]
      桌面/Android:全部 ORT CPU(无 EP)
```

**perfmode 统一策略**:CNN/小算子密集图用 **4**;transformer/FFC 大算子图用**默认 2**(perf4 收益与"派发开销占比"正相关,§8.4 line 504)。

---

## 6. 若要扩展 CANN 覆盖:缺口清单

| 缺口 | 位置 | 影响 |
|---|---|---|
| `OmModel` 只有 f32 读写 | `ohos_integration.rs:1323,1371` | CLIP / SAM 编码器需加 dtype 感知或 `write_input_bytes` |
| 无动态 shape 支持 | `:1237-1240` | SAM 解码器需固定点数重导出 + 多份 `.om` |
| 无通用 NPU 会话枚举 | `ai_processing.rs:120-136` | 每个新模型要新增 enum + 契约校验函数 |
| `.om` 仅侧载、**无 sha256 门禁** | `ai_processing.rs:609-628`(只做 IO 契约检查) | 新增/现有 `.om` 均无哈希校验;onnx 下载路径有 `download_and_verify_model` |
| 无 `.om` 分发路径 | 同上 | 新设备/新机器须手工 `hdc file send` |
| ORT 线程未配置 | 所有 `Session::builder()`(全仓无 `with_intra_threads`) | **免费的收益**:实测 8t 比全核快 11–16%(§8.4 line 525) |

---

## 7. 建议与重评触发条件

**建议**:

1. **不动** SAM / DepthAnything / LaMa / CLIP 的后端——已有真机证据表明 CPU 更优或转换不可行,改动为负收益。
2. (可选,低风险)给现有 ORT 会话加 `with_intra_threads(8)`,依据 §8.4 line 525 实测;桌面侧同参数需在真机复测后再定。
3. **收尾现有 3 条 `.om` 路线**:补 `.om` 的 sha256 门禁与分发路径;把 `scripts/convert-nind-ms.ps1` 模式扩展到 u2net/skyseg 的 `.om`;固化 perfmode 分档(CNN=4 / transformer=2)。

**未来重评的触发条件**(按优先级):

- 固件注册 `ROIAlignV2` / 补齐 NPUCL Cast kernel → 届时 SAM3 与 CLIP 的 `.om` 件可直接复测(现有 `.om` 工程件与执行配方保留为基线)。
- 上游对 depth / SAM 出**规范未量化导出** → 重跑 `.ms`/`.om` 转换。
- MindSpore Lite Kit 引擎升级(现设备引擎 = 2.7.0,converter 版本受其约束)。

---

## 8. 关键代码坐标

| 主题 | 位置 |
|---|---|
| 模型 URL / SHA256 常量(7 模型) | `ai_processing.rs:21-89` |
| `AiModels` / `ClipModels` / `AiState` | `ai_processing.rs:91-145` |
| `DenoiseSession` / `MaskSession` | `ai_processing.rs:120-136` |
| `.om` IO 契约校验 | `ai_processing.rs:560`(denoise)/`:576`(u2net 族) |
| `.om` 优先 + 回退(mask) | `ai_processing.rs:599-643` |
| NPU 三级回退(denoise) | `ai_processing.rs:789-887` |
| CANN tile 循环 | `ai_processing.rs:1408`(`run_native_denoise_om`) |
| `.ms` tile 循环 | `ai_processing.rs:1362`(`run_native_denoise_ms`) |
| `OmModel`(HIAI_MR 直连) | `ohos_integration.rs:1055-1400` |
| `MsModel`(MS Lite Kit) | `ohos_integration.rs:646-825` |
| CLIP 推理(int64 输入) | `tagging.rs:177,182,206` |
| LaMa 推理 | `ai_processing.rs:1456`(`run_lama_inpainting`) |
| 后端设置项 | `app_settings.rs:540`(`ai_denoise_backend`,默认 `"cpu"`);UI `SettingsPanel.tsx:591-592` |

---

*本文为代码现状 + 既有真机战役数据的**综合判定**,不含新实验;所有性能数字引自 `docs/MINDSPORE_LITE_NPU_EVAL.md` §8.2–§8.4 与 `docs/AI_DENOISE_HW_ACCELERATION.md` §5.5–§5.6 的真机实测表。*

---

## 9. 真机验证记录(2026-10-25)

> §1–§8 回答"哪个模型该在哪个后端";本节回答"这一版包在真机上实际跑通了哪些路径"。结论前置:**除 CANN/NPU 与导出、AI 推理外,图库 → RAW 解码 → GPU 缩略图 → 编辑器预览全链路已在真机跑通。**

### 9.1 构建与签名

- 构建:`scripts/build-ohos.ps1 -TimeoutSec 5400`(默认 debug,`-t aarch64`)→ `gen/ohos/entry/build/default/outputs/default/entry-default-signed.hap`(注入后 122.6 MB);脚本已完成 ORT + 36 个 CANN 闭包库注入、`libs/x86_64/` 残留清除、`ets/sourceMaps.map` 删除。
- hvigor 用 release signingConfig 出的包在真机直装**被拒**(`9568322 not trusted app source`),故本机走 `.csr/SIGNING.md` §7 调试签名(`keyAlias debugKey`)→ `entry-default-debug-signed.hap`(66.7 MB,sign-app 会重压缩条目)→ `verify-app success`(`Digest verify result: true`)→ `hdc install -r` 成功。
- 设备 `3XB0124617000013`(`deviceType: 2in1`,udid `D68C962C…E5A307`)命中调试 profile 的 `debug-info.device-ids[0]`。

### 9.2 调试 profile 的 ACL 限制(装机必读)

调试 profile 内 `"acls":{"allowed-acls":[]}`(空数组,直接从 p7b 读出),因此 `module.json5` 的 7 条权限中有 6 条(ACL 受限)会触发 `9568289 grant request permissions failed`。本次做法:装包前把 **HAP 内 `module.json`** 的 `requestPermissions` 临时裁到仅 `ohos.permission.INTERNET`(不动 `src-tauri/gen/` 源文件;`module.json5.agc-backup` 仍为 8 权限原件)。

裁权限后的实测边界:

- 启动时不再预扫描用户目录:`Could not scan directory '/storage/Users/currentUser/Download': Operation not permitted (os error 1)`(与 §6.14 同因)。
- **系统选择器给的是临时 grant,不依赖 ACL**:经选择器选中的目录依旧可用——本次全链路即以此跑通。
- `fileShare.persistPermission`(跨重启持久授权)、相册保存(`WRITE_IMAGEVIDEO`)必然失败。
- 要"全权限"本地包:在 AGC 控制台申请这些受限权限后,用 DevEco 自动签名重新下发含 `allowed-acls` 的调试 Profile。

### 9.3 真机实跑结果(证据取自 `files/logs/app.log` 或无障碍树 dump)

| 路径 | 结果 | 证据 |
|---|---|---|
| 启动 + GPU | ✅ | `Using GPU adapter: Maleoon 916 (backend: Vulkan, type: IntegratedGpu)` |
| GPU 计算管线 | ✅ | 冷缓存 `[pipeline-warmup] completed in 10.73s`;热缓存重启 `0.18–0.19s` |
| 前端渲染 | ✅ | 无障碍树:`Web` 组件占满内容区,且含真实文本节点("…RAW image editor. Open a folder to begin."、"Open Folder"、"Version 1.6.4")——非黑屏 |
| 选择器桥(Rust↔ArkTS) | ✅ | 点 "Open Folder" → `com.huawei.hmos.filemanager` 选择器弹出;选定后 `rootFolders` = `/storage/Users/currentUser/Download` |
| 图库扫描 + EXIF | ✅ | 4 项(tiff / .fff / jpg / png),网格含光圈·快门·ISO·焦距摘要 |
| RAW 解码(rawler) | ✅ | `Hasselblad X2D 100C` 100MP `.fff`:RGGB demosaic → calibration → `crop … 11656x8742` → `Raw enhancing … took 2.63s` |
| GPU 缩略图 | ✅ | `[generate_thumbnail_data] … on GPU in 377–749ms` ×4 |
| 编辑器预览 | ✅ | `[apply_adjustments] 959x1280 … on GPU in 376.8ms`;`[process_preview_job] … encode in 25.13ms, total 1.86s` |
| 已存调整回读 | ✅ | 打开即恢复滑杆 0.79 / 45 / 0 / 39 / -46 / 0 / 0.9 |
| **CANN(NPU)** | ⚪ 未跑 | 机上 `files/appdata/models` 只有 `sam_vit_b_01ec64_{encoder,decoder}.onnx` + `depth_anything_v2_vits.onnx`;**nind/u2net/skyseg 三个 `.om` 不在机上**,NPU 路径无从触发;且 `aiProvider`/`aiDenoiseBackend` = `cpu` |
| AI 推理(ORT) | ⚪ 未跑 | 需交互(Masks / AI 打标)或补模型 |
| 导出 | ⚪ 未跑 | 相册导出依赖 `WRITE_IMAGEVIDEO`(本包已裁) |

### 9.4 复跑配方与 NPU 前置

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-ohos.ps1 -TimeoutSec 5400
# 裁权限:HAP 内 module.json 只留 ohos.permission.INTERNET
# 调试签名 + 验签:.csr/SIGNING.md §7 材料与口令(debugKey)
hdc install -r <entry-default-debug-signed.hap>
hdc shell aa start -a EntryAbility -b io.github.CyberTimon.RapidRAW
hdc file recv /data/app/el2/100/base/io.github.CyberTimon.RapidRAW/files/logs/app.log .
```

NPU 复跑前置:把 `nind_v2.om` / `u2net.om` / `skyseg_u2net.om` 推到
`/data/app/el2/100/base/io.github.CyberTimon.RapidRAW/files/appdata/models/`(=`app_data_dir()/models`),
再把 `aiDenoiseBackend` / `aiProvider` 切到 NPU,按 `docs/HARMONYOS_PORTING.md` §6.16 的 NPUCL `SetVote` / hiaiserver `SetNPUPerf` 日志取证。
