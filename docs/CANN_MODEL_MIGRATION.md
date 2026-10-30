# 新版本模型迁移到 CANN(.om)可行性分析

> 分析日期:2026-10-30 · 适用分支:`v1.6.7`(已合并 harmonyos-port)
> 目标:判断 v1.6.7 的 AI 模型哪些适合从 ONNX Runtime(CPU)迁移到 HiAI Foundation CANN(`.om`),需要多少改造,以及推荐的实施顺序。
> 前置阅读:`docs/AI_MODELS_V1.6.7.md`(模型清单/SHA256/OHOS 现状)、`docs/MINDSPORE_LITE_NPU_EVAL.md` §8.4(`omg` 配方与旧模型实测)、`docs/HARMONYOS_PORTING.md` §6.16(现有集成)。

---

> **2026-10-30 实测更新**:已完成全部新模型候选的 `omg` 转换与真机加载/运行 PoC。前文 §3/§5 是基于 ONNX 结构的**预判**;以 **§6/§7 的实测结论为准**。

## 1. 现有 CANN 接入的硬约束

代码:`src-tauri/src/ohos_integration.rs` 的 `OmModel` + `src-tauri/src/ai_processing.rs` 的 `DenoiseSession::Cann`。

| 约束 | 代码事实 | 对迁移的影响 |
|---|---|---|
| 输入/输出仅 f32 | `write_input_f32` / `read_output_f32` | **int64/int32 输入无法直接喂**,文本/ESAM3 解码器会被挡住 |
| 形状必须静态 | 读取 dims 时只接受 `>=0` | 动态维度(符号 dim)必须通过转换固定,否则无法建 buffer |
| 必须 ION/DMA 后备 | 运行时校验 `fd >= 0` | 大激活(1024²/1008²)要评估 ION 配额 |
| 最多 16 个 tensor | `MAX_TENSORS = 16` | ESAM3 编码器 6 个输出没问题;更多输出需裁剪 |
| 多输入需手工管理 | 只有按 index 的读写 | ESAM3 解码器 9 输入、文本 1 输入都需要逐槽写入 |
| 设备/芯片专用 | `.om` 由 `omg --platform=kirinx90` 生成 | 不能进通用 HAP;只能侧载/按芯片分发,且要哈希固定 |
| perfmode | CNN=4,transformer/FFC=2 | 逐模型调优,不能全局一个值 |

结论:**现有 `OmModel` 只适合“f32 静态 IO 的 CNN”**。这不是改几行就能覆盖所有新模型的,需要按模型判断。

---

## 2. 新版本模型的 ONNX 结构实测

数据来自 2026-10-30 对官方模型的实际解析(opset 17)。

| 模型 | 输入 → 输出 | 规模/关键算子 | 结构判断 |
|---|---|---|---|
| `birefnet_general_lite` | `[1,3,1024,1024] f32` → `[1,1,1024,1024] f32` | 16400 节点;Conv 102、MatMul 150、LayerNorm 64、Softmax 24、Erf 24、Shape 412、Slice 502、Where/Equal 304、Expand 360 | **Swin Transformer 混合体**,静态 IO 但图内动态 shape 极多 |
| `efficientsam3_ev_m_image_encoder` | `[1,3,1008,1008] f32` → `fpn_0/1/2` + `pos_0/1/2`(符号维度) | 1100 节点;Conv 81、ConvTranspose 3、HardSwish 30、MatMul 14、Erf 2 | 以 CNN 为主 + 轻量 attention;输出动态 |
| `efficientsam3_ev_m_decoder` | fpn/pos + `language_features` + `exemplar_labels **[8,1] int64**` 等 9 输入 | 4664 节点;MatMul 278、LayerNorm 68、Softmax 37、Einsum、RoiAlign、InstanceNorm | **Transformer 解码器**,且含 int64 输入 |
| `efficientsam3_ev_m_text_encoder` | `tokens [1,16] **int64**` → 3 个 f32 输出 | 239 节点;MatMul 25、LayerNorm 9、Conv 8 | 小 transformer,但 int64 输入 |
| `mediapipe_face_landmarks` | `[N,256,256,3] f32`(N 动态)→ 3 输出 | 223 节点;Conv 106、PRelu 69、MaxPool 6 | **纯 CNN**,仅 batch 动态 |
| `mediapipe_selfie_multiclass` | `[1,256,256,3] f32` → `[1,256,256,6] f32` | 234 节点;Conv 85、ConvTranspose 1、Resize 6、Softmax 6 | **纯 CNN,全静态** |
| `nind_denoise_utnet_684` | `[1,3,504,504] f32` tile | 已转换 | CNN,现有 `.om` 已验通 |
| SAM / Depth / CLIP / LaMa / MoGe | — | ViT / FFT / 动态点维 | 旧评估已判定 NPU 不适用或收益为负 |

---

## 3. 逐模型 CANN 迁移判定

评级:**A = 低风险可做**,B = 需 PoC, C = 不建议, D = 明确不可行。

### 3.1 NIND 降噪 —— A(已完成)
- 模型未变,`.om` 配方(`nind_v2.om`)与 `DenoiseSession::Cann` 已在新版保留;
- 无需迁移工作,只需在真机复测新构建。

### 3.2 MediaPipe selfie_multiclass —— A
- 全静态 f32 IO、纯 CNN、15.7 MB,最符合现有 `OmModel` 契约;
- 只需 `omg` 转换 + 新增一个 `MaskSession::Cann`(或复用 denoise 的 Cann 变体)+ 255² 预处理;
- 预期收益小(模型本身很小,CPU 已经不慢),但**迁移成本最低**,适合作为新模型 CANN 链路的验证样板。

### 3.3 MediaPipe face_landmarks —— A/B
- 纯 CNN,但输入 batch 维是动态的;转换时用 `--input_shape` 固定为 1 即可;
- 6 个 int64 initializer 通常是 shape 常量,若 `omg` 报错可用 onnx-simplifier/常量折叠处理;
- 与人脸链路共用 ESAM3,单独加速收益有限,优先级低于 selfie。

### 3.4 BiRefNet 前景 —— B(建议做 PoC,不承诺成功)
- 优势:IO 是 **静态 f32 `[1,3,1024,1024]`**,满足 CANN 的基本契约;
- 风险:图内 150 MatMul / 64 LayerNorm / 24 Softmax / 24 Erf + 大量 Shape/Slice/Where/Expand,属于 Swin Transformer;`omg` 很可能转换失败,或把大量算子回退到 CPU,收益被抵消;
- 内存:1024² 输入即 12 MB,Transformer 中间激活可达数百 MB,ION 配额需要实测;
- 建议:先做一个**离线转换 + 单模型基准**的 PoC(不直接改应用架构)。若转换成功且 NPU 时延显著优于当前 FP32 CPU(旧 u2net NPU ~50 ms,BiRefNet 更重),再接入 `MaskSession`;否则保持 CPU。
- **不要**为了省内存强行在 CPU 上用 FP16(ORT CPU 对 fp16 支持有限)。

### 3.5 ESAM3 image encoder —— B/C
- 输入静态 `1008² f32`,以 CNN 为主,单独转换成功率中等;
- 但输出 6 个动态维 tensor,且下游 decoder 仍在 CPU,需要把 ~几十 MB 的 fpn/pos 特征回读到 CPU;
- 混合执行的传输/同步成本可能抵消 NPU 收益;除非 decoder 也上 NPU(见下),否则不建议。

### 3.6 ESAM3 decoder —— D(现有接口不可行)
- 输入 `exemplar_labels [8,1] int64`,现有 `OmModel` 只能喂 f32;
- 图内 278 MatMul / 68 LayerNorm / 37 Softmax / Einsum / RoiAlign,是典型 transformer 解码器,CANN 分区收益差;
- 若坚持:需要把 int64 改为 int32/f32 的**重导出模型** + 扩展 `OmModel` 的 dtype 支持 + 重新验证精度。投入大、收益不确定。

### 3.7 ESAM3 text encoder —— D
- 输入 `tokens [1,16] int64`,同样被 f32-only 挡住;
- 模型仅 239 节点,CPU 已足够快,文本遮罩不是实时交互路径;
- 不建议为它扩展 int64 通道。

### 3.8 SAM / DepthAnything / CLIP / LaMa / MoGe-2 —— C/D
- SAM:旧实测 NPU 更慢(约 6.2×),维持 CPU;
- DepthAnything:转换期即失败,维持 CPU;
- LaMa:FFT/FFC,NPU 可跑但更慢,维持 CPU;
- CLIP:NPU 转换受阻(Cast kernel),且 578 MB,维持 CPU;
- MoGe-2:ViT 系新模型,与 Depth/CLIP 同类风险,暂无迁移价值。

---

## 4. 如果迁移,需要哪些工程改造

### 4.1 运行时
1. **dtype 扩展**:为 `OmModel` 增加 `write_input_i64/i32`、`read_output_i64/i32`(需确认 HIAI ND tensor 是否支持;若只支持 f32,则必须重导出模型把 int64 降为 int32/f32)。
2. **静态 shape 校验**:保留现有 `dims > 0` 检查,转换阶段用 `--input_shape` 固化所有动态维。
3. **多输入封装**:ESAM3 decoder 有 9 个输入,现有按 index 写可行但易错;建议增加 `write_inputs(&[(idx, &[f32])])` + 每模型 IO 契约结构。
4. **会话抽象**:恢复/新增 `MaskSession { Ort, Cann }`(或通用 `NpuSession`),把 `AiModels` 的 `birefnet`/`esam3_*` 改成枚举;`run_*` 内按变体分发。
5. **回退策略**:`.om` 存在且 IO 契约匹配才启用,否则 ORT;保持“任何失败不影响功能”的现有原则。
6. **设置项**:当前只有降噪 `aiDenoiseBackend`;若蒙版也上 NPU,建议增加统一 `aiMaskBackend`(auto/cpu/npu)避免隐式行为。

### 4.2 模型转换与分发
- `omg` 的基本形态(旧配方):`omg --framework=5 --model=<onnx> --output=<name> --platform=kirinx90 --input_shape=<...>`;CNN 用 perfmode 4,transformer 用 2;
- 转换产物是**芯片/固件相关**的,不能放进通用 HAP;继续沿用 `src-tauri/libs/ohos/<abi>/` 侧载 + 哈希固定 + `.om` 存在才启用的模式;
- 每个 `.om` 记录:源模型 SHA256、转换命令、`omg` 版本、perfmode、IO 契约(名称/维度/字节数)、精度对比结果。

### 4.3 验证
- 精度:与 ORT CPU 输出逐像素对比(蒙版建议 IoU ≥ 0.99,概率图 max_abs ≤ 1e-2;参考旧 `.ms` NNRT max_abs 1.6e-3);
- 性能:单模型冷/热启动、稳态时延、PSS/ION 峰值;
- 稳定性:连续触发 10 次、切换模型、进程重启;
- 失败回退:删除 `.om` 后必须自动回到 ORT,且无 panic。

---

## 5. 推荐路线与工作量

| 优先级 | 模型 | 目标 | 预估工作量 | 成功概率 | 收益 |
|---|---|---|---|---|---|
| P0 | NIND 降噪 | 复测现有 `.om` | 0.5 天 | 高(已验证) | 保持 9× CPU |
| P1 | MediaPipe selfie_multiclass | 打通新模型 `.om` 链路 | 1–2 天 | 高 | 小(模型小) |
| P1 | BiRefNet 前景 | 离线 PoC + 基准 | 2–3 天 | **低–中** | 若成功,可能显著优于 FP32 CPU |
| P2 | MediaPipe face_landmarks | 复用 P1 链路 | 1 天 | 高 | 小 |
| P3 | ESAM3 image encoder | 仅编码器 NPU + 特征回读 | 3–5 天 | 低–中 | 不确定(受 decoder CPU 拖累) |
| — | ESAM3 decoder/text、CLIP、SAM、Depth、LaMa、MoGe | 不建议 | — | — | 负收益/不可行 |

**总体判断**:新版本里真正值得尝试 CANN 的只有 **BiRefNet(前景)** 和两个 **MediaPipe CNN**。ESAM3 的核心价值在 decoder/text 的 transformer 部分,而现有 `OmModel` 和 CANN 算子覆盖都不适合,强行迁移的工程成本远高于收益;NIND 降噪继续沿用已验证的 `.om` 即可。

> 注:本分析未执行 `omg` 转换(本机/设备均无 `omg` 工具链),结论基于 ONNX 算子结构、现有 `OmModel` 契约和旧版 §8.4 的实测经验;P1 PoC 需要在具备 CANN Kit/`omg` 的机器上完成后再定稿。

---

> X90 芯片/固件层面的支持范围、性能基线与跨芯片限制,见 **`docs/KIRIN_X90_SUPPORT.md`**。
> Kirin XE90(MateBook Pro S)是另一款芯片,当前工具链无 `kirinxe90` 平台配置,详见 **`docs/KIRIN_XE90_SUPPORT.md`**。

## 6. 全量 POC 实测结果(2026-10-30)

工具链:CANN Kit DDK 6.1.1.0(WSL)+ `omg --platform=kirinx90`;设备:HUAWEI MateBook Pro(HAD-W24,3XB0124617000013);运行器:既有 `hiai_mr_test7`(HIAI_MR 直连,ION buffer,perfmode 4)。

### 6.1 汇总

| 模型 | omg 转换 | 设备 restore/run | 实测(合成输入) | 结论 |
|---|---|---|---|---|
| NIND 降噪(既有) | 已完成 | PASS | ~130 ms/tile(历史) | 保持 |
| MediaPipe face_landmarks | **成功**(`--input_shape` 固定 batch=1) | **PASS** | restore 500 ms;run 3.9–4.3 ms | 可用 |
| MediaPipe selfie_multiclass | **成功**(Resize 模式手术) | **PASS** | restore 146 ms;run 58–77 ms | 可用,需验证精度 |
| EfficientSAM3 image encoder | **成功**(onnxsim 1100→262 节点) | **PASS** | restore 658 ms;run 193–227 ms | 可用,但输出被裁剪 |
| EfficientSAM3 text encoder | **成功**(onnxsim + int64→int32) | **FAIL** | restore NULL | NPUCL 设备端不支持 |
| EfficientSAM3 decoder | **失败** | — | — | 需多次图手术,未完成 |
| BiRefNet foreground | **失败** | — | — | GatherND/Sum 不支持 |

### 6.2 逐模型细节

**MediaPipe face_landmarks — PASS**
- 转换:`onnxsim` 后 `omg` 直接成功(2.6 MB);输入 `input_12 [1,256,256,3] f32`,3 个输出。
- 设备:restore 499.7 ms,perfmode 4,run 4.3 / 4.0 / 3.9 ms,输出维度与 ONNX 一致(`1434` 关键点 + 2 个标量)。
- 结论:可直接作为新模型 CANN 链路的第一个落地项。

**MediaPipe selfie_multiclass — PASS(需注意语义修改)**
- 原始模型被 `omg` 拒绝:`Resize` 的 `coordinate_transformation_mode=tf_half_pixel_for_nn` / `nearest_mode=floor` 不支持。
- 手术:改为 `half_pixel` + `round_prefer_floor` 后转换成功(8.4 MB)。
- 设备:restore 145.6 ms,run 77.3 / 58.4 / 58.3 ms,输出 `[1,256,256,6]`。
- 风险:Resize 模式改变会带来亚像素级差异,接入前必须与 ORT 输出对比(尤其边缘/小目标)。58 ms 对 256² CNN 偏慢,存在 CPU 分区。

**EfficientSAM3 image encoder — PASS,但输出不完整**
- 直接 `omg` 在 `position_encoding_2/Concat` 动态 shape 推断失败;`onnxsim` 把 1100 节点折到 262 后转换成功(42.8 MB)。
- 设备:restore 657.5 ms,run 192.5 / 204.5 / 227.0 ms。
- **关键缺口**:ONNX 有 6 个输出(`fpn_0/1/2` + `pos_0/1/2`),`.om` 只剩 `fpn_0/1/2`。`pos_*` 被 omg 当作常量折叠/裁剪,而 ESAM3 decoder 需要 `pos_2`。
- 结论:编码器本身可上 NPU,但必须解决 `pos_2` 的来源(从 ONNX 预计算后随模型常量下发,或加 dummy 输出强制保留),否则无法替换 ORT 编码器。

**EfficientSAM3 text encoder — 转换成功、设备失败**
- 图手术:onnxsim(239→170 节点)+ `tokens` int64→int32(否则 `GatherV2D` 拒绝 int64 索引),`omg` 产出 135 MB `.om`。
- 设备 restore 直接返回 NULL。hilog 证据:`NPUCL` 不支持 `Erf`、`Reshape`(dimInfo)、`Permute`(realdimCnt > 4)、`Equal`(bool)、`CastT` 等,`PreGraphSaveOptimize` 失败 → `no runtime support the Model`。
- 结论:**转换成功不代表可运行**;该 transformer 模型在 Kirin X90 NPUCL 上不可用,继续停留在 ORT CPU。

**EfficientSAM3 decoder — 转换未完成**
- 已完成两步手术:Einsum(`bqc,bchw->bqhw`)拆成 Reshape+MatMul+Reshape;bool `Gather` 前后加 int32 Cast。
- 仍被 int64 路径挡住:`geometry_encoder/ReduceSum` 输入 int64、随后 `Concat` 动态类型不一致;把 `exemplar_labels` 改 int32、Cast 改 int32 后又出现新的 shape 链问题。
- 即使转换成功,其 278 MatMul / 68 LayerNorm / 37 Softmax 与 text encoder 同类,设备端 NPUCL 大概率同样拒绝;继续投入的 ROI 很低。

**BiRefNet foreground — 转换失败**
- `omg` 预检查失败:100 个不支持节点,`GatherND` × 80 + `Sum` × 20,集中在 `squeeze_module/*/dec_att/aspp*`(可变形注意力)。
- 这些是 decoder 的核心算子,拆解/替换工作量大且精度风险高;当前版本不具备迁移条件。

### 6.3 复现命令

```bash
# WSL: CANN Kit omg(本机二进制解释器已丢失,用系统 loader 启动)
bash /mnt/c/Users/fxre/AppData/Local/Temp/opencode/run_omg.sh \
  --framework=5 --model=<model.onnx> --output=<out> \
  --platform=kirinx90 --check_report=<out>.report.json

# 设备:加载 + 合成输入运行
hdc shell "/data/local/tmp/hiai_mr_test7 /data/local/tmp/msmodels/v167/<out>.om --iters 3 --fill 0 --perfmode 4"
```

## 7. 修正后的结论

| 模型 | 结论 | 建议 |
|---|---|---|
| NIND 降噪 | 保持 | 无需改动 |
| MediaPipe face_landmarks | **可落地** | 优先接入,1–2 天 |
| MediaPipe selfie_multiclass | **可落地(需精度验证)** | 修正 Resize 语义后接入 |
| ESAM3 image encoder | **可运行,但不完整** | 解决 `pos_2` 输出后再评估;单独编码器收益有限 |
| ESAM3 decoder/text encoder | **不可行(当前 NPUCL)** | 保持 ORT CPU |
| BiRefNet foreground | **不可行(算子缺失)** | 保持 ORT CPU;除非换模型或大规模图手术 |
| SAM/Depth/CLIP/LaMa/MoGe | 保持 CPU | 旧结论不变 |

**总体**:新模型里真正能落地的只有两个 MediaPipe CNN;ESAM3 编码器可转但输出/混合执行问题未解;BiRefNet 与 ESAM3 transformer 部分在 Kirin X90 的 omg/NPUCL 上不可行。此前“全量迁移”的预期应下调为“两个小 CNN 的局部加速”。
