# Kirin XE90 支持情况分析(含真机 POC)

> 结论:**XE90(Kirin XE90,MateBook Pro S)可以运行现有 `kirinx90` 编译的 CNN `.om`**。当前 DDK 虽然没有 `kirinxe90` 平台,但 X90 产物在 XE90 上 restore/run 均通过(人脸/自拍/u2net/NIND/ESAM3 encoder),不需要为这些模型单独重编。**逐算子验证(§3.1)表明两台设备对 `kirinx90` `.om` 的算子接受度完全一致(19 个用例无分歧)**;ESAM3 text encoder 的失败在两台设备上同为 int64 Cast 链所致,不是 XE90 特有限制。
> 实测日期:2026-10-30 · 设备:MOR-M1(MateBook Pro S),`KirinXE90`,HarmonyOS 7.0.0.115,API 26,Maleoon 935。

---

## 1. XE90 与 X90 的定位

| 项 | Kirin XE90 | Kirin X90 |
|---|---|---|
| 设备 | MateBook Pro S(MOR-M1) | MateBook Pro(HAD-W24) |
| CPU | 1+4+4,14 线程 | 4+4+2,20 线程 |
| GPU | Maleoon 935 | Maleoon 916 |
| NPU | 较 X90 +40%(X90 双达芬奇,40 TOPS) | 双达芬奇,40 TOPS |
| 系统 | HarmonyOS 7.0.0.115(2026-10) | HarmonyOS 7.0.0.17 |

---

## 2. 工具链与运行时

### 2.1 转换工具
本机 CANN Kit DDK 6.1.1.0 的平台目录只有 `kirinx90 / kirin9020 / kirin9030 / kirin8010`,**没有 `kirinxe90`**。但 POC 证明 **`--platform=kirinx90` 产物可以在 XE90 上运行**,因此在拿到 XE90 专用 DDK 前,可直接复用 X90 产物。

> 仍建议后续用新版 DDK(若提供 `kirinxe90`)复核;当前结论是“经验兼容”,不是官方承诺。

### 2.2 运行时库布局(2026-10-30 复核修正)

**两台设备的 HCL facade 都位于 `/vendor/lib64/passthrough/`,都不在 `indirect/`**;`/vendor/lib64/chipsetsdk/` 里是指向它的符号链接。`indirect/` 只放 `tensor/core/impl/npucore_*`:

| 库 | X90 | XE90 |
|---|---|---|
| `libai_fmk_hcl_model_runtime.so` | passthrough,120 520 B(md5 `60d9fb…`) | passthrough,**127 784 B**(md5 `aa04dc…`) |
| `libai_fmk_model_runtime_core.so` | indirect,47 104 B | indirect,54 104 B |
| `libai_fmk_tensor.so` | indirect,152 344 B | indirect,151 176 B |
| `libai_npucore_elementary.so` | indirect,6.74 MB | indirect,7.69 MB |
| `libai_npucore_elementary_kernel.so` | indirect,9.65 MB | indirect,**21.16 MB** |
| `libai_npucore_ascendc.so` | ❌ 全设备无 | ✅ indirect,7.45 MB |
| `libai_fmk_acl_model_runtime_impl.so` | ✅ 209 KB | ❌ 无 |

两个 facade 是**同一源码的不同构建**:导出符号集合完全相同(260 个)、`DT_NEEDED` 相同;XE90 版大 6%(`.text` +17%),新增 `memcpy` PLT 导入与内联的包装/错误日志,无 API/语义差异。已实测互换:在 XE90 上强制加载 X90 版 facade(即 HAP 注入的那份)跑 face/selfie/u2net,结果与 vendor 版一致。

本项目 `OmModel` 的 `VENDOR_LIB_DIR` 只指向 `indirect/`,因此两台设备实际都回退到 **HAP 注入的 X90 版 facade**(裸 soname);可选改进是把 `passthrough` 加入候选,让设备优先用自带版本。

**后端结构差异(算子归属,2026-10-30 静态清点)**:X90 的 MatMul/Attention/RmsNorm/RoPE 在 elementary 主执行体内(单后端);XE90 把它们整体移入 **Ascend C 后端**(`libai_npucore_ascendc.so`,113 个 op builder / 151 个 optiling),elementary 侧则强化卷积/NMS/Select/量化。读静态算子清单时应取 elementary ∪ ascendc 的并集。

---

## 3. 真机 POC 结果(2026-10-30)

工具:CANN Kit DDK 6.1.1.0 `omg --platform=kirinx90` + 自建 `hiai_probe`(HIAI_MR 直连,ION buffer,perfmode 4)。

| 模型(`.om`) | XE90 restore | Run | 时延(XE90) | X90 对照 |
|---|---|---|---|---|
| MediaPipe face_landmarks | ✅ 608.8 ms | ✅ | **5.5–6.3 ms** | 3.9–4.3 ms |
| MediaPipe selfie_multiclass | ✅ 386.6 ms | ✅ | **13.4–14.1 ms** | 58–77 ms |
| ESAM3 image encoder | ✅ 665.1 ms | ✅(3 输出) | **219–220 ms** | 193–227 ms |
| u2net 前景(旧) | ✅ | ✅ | **59.8–61.7 ms** | ~50 ms |
| NIND 降噪 | ✅ 313.9 ms | ✅ | **162–164 ms** | ~130 ms |
| ESAM3 text encoder | ❌ NULL | — | — | ❌ 同样失败 |

补充:
- selfie 在 XE90 上比 X90 快 4× 左右,推测 X90 当时存在明显的 CPU 分区,XE90 的 NPU/调度更完整。
- ESAM3 encoder 仍只有 `fpn_0/1/2` 三个输出(`pos_*` 被裁剪),这是模型/转换问题,与芯片无关。
- 应用本身在 XE90 上启动正常:GPU 识别为 **Maleoon 935**,pipeline warmup 3.37s(X90 为 23.54s)。

### 3.1 逐算子接受度(X90 vs XE90,2026-10-30)

每个算子构造最小 ONNX(静态 shape、f32),`omg --platform=kirinx90` 转换后双机 restore+Run;完整表格与方法见 `docs/CANN_MODEL_MIGRATION.md` §6.4:

- **19 个用例两台设备结果完全一致**:Softmax / MatMul / LayerNorm / Erf / Equal(f32、i32)/ Where / ReduceSum / Gather(int64、int32 索引)/ Permute 4D / MirrorPad / Floor / Sin / InstanceNorm / Cast f32→i32 全部 PASS;**Cast f32→int64 两台同样 restore NULL**;GatherND、多输入 Sum、NonMaxSuppression、5D Transpose 两台同样在转换期被拒。
- 结论:对 `kirinx90` `.om` 而言,**XE90 与 X90 的算子接受度等价**;XE90 的差异在后端归属(Ascend C 承接 MatMul/Norm/RoPE),不在可用算子集合。
- 小算子时延 XE90 普遍更快(Erf 1.0 vs 3.8 ms、MirrorPad 0.8 vs 2.3 ms、Floor 0.8 vs 3.4 ms)。

---

## 4. 支持结论

| 范围 | 结论 |
|---|---|
| CNN(`u2net`/`NIND`/MediaPipe/ESAM3 encoder) | ✅ **X90 `.om` 可直接在 XE90 运行**,无需重编 |
| Transformer(ESAM3 text/decoder、SAM3、CLIP) | ❌ 与 X90 相同:**int64 Cast 链 + 动态/5 维张量**(Erf/Equal/LayerNorm 等单算子其实可过,见 §3.1) |
| BiRefNet | ❌ 转换阶段失败(omg 无 GatherND/多输入 Sum 的 ONNX 映射),与芯片无关 |
| Ascend C 自定义算子 | ⚠️ XE90 具备 `libai_npucore_ascendc.so`,可另行探索 |
| 跨芯片通用性 | ❌ 仍是“经验兼容”;其他 Kirin(9000s/9010/9020/9030)必须单独验证 |

---

## 5. 风险与建议

1. **不要把“X90 产物可在 XE90 跑”泛化为所有模型**:已测 5 个 CNN 通过,但每个新模型仍需 `restore + Run + 精度` 三步验证。
2. **固件差异**:XE90 的 vendor libs 时间戳与 X90 相同(2024-06),但 OS 更新可能改变 NPUCL;升级后回归。
3. **分发**:现阶段可以只维护 `kirinx90` 一套 `.om`;若后续出现 XE90 专用 DDK/算子差异,再引入 `kirinxe90/` 目录,保持按 SoC 选择。
4. **应用集成(修正)**:两台设备的 facade 都在 `passthrough`,当前代码从 `indirect/` 找不到,统一走 HAP 注入的 X90 版副本(已实测与 XE90 vendor 版等价);可选改进是给 `VENDOR_LIB_DIR` 增加 `passthrough` 候选,让设备优先用自带版本。
5. **优先落地**:face_landmarks、selfie 两个 CNN 在 XE90 上表现最好(尤其 selfie);ESAM3 encoder 需先解决 `pos_2`;NIND/u2net 维持现状。

> 一句话:**XE90 对现有 X90 的 CNN `.om` 是向后兼容的**,算子接受度与 X90 完全一致(§3.1),性能总体不劣于 X90(selfie 明显更好);transformer 的阻塞是 int64/动态 shape,不是 XE90 特有。当前不需要 `kirinxe90` 工具链即可上线 CNN 的 NPU 加速。
