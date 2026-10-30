# SAM3 移植状态(HarmonyOS / Kirin)

> 收口日期:2026-10-30 · 本文是 **Meta SAM3 三件套移植到 OHOS 设备**的独立入口;战役原始记录(手术过程、hilog 逐轮排障、误差分解)在 `docs/MINDSPORE_LITE_NPU_EVAL.md` §8.4,`.om` 工程件保留在 WSL `~/cannkit/out/`。
> 一句话:**移植在技术上已完成并真机全通(含精度验证),但 NPU 路线性能为负收益(CPU 快 6.2×);现实可用形态是 ORT CPU;当前应用代码未引用 SAM3,维持"技术储备"定位。**

---

## 1. SAM3 三件套与本仓溯源

上游模型基座仓库 `CyberTimon/RapidRAW-Models` 2026-10-07 新增 HF 量化原件:

| 组件 | 原件 | 已转 `.om` | 体积 |
|---|---|---|---|
| vision encoder | `sam3_vision.onnx` 498MB | `sam3img_v3.om` | 965MB |
| text encoder | `sam3_text.onnx` 355MB | `sam3lang_v6.om` | 808MB |
| decoder | (量化件) | `sam3dec_roi_v2.om`(8 轮手术终版) | 162MB |

**当前 v1.6.7 应用代码不引用 SAM3**(遮罩栈 = SAM ViT-b 交互 + BiRefNet 前景 + ESAM3 天空/文本/人脸);SAM3 是战役期从模型基座仓库取用的评估对象。

## 2. 三路线判定

| 路线 | 状态 | 数据(X90 真机) | 判定 |
|---|---|---|---|
| **ORT CPU(量化原件)** | ✅ HF 量化三件套 ORT 直接加载(含 mmcv RoiAlign,属性集实为标准 `output_half_pixel`) | 全链 **9.6s**(vision 8.34s + decoder 1.33s,8t) | **现实可用形态** |
| **CANN NPU(`.om`)** | ✅ 三件套全部 restore+Run 全通 | 全链 **~82s 冷 / ~139s 热稳态**(encoder 51.9–58.3s 冷、125–128s 热节流;decoder 7.6–9.4s;text 161ms) | ❌ **慢 6.2×,负收益**;且 `.om` 合计 ~1.9GB,4GB 设备不可行 |
| 桌面参照 | — | i7-8700 量化 ORT ~26s | — |

## 3. NPU 路线的手术清单(decoder 8 轮)

1. **uint8 Cast 口袋**:GatherND 前全图唯一 uint8 链(Cast→Gather→3×Cast),Gather data 限 fp32/fp16/int32 → 一行 Cast `to` UINT8→FLOAT 手术。
2. **ROIAlignV2 固件类型级封杀**(官方算子表列支持但设备拒)→ **90 节点原语分解**:K=4 固定采样 + torchvision `aligned=False` 精确语义 + 平铺 1-D int32 Gather 索引。
3. **fp16 GemmD dtype**:`CPUCL Gemm_131(GemmD) input2 dtype should be fp32` → `--weight_data_type=FP32`(98.8→161.8MB;与本仓"omg 默认 FP16 权重"结论一致)。
4. **GatherV2D 负索引**运行期不支持 → 常量索引 `[-1]→[5]`(6 路多路复用末位选择)。
5. text 侧:tokens **int64→int32**;harness 填充必须 `--fill 0.0`(0.5f 字节模式对 int32 是越界 token id)。

预处理契约(特殊):vision 输入 = **1008×1008 uint8 原始像素、零归一化**(量化尺度折叠进 ConvInteger 头链,图内无 mean/std 节点);prompt 槽位 padding = box_coords 0 / labels 0 / box_masks false。

## 4. 精度验证(已通过)

24MP 真照片(6000×4000)、房屋 box 提示:

- 设备 mask IoU vs 解析轮廓 **0.954**(桌面金标准 0.953);设备 vs 金标准 **IoU 0.985**;presence_logit **+9.11**。
- 误差分解:**E1 设备执行罚项 ~1e-4(可忽略)**;E2 编码器特征传播 0.11;**E3 原件 int8 量化 0.32(最大误差源)**——E3≫E2≫E1,四种跑法 IoU_house 全部 0.9515–0.9541。

## 5. 为什么 NPU 输(与逐算子验证相互印证)

- **encoder 占 86–91% 是绝对瓶颈**,与 RoiAlign 无关;即使手术段(90 节点吃 27% 时长,24.5ms/节点 vs 全图均值 2.9ms/节点)归零,decoder transformer 段自身仍慢 4.4×。
- 根因 = 子图派发 + `four2five` 布局转换 + fp16 dtype 转换税——transformer 在这代 NPU 上系统性吃亏(`docs/CANN_MODEL_MIGRATION.md` §6.4 逐算子验证的"算子能过 ≠ 整图快"结论,SAM3 战役是最早实证)。
- SAM3 战役踩过的坑(GatherND 转换期拒、int64 Cast、fp16 权重 dtype、ROIAlign 固件封杀)与 §6.4 的三条边界完全吻合。

## 6. SAM3 text encoder 存活 vs ESAM3 text encoder 阵亡

同为 int64 tokens→int32 手术:**SAM3 text 手术后 restore+Run 全通(161ms)**,ESAM3 text 手术后仍 restore NULL。区别:量化原件图更"驯"(4×ScatterND int32 索引等 ORT 规范问题需插桩,但设备宽容)。SAM3 战役的手术经验(§8.4 外部约定收口)对 ESAM3 复活有直接参考价值。

## 7. XE90 适用性

- 逐算子验证已证明 **XE90 与 X90 对 `kirinx90` `.om` 的算子接受度完全一致**,三件套理论上可在 XE90 复测。
- XE90 CPU 14 线程(1+4+4),CPU 路线基线需重测(预计略慢于 X90 8t,仍远好于 NPU)。
- 965MB vision `.om` 在 XE90 上 restore/内存压力更大;NPU 侧同样负收益。

## 8. 复启条件与应用集成量

**复启条件**(满足其一):
1. NPU 栈修复派发税(XE90 的 Ascend C 后端是可能变量,需新版 DDK 提供 ascendc 平台);
2. 上游把 SAM3 设为默认遮罩引擎,迫使跟进。

**应用集成剩余工作量**(若启动):ModelAsset 注册(下载+SHA256)、会话与预处理契约(uint8 零归一化)、prompt padding 约定、查询选择+阈值+上采样后处理 Rust 化、内存评估(vision 原件 498MB;`.om` 路线出局)。功能上与现有 ESAM3 文本提示遮罩重叠,边际收益需先论证。
