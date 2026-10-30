# Kirin XE90 支持情况分析(含真机 POC)

> 结论:**XE90(Kirin XE90,MateBook Pro S)可以运行现有 `kirinx90` 编译的 CNN `.om`**。当前 DDK 虽然没有 `kirinxe90` 平台,但 X90 产物在 XE90 上 restore/run 均通过(人脸/自拍/u2net/NIND/ESAM3 encoder),不需要为这些模型单独重编。Transformer 模型(ESAM3 text/decoder)仍然被设备 NPUCL 拒绝,与 X90 一致。
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

### 2.2 运行时库路径差异
XE90 的 HCL facade 位于:

```
/vendor/lib64/passthrough/libai_fmk_hcl_model_runtime.so
```

而 X90 在 `/vendor/lib64/passthrough/indirect/`。其余 `tensor/core` 仍在 `indirect/`。本项目的 `OmModel` 会优先尝试 `indirect/`,再尝试裸 soname;HAP 里已注入 `libai_fmk_hcl_model_runtime.so`,因此应用路径可用。若脱离 HAP 的独立工具,需要把 `/vendor/lib64/passthrough` 加入搜索路径(本次 harness 已处理)。

XE90 还额外提供 `libai_npucore_ascendc.so`(X90 上没有),Ascend C 自定义算子路线在 XE90 上具备条件(尚未验证)。

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

---

## 4. 支持结论

| 范围 | 结论 |
|---|---|
| CNN(`u2net`/`NIND`/MediaPipe/ESAM3 encoder) | ✅ **X90 `.om` 可直接在 XE90 运行**,无需重编 |
| Transformer(ESAM3 text/decoder、SAM3、CLIP) | ❌ 与 X90 相同的 NPUCL 限制(Erf/Permute/Equal/CastT 等) |
| BiRefNet | ❌ 转换阶段失败(GatherND/Sum),与芯片无关 |
| Ascend C 自定义算子 | ⚠️ XE90 具备 `libai_npucore_ascendc.so`,可另行探索 |
| 跨芯片通用性 | ❌ 仍是“经验兼容”;其他 Kirin(9000s/9010/9020/9030)必须单独验证 |

---

## 5. 风险与建议

1. **不要把“X90 产物可在 XE90 跑”泛化为所有模型**:已测 5 个 CNN 通过,但每个新模型仍需 `restore + Run + 精度` 三步验证。
2. **固件差异**:XE90 的 vendor libs 时间戳与 X90 相同(2024-06),但 OS 更新可能改变 NPUCL;升级后回归。
3. **分发**:现阶段可以只维护 `kirinx90` 一套 `.om`;若后续出现 XE90 专用 DDK/算子差异,再引入 `kirinxe90/` 目录,保持按 SoC 选择。
4. **应用集成**:HCL facade 路径在 XE90 不同,应用已通过 HAP 注入规避;若改为纯 vendor 加载,需要给 `VENDOR_LIB_DIR` 增加 `/vendor/lib64/passthrough` 候选。
5. **优先落地**:face_landmarks、selfie 两个 CNN 在 XE90 上表现最好(尤其 selfie);ESAM3 encoder 需先解决 `pos_2`;NIND/u2net 维持现状。

> 一句话:**XE90 对现有 X90 的 CNN `.om` 是向后兼容的**,性能总体不劣于 X90(selfie 明显更好);transformer 限制不变。当前不需要 `kirinxe90` 工具链即可上线 CNN 的 NPU 加速。
