# Kirin XE90 支持情况分析

> 结论先行:**XE90 是另一款 PC 芯片(Kirin XE90,首发 MateBook Pro S),不是 Kirin X90**。当前仓库/工具链**没有 XE90 的 CANN 平台配置,也没有做过 NPU 验证**;应用可以按现有 ORT CPU 路径运行,但**不能把 X90 的 `.om` 直接当作 XE90 可用**。
> 分析日期:2026-10-30 · 依据:本机 CANN Kit DDK 6.1.1.0 平台目录实测、公开规格(2026-07/08 发布会)、仓库历史真机记录。

---

## 1. XE90 是什么

公开信息(2026-07-30 技术沟通会 / 2026-08 发布):

| 项 | Kirin XE90 | 对照 Kirin X90 |
|---|---|---|
| 定位 | 高能效 PC 处理器,首发 **MateBook Pro S** | 高性能 PC,MateBook Pro |
| CPU | 1+4+4,共 14 线程 | 4+4+2,20 线程 |
| GPU | **Maleoon 935** | Maleoon 916 |
| NPU | 较 X90 **+40%**(X90 为双达芬奇 NPU,最高 40 TOPS) | 双达芬奇 NPU,40 TOPS |
| 同代关系 | 核心参数接近 Kirin 9030 Pro | 上一代 PC 芯片 |

仓库里能对应的历史证据:GPU 导出测试用过 **MateBook Pro S / Maleoon 935**(101.9MP 图),说明项目曾经接触过 XE90 硬件;但**没有留下任何 CANN/NPU 测试记录**。

---

## 2. 当前工具链对 XE90 的支持

### 2.1 `omg` 平台配置:不支持
本机 CANN Kit DDK 6.1.1.0:

```
tools/platform/
  kirinx90/
  (libplatform.so strings: kirinx90, kirin9020, kirin9030, kirin8010)
```

- **没有 `kirinxe90`**,也没有 XE90 的 `SOC_VERSION`。
- `omg` 无法生成“XE90 专用”的 `.om`;只能产出 x90/9020/9030/8010 目标。
- `.om` 是按 SoC/平台编译的产物,**X90 的 `.om` 不保证能在 XE90 上 restore**;不能直接复用。

### 2.2 可能的替代路径(均未验证)
1. **用 `--platform=kirin9030` 试**:XE90 定位接近 9030 Pro,存在“9030 配置可被 XE90 接受”的可能,但这是 SoC 级兼容性,必须真机 restore 验证,不能假设。
2. **等待新版 DDK**:XE90 于 2026-07/08 发布,DDK 6.1.1.0 早于它;华为后续 DDK 若加入 `kirinxe90`,才是官方路径。
3. **继续 ORT CPU**:XE90 CPU 为 14 线程、9030 Pro 级,CPU 回退完全可用;对 transformer 类模型甚至可能优于 NPU(参考 X90 上 SAM3 CPU 快 6.2×)。

### 2.3 运行时 API:大概率可复用
应用侧使用的是 HIAI_MR / HCL 直连(`ohos_integration.rs` 的 `OmModel`),这套 C API 在 X90 上已验证;XE90 若继续提供同版本 HIAI 运行时,**代码路径不需要重写**,缺的是:
- 正确的 `.om` 平台产物;
- 设备端 `HIAI_GetVersion()` / `HIAI_IsSupportNpu()` / restore 验证;
- 按 SoC 选择产物的分发逻辑。

---

## 3. 与 X90 的差异对模型支持的影响(预判)

| 维度 | 预判 |
|---|---|
| 纯 CNN(u2net/NIND/MediaPipe/ESAM3 encoder) | **较可能可用**;NPU +40%,若工具链支持,收益应不低于 X90 |
| Transformer(ESAM3 text/decoder、SAM3、CLIP) | **仍不乐观**;X90 的 NPUCL 不支持 Erf/Permute/Equal/CastT 等,XE90 是否补齐取决于新固件,不能假设 |
| GatherND/Sum、RoiAlign、int64 | 取决于 XE90 的 NPUCL 算子注册表;需逐项 `omg` + restore 验证 |
| Ascend C 自定义算子 | XE90 若随新 OS 提供 `libai_npucore_ascendc.so`,可能打开自定义算子路线;X90 当前缺该插件 |
| 性能 | CNN 有望继续 10× 级;transformer 仍可能被 CPU 反超 |

---

## 4. 要在本项目中“支持 XE90”,需要做什么

### 4.1 工具链
1. 获取支持 XE90 的 DDK/`omg`(优先),确认 `omg --help` 中存在 `kirinxe90` 或官方说明 9030 配置兼容。
2. 建立 XE90 专用构建命令:`omg --platform=kirinxe90 ...`(或官方指定值),产物单独命名。
3. 记录 `omg` 版本、平台、源模型 SHA256、perfmode。

### 4.2 运行时/分发
当前代码只认 `src-tauri/libs/ohos/<abi>/` 一个目录;需要扩展为**按 SoC 分目录**:

```
src-tauri/libs/ohos/arm64-v8a/
  kirinx90/...
  kirinxe90/...
```

- 启动时读取设备 SoC/平台标识(如 `ohos.boot.hardware`、`deviceInfo` 或 HIAI 平台查询),选对应目录;
- 找不到匹配 `.om` 时回退 ORT,禁止跨 SoC 加载;
- HAP 注入脚本(`build-ohos.ps1`)增加 `-Soc` 参数,按目标 SoC 注入。

### 4.3 验证清单(有 XE90 真机后)
1. `HIAI_GetVersion()`、`HIAI_IsSupportNpu()`、设备型号/SoC 字符串。
2. 用 `hiai_mr_test7`(或等价 harness)对**同一组模型**做 `--restore-only` → `Run`:
   - CNN:u2net/NIND/face/selfie/ESAM3 encoder;
   - Transformer:ESAM3 text/decoder、SAM3;
   - 重点看 `CheckOpSupported` 是否仍拒绝 Erf/Permute/Equal/CastT/int64。
3. 性能与 PSS,和 X90、XE90 CPU 对照。
4. 精度(IoU/max_abs)与回退验证。

---

## 5. 结论

- **XE90 目前不在本项目的支持矩阵内**:现有 DDK 没有 `kirinxe90`,没有 `.om`,也没有真机 NPU 数据。
- **不能复用 X90 的 `.om`**;需要新平台产物或官方兼容性确认。
- **应用本身可在 XE90 上运行**,AI 走 ORT CPU;是否上 NPU 取决于 DDK + 真机验证。
- 推荐策略:**先 CPU 保底;拿到 XE90 真机后,用 `kirin9030` 产物做兼容性试探,同时等待 `kirinxe90` DDK**;CNN 优先,transformer 继续观望。

> 如果后续提供 XE90 设备或新版 DDK,我可以按 X90 的同一套 harness 直接补一轮 POC,并把结果并入 `docs/CANN_MODEL_MIGRATION.md`。
