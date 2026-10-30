# Kirin X90(XE90)NPU/CANN 支持情况分析

> 说明:本文只讨论 **Kirin X90**(`NPU_ohos.boot.hardware.KirinX90_v2_0`,`omg --platform=kirinx90`)。**Kirin XE90 是另一款芯片(MateBook Pro S,Maleoon 935),不在本文范围内**,其支持情况见 **`docs/KIRIN_XE90_SUPPORT.md`**。
> 分析日期:2026-10-30 · 依据:本机 CANN Kit DDK 6.1.1.0 实测、设备 `3XB0124617000013`(HUAWEI MateBook Pro / HAD-W24,HarmonyOS 7.0.0.17,API 26)以及既有 `MINDSPORE_LITE_NPU_EVAL.md` §8.4 战役数据。

---

## 1. X90 在本项目中的定位

| 项 | 值 |
|---|---|
| 设备 | HUAWEI MateBook Pro(HAD-W24 / HAD-W32),HarmonyOS PC,API 26 |
| NPU | Kirin X90(`NPU_ohos.boot.hardware.KirinX90_v2_0`) |
| GPU | Maleoon 916(与 NPU 无关;wgpu 走 Vulkan) |
| HIAI 运行时 | `HIAI_GetVersion() = 800.636.120.010` |
| NPU 探测 | `HIAI_IsSupportNpu() = -1`(文件探测值,非阻塞;实际可推理) |
| CPU | X90 20 核,支持 int8 dotprod NEON(transformer/量化模型 CPU 很强) |
| 工具链 | CANN Kit DDK 6.1.1.0,`omg --platform=kirinx90`,宿主 x86-64 后端 |

结论:X90 是**当前唯一完成 CANN/`.om` 端到端验证的芯片**;它是本项目 NPU 路线的事实基线,但不代表其他 Kirin 芯片。

---

## 2. 工具链支持情况(宿主)

| 能力 | 状态 | 说明 |
|---|---|---|
| `omg` ONNX→`.om` | ✅ | `--framework=5 --platform=kirinx90`;需 `LD_LIBRARY_PATH` 指向 `tools_omg/master/lib64`;本机二进制解释器 `/tmp/ld-linux-x86-64-2.35.so.2` 会随重启丢失,需用系统 loader 启动 |
| 宿主 pre-check / check_report | ✅ | 报告 `total/pass/fail` 与逐节点 `cause`,可提前发现不支持算子 |
| 图优化 / 常量折叠 | ✅ | `onnxsim` 配合效果显著(ESAM3 encoder 1100→262 节点) |
| kirinx90 宿主编译后端 | ✅(x86-64) | `tools/platform/kirinx90/lib64/`;加入 `LD_LIBRARY_PATH` 可启用完整 pre-check/优化 |
| Ascend C 自定义算子 | ⚠️ | 官方 `cann-recipes-harmony-infer` 已支持 Kirin 9030/X90;但设备缺 `libai_npucore_ascendc.so` 插件,应用侧暂不可用 |
| 其他 Kirin 平台 | ❌/未验证 | 9000s/9010/9020 不在官方平台配置;9030 是另一套配置,`.om` 不可跨芯片复用 |

关键点:**宿主能转 ≠ 设备能跑**。`omg` 只做图级检查,设备 restore 时会重新做算子兼容检查(`BuildModelByHcl → CheckOpSupported`),首个不支持算子即整体拒绝。

---

## 3. 设备运行时支持情况(X90 / HIAI_MR)

### 3.1 已验证可运行
- **纯 CNN 小模型**:u2net、NIND、MediaPipe face_landmarks、MediaPipe selfie(Resize 模式修正后)、ESAM3 image encoder(onnxsim 后)。
- IO 要求:**f32 + 静态 shape + ION/DMA buffer**;多输入/多输出可用,但需逐槽管理。

### 3.2 算子边界(2026-10-30 双机逐算子验证后修正)

单算子最小图(`omg --platform=kirinx90` → 设备 restore+Run)在 **X90 与 XE90 上结果完全一致**,完整表格与方法见 `docs/CANN_MODEL_MIGRATION.md` §6.4。修正后的边界:

| 阶段 | 算子/现象 | 说明 |
|---|---|---|
| ✅ 设备端可用(静态 shape) | `Softmax`、`MatMul`、`LayerNorm`、`Erf`(数值精确)、`Equal`(bool 输出,f32/i32)、`Where`、`ReduceSum`、`Gather`(int64/int32 索引)、`Permute 4D`、`MirrorPad`、`Floor`、`Sin`、`InstanceNorm`、`Cast f32→i32` | 此前“NPUCL 不支持 Erf/Equal”为过度概括 |
| ❌ 设备级硬阻塞 | **`Cast` 产出 int64**(两台设备 restore NULL) | ESAM3 text encoder restore NULL 的实证根因 |
| ❌ 转换级(omg) | `GatherND`(类型映射缺失)、多输入 `Sum`、`NonMaxSuppression`(int64 输出 verify 拒)、5 维张量(`TensorDesc is invalid`) | BiRefNet 预检查失败的 80+20 节点即前两者 |
| ❌ 固件/类型级 | `RoiAlignV2` | 固件类型级封杀(即使宿主 pre-check 通过) |
| ❌ 量化 | `DynamicQuantizeLinear`/`MatMulInteger` | omg 拒收量化模型,只能 fp32 重建 |
| ⚠️ 未单测 | `Reshape`(dimInfo 动态)、`Permute realdimCnt>4`(5D 图转换期即拒) | 与 int64 链一起构成 transformer 图的主要阻塞 |

### 3.3 性能基线(X90)
| 模型 | NPU | CPU | 结论 |
|---|---|---|---|
| u2net 320² | ~50 ms | ~580 ms | NPU **11.6×** |
| NIND 504² tile | ~130 ms | ~1215 ms | NPU **9.3×** |
| MediaPipe face_landmarks | ~4 ms | 未测(应更低) | NPU 可用但收益有限 |
| MediaPipe selfie 256² | ~58–77 ms | 未测 | 疑似 CPU 分区,需对比 |
| ESAM3 image encoder | ~193–227 ms | 未测 | 仅编码器,decoder 仍在 CPU |
| SAM3 vision | 51.9 s | 8.34 s(X90 CPU 8t) | **CPU 快 6.2×** |
| SAM3 decoder | 8.02 s | 1.33 s | CPU 快 6× |
| LaMa 768² | 10.2 s | 7.3–7.8 s | CPU 更快 |

**X90 的特性**:CPU(20 核 + int8 dotprod)对 transformer/量化模型非常强,NPU 的收益集中在 **卷积堆**;把 ViT/transformer 搬上 NPU 往往被“子图派发 + 布局转换 + fp16 转换”税吃掉。

---

## 4. X90 对模型的支持矩阵(结合最新 POC)

| 模型 | 转换 | 设备运行 | 建议 |
|---|---|---|---|
| NIND 降噪 | ✅ | ✅ | 保留 `.om` |
| u2net(旧) | ✅ | ✅ | 已被新版本模型替换 |
| MediaPipe face_landmarks | ✅ | ✅ | **优先落地** |
| MediaPipe selfie | ✅(Resize 手术) | ✅ | 可落地,需精度验证 |
| ESAM3 image encoder | ✅(onnxsim) | ✅(输出裁剪) | 需补 `pos_2` 后再评估 |
| ESAM3 text encoder | ✅ | ❌ restore NULL(int64 Cast 链) | 保持 CPU |
| ESAM3 decoder | ❌ | — | 保持 CPU |
| BiRefNet | ❌ omg pre-check 拒 GatherND/Sum | — | 保持 CPU |
| SAM / Depth / CLIP / LaMa / MoGe | 旧结论 | — | 保持 CPU |

---

## 5. 限制与风险

1. **固件/版本绑定**:`.om` 与 `kirinx90`、HIAI 800.636.120.010 强绑定;系统升级可能改变算子注册表(既有 RoiAlign/Cast 类型级判定即为例证),需要回归。
2. **不可跨芯片**:`.om` 只能给 X90 用;9030 需另一套平台配置,9000s/9010/9020 不在官方 CANN 配方内。**不能把 `.om` 放进通用 HAP**。
3. **缺少 Ascend C 插件**:`libai_npucore_ascendc.so` 只有 x86 宿主版本,设备端无 aarch64 对应物,自定义算子路线暂时走不通。
4. **int64 缺口(修正)**:int64 **输入/索引**可用(Gather 单算子 PASS);真正缺的是 **产出 int64 的 Cast**(两台设备 restore NULL)与 5 维/动态张量。bool 输出(Equal)可用。
5. **转换成功≠可运行**:text encoder 即实例;必须走“omg → 设备 restore → Run”三级验证。
6. **精度**:`.om` 中间张量按 fp16 有效精度执行(旧实测 RoiAlign 分解 mean 1.7e-3 / max 3.3e-2),蒙版类需 IoU/像素对比。
7. **内存/ION**:1008²/1024² 激活大;X90 设备可用,但 4 GB 设备/其他芯片需单独评估。

---

## 6. 建议

1. **短期**:在 X90 上只落地 MediaPipe face/selfie 两个 CNN;NIND 保持既有 `.om`;其余模型继续 ORT CPU。
2. **中期**:若必须优化 ESAM3,先解决 encoder 的 `pos_2` 输出与 decoder CPU 混合的传输成本;不要先做 text/decoder 的 NPU 移植。
3. **分发**:`.om` 按芯片/固件版本单独打包或侧载,记录 `omg` 版本、平台、perfmode、源模型 SHA256、HIAI 版本;应用内按 `os.sdkApiVersion`/芯片标识选择,失败回退 ORT。
4. **跨芯片**:在 9030/9000s/9010/9020 上重复同一 POC(`hiai_mr_test7 --restore-only` 即可判定),不要假设 X90 结论可平移。
5. **验证门禁**:转换、restore、Run、精度(IoU/max_abs)、PSS 五项全过才算“支持”。

> 一句话:X90 的 CANN 支持是**“CNN 可用、transformer 基本不可用”**;它的 CPU 很强,NPU 只在卷积模型上有明确收益。全量迁移不成立,应按模型逐个 PoC、按芯片单独分发。
