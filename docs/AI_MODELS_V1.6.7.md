# v1.6.7 模型清单与 OHOS 适配判定

> 更新时间:2026-10-30 · 适用分支:`v1.6.7`(已合并 `feature/harmonyos-port`)
> 背景:v1.6.7 上游替换了遮罩模型族(旧的 u2net / skyseg 已删除),并新增人脸、法线与 EfficientSAM3。本文给出当前代码实际引用的模型清单、与旧移植分支的差异,以及每个模型在 OHOS 上是否需要额外适配(CPU / CANN `.om` / MindSpore `.ms`)的结论。
> 证据:代码常量与 SHA256(`src-tauri/src/ai_processing.rs`)、模型文件实测(2026-10-30 从 hf-mirror 按 commit `d25f1328` 下载并逐个校验 SHA256,全部与代码一致)、ONNX 元数据(`onnx` 1.23.2 解析)。

---

## 1. 当前模型清单(v1.6.7)

| 功能 | 模型文件 | 大小 | 输入 → 输出(ONNX) | opset | OHOS 运行位置 |
|---|---|---|---|---|---|
| 主体遮罩(SAM) | `sam_vit_b_01ec64_encoder.onnx` | 95.6 MB | `[1,3,1024,1024] f32` → image embeddings | 17 | ORT CPU |
| | `sam_vit_b_01ec64_decoder.onnx` | 8.3 MB | embeddings + prompts → mask | 17 | ORT CPU |
| 前景遮罩 | `birefnet_general_lite.onnx` | 213.6 MB | `[1,3,1024,1024] f32` → `[1,1,1024,1024] f32` | 17 | ORT CPU(**OHOS 实际使用**) |
| | `birefnet_general_lite_fp16.onnx` | 169.3 MB | 同上 | 17 | 仅 GPU EP 可用时(DirectML/CoreML) |
| 天空遮罩 | `efficientsam3_ev_m_image_encoder.onnx` | 81.5 MB | `[1,3,1008,1008] f32` → `fpn_0/1/2` + `pos_*` | 17 | ORT CPU |
| | `efficientsam3_ev_m_decoder.onnx` | 123.8 MB | fpn/pos + 语言/示例提示 → logits/boxes/masks | 17 | ORT CPU |
| | `efficientsam3_sky_prompt.bin` | 16 KB | 预计算语言特征 | — | ORT CPU |
| 文本遮罩 | `efficientsam3_ev_m_text_encoder.onnx` | 161.3 MB | `tokens [1,16] **int64**` → language features | 17 | ORT CPU |
| | `clip_tokenizer.json` | 2.1 MB | tokenizer | — | 前端/Rust tokenizers |
| 人脸遮罩 | `efficientsam3_ev_m_image_encoder.onnx` | 81.5 MB | 同上(与天空共用) | 17 | ORT CPU |
| | `efficientsam3_ev_m_decoder.onnx` | 123.8 MB | 同上 | 17 | ORT CPU |
| | `efficientsam3_face_prompt.bin` | 16 KB | 预计算语言特征 | — | ORT CPU |
| | `mediapipe_face_landmarks.onnx` | 4.7 MB | `[N,256,256,3] f32` → landmarks | 17(+ai.onnx.ml 2) | ORT CPU |
| | `mediapipe_selfie_multiclass.onnx` | 15.7 MB | `[1,256,256,3] f32` → `[1,256,256,6] f32` | 17(+ai.onnx.ml 2) | ORT CPU |
| 深度遮罩 | `depth_anything_v2_vits.onnx` | 94.8 MB | `[1,3,518,518]` → depth | 17 | ORT CPU |
| 自动打标 | `clip_model.onnx` | 577.7 MB | image/text embeddings | 17 | ORT CPU |
| | `clip_tokenizer.json` | 2.1 MB | tokenizer | — | CPU |
| 局部修补 | `lama_fp16.onnx` | 106.4 MB | image + mask → inpaint | 17 | ORT CPU |
| 降噪 | `nind_denoise_utnet_684.onnx` | 118.4 MB | `[1,3,504,504] f32` tile | 17 | `.om` → `.ms` → ORT CPU |
| 法线/Relight | `moge-2-vits-normal-onnx.onnx` | 134.3 MB | image → normal | 17 | ORT CPU |

> 缓存总量:全部模型约 **1.74 GB**;旧移植分支遮罩栈(SAM + u2net + skyseg + depth)约 0.54 GB。4 GB 设备需要单独评估并发会话内存。

---

> 将模型迁移到 CANN(`.om`)的逐模型可行性、改造清单与**全量 POC 实测结果**见 **`docs/CANN_MODEL_MIGRATION.md`**(含 §6.4 双机逐算子验证):MediaPipe face_landmarks / selfie 已转换并在 **X90 + XE90 双机**跑通;ESAM3 image encoder 可转可跑但输出被裁剪(`pos_2` 缺失);ESAM3 text encoder 转换成功但设备 restore NULL(**int64 Cast 链**,单算子实证;Erf/Equal/LayerNorm 等单算子均可过);ESAM3 decoder 与 BiRefNet 在 **omg 转换期**失败(GatherND/多输入 Sum 映射缺失、5D/动态 shape),与设备无关。

## 2. 与旧移植分支的差异

| 能力 | 旧(`feature/harmonyos-port` 基线) | 新(v1.6.7) | 影响 |
|---|---|---|---|
| 前景遮罩 | `u2net.onnx`(167.8 MB),CANN `.om` 已接入 | **BiRefNet**(213.6 MB FP32 / 169.3 MB FP16) | 旧 NPU 路径不存在;OHOS 无 GPU EP → 实际走 **FP32 CPU** |
| 天空遮罩 | `skyseg-u2net.onnx`(167.8 MB),CANN `.om` 已接入 | **EfficientSAM3**(encoder + decoder + sky prompt) | 旧 `.om` 契约与代码已删除;改为 ORT CPU |
| 文本/人脸遮罩 | 无 | ESAM3 text/face + MediaPipe landmarks/selfie | 新增 CPU 负载与新模型下载 |
| 降噪 | NIND `nind_denoise_utnet_684` + `.ms`/`.om` | **同文件、同 SHA256** | **无需适配**,现有 NPU 配方继续有效 |
| SAM / Depth / CLIP / LaMa | 同左 | 同文件、同 SHA256 | 现有结论不变 |
| 法线 | 无 | MoGe-2(134.3 MB) | 新增 CPU 模型 |

---

## 3. 各模型 OHOS 适配判定

### 3.1 NIND 降噪 —— 无需升级
- 模型文件名与 SHA256(`ee358627…`)未变,`.om`(`nind_v2.om`,CE 侧载)与 `.ms`(`nind_denoise_utnet_684.ms`)配方继续有效。
- 代码集成已在新版 `DenoiseSession`(Ort/Nnrt/Cann)+ 通用 `run_tiled_denoise` 中保留,默认 CPU,设置 `ai_denoise_backend=npu` 时走 NPU。

### 3.2 SAM / DepthAnything / CLIP / LaMa —— 无需适配
- 文件与哈希均未变;旧结论(NPU 收益有限或不可行)继续成立,保持 ORT CPU。
- SAM3(上游 2026-10-07 新增的量化原件,当前应用未引用)的移植状态与三路线判定见 **`docs/SAM3_PORTING_STATUS.md`**;同类应用 darktable 的 AI 算法选型对照见 **`docs/DARKTABLE_AI_ANALYSIS.md`**。

### 3.3 BiRefNet 前景 —— 需要重点评估(**当前是 CPU 回退**)
- OHOS 的 `gpu_execution_provider()` 返回 `None`(无 DirectML/CoreML),因此 `gpu_inference()` 为 false,`AiMaskModel::Foreground` 会选择 **FP32 213.6 MB** 模型,而不是 FP16。
- 模型 IO 是静态 `[1,3,1024,1024] f32`(符合 CANN 的 f32/静态要求),但图内是 **Transformer 风格**:16400 个节点,大量 `Shape/Slice/Gather/Reshape/Transpose/MatMul/Where`,并非纯 CNN。
- 结论(2026-10-30 PoC 后更新):
  - **不要强行让 OHOS CPU 使用 FP16 版本**——ORT CPU EP 对 fp16 算子支持有限,通常会插入 Cast 或直接不支持,收益不确定。
  - **CANN `.om` PoC 已做,转换失败**:`omg` 预检查拒收 100 个节点(`GatherND` × 80 + 多输入 `Sum` × 20),集中在 `squeeze_module/*/dec_att/aspp*`(可变形注意力)。这是 **omg 的 ONNX 类型映射缺失**,不是设备缺算子(设备端带 `GatherNd`/`ReduceSum` 实现,归约 `ReduceSum` 单算子可跑);但可变形注意力是 BiRefNet 的核心结构,Sum→Add / GatherND→Gather 的图手术代价高且精度风险大——**保持 ORT CPU,除非换模型**。
  - 备选:保留一个更轻量的前景模型(上游未提供),或在前景/天空/人脸之间复用 ESAM3 encoder 以降低内存。

### 3.4 ESAM3(天空 / 文本 / 人脸)—— 不建议走 CANN,保持 ORT CPU
现有 `OmModel` 的硬约束是 **f32-only + 静态维度**;ESAM3 不满足:
- **decoder** 的 `exemplar_labels` 是 **int64** `[8,1]`;
- **text encoder** 的 `tokens` 是 **int64** `[1,16]`;
- **image encoder** 的 `fpn_*` / `pos_*` 输出使用符号维度(dynamic),CANN 需要固定 shape;
- 图内包含 LayerNormalization / MatMul / Softmax 等 ViT 算子,NPU 分区收益低。

**2026-10-30 PoC 修正**(详见 `docs/CANN_MODEL_MIGRATION.md` §6.2/§6.4):真正的阻塞**不是 ViT 算子本身**——LayerNorm/MatMul/Softmax/Erf/Equal 单算子在 X90/XE90 上全部可跑且数值正确;是:
- **int64 链**:`Cast` 产出 int64 在两台设备 restore NULL(text encoder 实测根因);Gather 的 int64 **输入**索引其实可用,缺的是"产出 int64"这条边;
- **动态/5 维张量**:`Reshape(dimInfo)`、`Permute realdimCnt>4`(5D 图在 omg 转换期即被拒);
- image encoder:onnxsim(1100→262 节点)后可转可跑(X90 193–227 ms / XE90 219 ms),但 `pos_*` 输出被 omg 裁剪,decoder 依赖的 `pos_2` 缺失,单独上 NPU 意义有限。

若未来一定要上 NPU,需要同时做:重新导出 **int32 + 静态 shape(≤4 维)** 的模型、保住 `pos_2` 输出、重新验证精度。投入产出比低,当前建议保持 CPU;优先保证功能可用与内存可控。

### 3.5 MediaPipe 人脸(landmarks / selfie)—— **已验证可落地(双机 PoC 通过)**
- 生产 `.om` 已构建并完成**双机真实输入验证**(3840×2400 实拍照片,与应用侧同预处理,ORT CPU 为参考;详见 `docs/CANN_MODEL_MIGRATION.md` §6.5):
  - face_landmarks:关键点误差 max 0.61 / 0.93 px(X90/XE90),**亚像素级**;presence 误差 ≤1.1e-4。
  - selfie_multiclass:argmax 类别一致率 **99.90% / 99.88%**,logits mean_abs ≈0.002;Resize 手术在该输入上与原模型逐位一致。
  - 时延:face ~4–5 ms(双机);selfie ~58 ms(X90)/ 14–15 ms(XE90)。
  - 产物、源 ONNX 与评估脚本统一在 `npu-models/`(README 记录来源与 SHA256)。
- 落地次序仍放在 ESAM3 之后:它们只占人脸链路的一小部分(主要耗时在 ESAM3 encoder/decoder),单独加速收益有限。

### 3.6 MoGe-2 法线 —— 保持 CPU
- 新增 134.3 MB ViT 系模型;NPU 转换风险与 ESAM3 同类,当前无适配计划。

---

## 4. 内存与下载注意事项

- 模型仍由运行时按需下载到 `app_data_dir()/models`;OHOS 经 `crate::app_paths` 落到 `/data/storage/el2/base/files/appdata/models`,不受 FUSE `rename` 问题影响(`persist_downloaded_asset` 的字节拷贝回退保留)。
- 全部下载 URL 已统一为 `hf-mirror.com`;下载完整性由代码内 SHA256 校验(本次核对全部命中)。
- 旧版本遗留的 `u2net.onnx` / `skyseg-u2net.onnx` / `skyseg_u2net.onnx` 已不再被引用,可在设备上手动删除以回收约 335 MB;代码未自动删除,避免降级时丢模型。
- 并发会话:新版遮罩栈明显更重(SAM 104 MB + BiRefNet 214 MB + ESAM3 205~370 MB + depth 95 MB + CLIP 578 MB + LaMa 106 MB + 法线 134 MB)。4 GB 设备建议限制同时加载的模型数量,并复用 `AiState::release_sessions()` 的释放时机。

## 5. 建议的下一步(按优先级)

1. **P0 真机基准**:分别触发前景(BiRefNet)、天空/文本/人脸(ESAM3)、法线,记录首帧耗时、稳态耗时与 PSS;确认 ORT CPU 下没有 int64/算子不支持错误。
2. **P0 内存验证**:在 4 GB / 8 GB 设备上连续切换 SAM → 前景 → 天空 → 人脸,确认不触发 LowMemoryKill;必要时增加按需卸载/模型互斥。
3. **P1 前景优化**:BiRefNet `.om` PoC 已完成——转换期被拒(GatherND/多输入 Sum 映射缺失),维持 CPU;若 CPU 明显慢于旧 u2net NPU(~50 ms),评估轻量替代模型而非继续图手术。
4. **P2 人脸小模型**:生产 `.om` 已构建并完成双机真实输入精度验证(见 §3.5);下一步是接入 `MaskSession` 的 Cann 后端与 face 链路。
5. **文档维护**:`AI_BACKEND_PLACEMENT.md` 的 u2net/skyseg 章节与 `HARMONYOS_PORTING.md` §6.16 已标记为历史记录;后续 NPU 结论统一更新到本文。

---

## 6. 真机验证记录(2026-10-30,设备 3XB0124617000013)

- `generate_ai_depth_mask`:`depth_anything_v2_vits.onnx` 下载 95 MB,ORT CPU 推理成功并返回 `maskDataBase64`。
- `generate_ai_foreground_mask`:OHOS 无 GPU EP,实际下载并加载 **BiRefNet FP32(214 MB)**;ORT CPU 推理成功,返回约 89 KB 的蒙版 Base64。说明新前景模型在 OHOS 上可用,但走的是 CPU/FP32 路径。
- 设备 `files/appdata/models` 已出现 `efficientsam3_ev_m_{image_encoder,decoder}.onnx`、`efficientsam3_sky_prompt.bin` 与 `nind_denoise_utnet_684.onnx`,说明新模型下载链路正常。
- 待补:天空/文本/人脸与法线的真机耗时、PSS 峰值,以及 BiRefNet 与旧 u2net NPU(~50 ms)的量化对比。
- CANN `.om` 侧的 PoC 结论(转换 + 双机 restore/Run + 逐算子验证)统一记录在 `docs/CANN_MODEL_MIGRATION.md`(§6/§6.4)与 `docs/KIRIN_XE90_SUPPORT.md`;本文 §3 的判定已按其修正。
