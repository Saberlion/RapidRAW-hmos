# ONNX → .ms(MindSpore Lite)转换与鸿蒙 NPU 部署方案调研

> 调研日期:2026-10-04 · 服务对象:`HARMONYOS_PORTING.md` Phase 4「(可选)MindSpore Lite / NNRt NPU 路径评估」的前置调研
> 证据标注:`[official-doc]` 官方文档 / `[source-code]` 源码 / `[release-page]` 发布页 / `[community]` 社区 / `[inferred]` 推断 / **[未验证]** 研究未能确认
> 所有外部结论锚定版本:**MindSpore Lite 2.10.0**(当前稳定版,2026-07-30 随 MindSpore 2.10 发布)

---

## 1. 背景与结论速览

**背景**:本仓 AI 功能(SAM/U2Net/skyseg/CLIP/LaMa/DepthAnything/NIND)当前全部经 ONNX Runtime(ort 2.0.0-rc.10,load-dynamic)跑 **CPU EP**(代码零 EP 注册);OHOS 端 ORT `.so` 未打包,AI 功能降级。前序结论已确认 **CANN 后端不适用**(CANN 面向昇腾 Atlas 硬件;鸿蒙手机麒麟 NPU 的应用级通道是 NNRt/HiAI,ORT 无对应 EP)。本调研评估 MindSpore Lite 的 `.ms` 路线作为鸿蒙真机 NPU 加速方案。

**结论速览**:

1. **可行,推荐路径明确**:鸿蒙上 MindSpore Lite 是**系统内置引擎**(MindSpore Lite Kit,API 10+),应用链接系统 `libmindspore_lite_ndk.z.so`(纯 C `OH_AI_*` API,约 30 个函数),配置 **NNRT(加速器)+ CPU 双设备**,逐算子自动回退——**无需打包任何推理运行时**。
2. **转换是离线工序**:Windows/Linux x64 的 `converter_lite --fmk=ONNX` 一条命令产出 `.ms`;无 macOS/Android/OHOS 版转换器。NNRT/HIAI **不要求任何特殊转换标志**(运行时在线图切分)。
3. **逐模型命运差异巨大**:纯 CNN(U2Net/skyseg/NIND)转换低风险、NPU 友好;ViT 系(SAM 编码器/DepthAnything/CLIP)转换需重导出(分解 LayerNorm/attention、int64→int32)且 NPU 收益有限(transformer 算子大多回退 CPU);**LaMa 高风险**(FFC 块依赖 FFT,ONNX `DFT` 不在支持表,除非改用无 FFT 变体否则转换直接失败);**SAM 解码器的动态点数维度**是额外考点。
4. **两个硬约束**:①DevEco 模拟器**完全不支持** MindSpore Lite Kit/NNRt Kit(官方 Kit 支持表 ARM/x86 镜像均标"否")——`.ms` 路线的全部验证(含 CPU 冒烟)都需真机;②**无现成 Rust 绑定**(crates.io/GitHub 均无),需手写约一天的 `extern "C"` 包装。
5. **与 ORT 共存而非替换**:桌面继续 ORT;OHOS 上 `.ms`-Kit 成为主路径,ORT `.so` 保留为不可转换模型(LaMa)的可选兜底。

---

## 2. 本仓现状(AI 栈盘点)

模型清单(`src-tauri/src/ai_processing.rs`,全部运行时从 HF `CyberTimon/RapidRAW-Models` 下载 + sha256 校验,落 `app_data_dir()/models/`):

| 模型 | 文件 | 输入形状 | 架构 | .ms 转换关键点 |
|---|---|---|---|---|
| SAM ViT-B 编码器 | `sam_vit_b_01ec64_encoder.onnx` | 1024×1024 **静态**(`SAM_INPUT_SIZE=1024`) | ViT-B | transformer 算子链;LayerNorm 须分解导出 |
| SAM 解码器 | `sam_vit_b_01ec64_decoder.onnx` | **动态**:`(1, pc_len, 2)` 点坐标 + `(1, pl_len)` 标签 + 低分 mask `(1,1,256,256)` + `has_mask_input` + `orig_hw (2,)` | 轻量 transformer | 动态维 `-1` + 迭代细化(点数逐轮递增) |
| U2Net(主体/前景) | `u2net.onnx` | 320×320 **静态** | 纯 CNN | 低风险 |
| skyseg(天空) | `skyseg_u2net.onnx` | 320×320 **静态** | 纯 CNN | 低风险 |
| CLIP(自动打标) | `clip_model.onnx` + `clip_tokenizer.json` | 图像分支 + 文本 token 分支(双输入;tokenizer 用 HF `tokenizers` crate) | ViT + 文本 transformer | **int64 token id 是已知雷区**(见 §7);文本分支逐 tag 推理 |
| NIND 降噪 | `nind_denoise_utnet_684.onnx` | 滑窗 tile ×3 种:`TILE_FASTER 504×504/overlap 0`、`TILE_BALANCED 504×480/6`、`TILE_HIGHER_QUALITY 504×448/12` | CNN(UTNet) | 三种静态 tile = 三份 `.ms` 或 `-1` 动态维 |
| LaMa(局部修补) | `lama_fp16.onnx` | 已 fp16(具体输入 POC 时核对) | FFC/卷积 | **FFT 依赖 → 高风险** |
| DepthAnything V2 | `depth_anything_v2_vits.onnx` | 518×518 **静态**(`DEPTH_INPUT_SIZE=518`) | ViT-S + DPT 头 | DPT 头 Resize 模式 + transformer 算子 |

运行时架构事实:

- `ort = "=2.0.0-rc.10"`,features 仅 `["ndarray", "load-dynamic"]`;所有 Session 裸 `Session::builder()?.commit_from_file()` —— **零 EP 注册,全 CPU**
- 桌面 `onnxruntime.dll`(build.rs 从 HF 拉取)实测含 CUDA/NNAPI/DML/OpenVINO/CPU,**无 CANN**;OHOS `.so`(`libs/ohos/arm64-v8a/`,csukuangfj/onnxruntime-libs 系)各机自备,本机未放置
- `lib.rs` 在 OHOS 启动时设 `ORT_DYLIB_PATH=libonnxruntime.so`(动态加载架构,换后端=换文件)

---

## 3. MindSpore Lite 生态现状(2026-10)

- **版本**:2.10.0(2026-07-30 官宣,https://www.mindspore.cn/version-updates/zh/2_10);下载页 https://www.mindspore.cn/lite/docs/zh-CN/r2.10.0/use/downloads.html
- **仓库已拆分迁移**:2.7.0 起独立仓库("MindSpore Lite has established an independent code repository" [release-page] https://github.com/mindspore-ai/mindspore-lite/blob/master/RELEASE.md);**主仓库在 AtomGit**(`https://atomgit.com/mindspore/mindspore-lite`,官方构建命令 `git clone -b r2.10 https://atomgit.com/mindspore/mindspore-lite.git`);GitHub(`mindspore-ai/mindspore-lite`)/Gitee/GitCode 为镜像;issue 在 AtomGit/GitCode,旧 `gitee.com/mindspore/mindspore` 库保留拆分前历史
- **文档两套不可混用**:端侧(.ms)`https://www.mindspore.cn/lite/docs/...`;云侧(Ascend)`https://www.mindspore.cn/lite/cloud_docs/...`(云侧 converter 甚至多出 `--optimizeTransformer` 而端侧工具没有)
- **版本漂移注意**:2.10 移除 OpenSSL 加密(`--encryption` 消失),2.8 移除 MindData 预处理——每次升级需复核算子表与 CLI;建议钉 `r2.10` tag
- **许可**:Apache-2.0 [source-code] https://raw.githubusercontent.com/mindspore-ai/mindspore-lite/master/LICENSE

---

## 4. ONNX → .ms 转换工具链(converter_lite)

### 4.1 工具获取

- 确切名称 **`converter_lite`**,打包在 runtime 发行包内,无单独下载;**仅 Windows x64 / Linux x86_64**(无 macOS/Android/OHOS 转换器)[official-doc] build.html
- Windows:`mindspore-lite-2.10.0-win-x64.zip`(https://ms-release.obs.cn-north-4.myhuaweicloud.com/2.10.0/MindSporeLite/lite/release/windows/mindspore-lite-2.10.0-win-x64.zip,含 runtime/Micro/benchmark/converter);路径 `tools/converter/converter/converter_lite.exe`,**需 MinGW-w64**,`set PATH=%PACKAGE_ROOT_PATH%\tools\converter\lib;%PATH%`
- Linux:`mindspore-lite-2.10.0-linux-x64.tar.gz`(https://ms-release.obs.cn-north-4.myhuaweicloud.com/2.10.0/MindSporeLite/lite/release/linux/x86_64/mindspore-lite-2.10.0-linux-x64.tar.gz);需 `export LD_LIBRARY_PATH=${PACKAGE_ROOT_PATH}/tools/converter/lib:$LD_LIBRARY_PATH`(glog/OpenCV 依赖)

### 4.2 CLI 与关键参数(2.10.0 全表已核对 [official-doc] https://www.mindspore.cn/lite/docs/en/r2.10.0/converter/converter_tool.html)

```bash
# 最小转换(Windows)
call converter_lite --fmk=ONNX --modelFile=model.onnx --outputFile=model
# 成功标志 "CONVERT RESULT SUCCESS:0" → 生成 model.ms
```

| 参数 | 说明 |
|---|---|
| `--fmk` | MINDIR/CAFFE/TFLITE/TF/**ONNX**/PYTORCH(需源码构建+libtorch)/MSLITE(仅 Micro)/THIRDPARTY(厂商离线模型,见 §5.3) |
| `--saveType` | `MINDIR_LITE`(默认,→`.ms`)或 `MINDIR` |
| `--fp16=on\|off` | float32 常量张量序列化为 fp16;**运行时 fp16 是 context 设置(`OH_AI_DeviceInfoSetEnableFP16`),不是转换器设置** |
| `--inputShape` | 如 `"in_1:1,3,224,224;in_2:1,64"`;优化图结构但 "may lose the characteristics of dynamic shape" |
| `--optimize` | `none`/`general`(默认)/`gpu_oriented`/`ascend_oriented`;`none`=不做离线图优化(后端中立) |
| `--inputDataFormat`/`--outputDataFormat` | NHWC(默认)/NCHW,仅 4-D;官方注:接 NCHW 规格三方硬件时指定 NCHW "will have a significant performance improvement" |
| `--configFile` | INI:量化节 + `[registry]` 扩展节 |
| `--infer` | 转换时预推理(含 `-1` 维的模型自动跳过) |

### 4.3 量化 configFile([official-doc] https://www.mindspore.cn/lite/docs/en/r2.10.0/advanced/quantization.html)

```ini
# 权重量化(推荐起步)
[common_quant_param]
quant_type=WEIGHT_QUANT        # 或 FULL_QUANT / DYNAMIC_QUANT
bit_num=8                      # WEIGHT_QUANT: [0,16],0=混合比特自动搜索;FULL/DYNAMIC: 8
min_quant_weight_size=0
min_quant_weight_channel=16
skip_quant_node=node1,node2    # 跳过指定节点

# 全 INT8 + 校准集(100~500 张,须 NHWC)
[common_quant_param]
quant_type=FULL_QUANT
[data_preprocess_param]
calibrate_path=input_name_1:/path/to/calib_dir   # 多输入逗号分隔
calibrate_size=100
input_type=IMAGE               # 或 BIN
image_to_format=RGB
normalize_mean=[127.5,127.5,127.5]
normalize_std=[127.5,127.5,127.5]
resize_width=224
resize_height=224
[full_quant_param]
activation_quant_method=MAX_MIN # 或 KL / REMOVAL_OUTLIER
```

另有 `DYNAMIC_QUANT`(权重离线/激活运行时,面向 NLP/transformer;不允许与 fp16 同用)。

### 4.4 动态 shape 语义

- **转换时不要求固定 shape**:不传 `--inputShape` 则保留动态维(`.ms` 可含 `-1`,文档化运行时类别)[official-doc]
- 运行时改 shape:`OH_AI_ModelResize`(Kit)/`Model::Resize`(C++)/`resize()`(ArkTS,`dims: Array<Array<int>>`);FAQ 约束:Resize 输入的 **rank 不得大于 Build 时 rank**
- Kirin NPU:运行时动态输入 shape 不支持 → **自动回退 CPU**;NNRT:shape 支持依驱动而定
- `--inputShapes`(benchmark 工具)可命令行灌形状

### 4.5 关于 "NPU 必须用 `--noFusion` 转换" 的辟谣

- `--noFusion` **在 2.10.0 参数表中不存在**;历史论断无法在任何现存/存档官方页面验证([community/unverified];已验证的对应物:r1.3(2021)有 `disable_fusion` **config 键**,2.9 Python converter 将 `optimize="none"` 映射为 `set_no_fusion(True)`——即现代等价物是 `--optimize=none`)
- **当前 NPU 路径(HiAI delegate 与 NNRT)均不要求特殊转换标志**:前者运行时在线图切分(≤20 个 NPU 子图),后者 MindIR 与 NNRt 内部模型镜像同格式 [official-doc]
- 现存融合控制仅在 configFile `[registry]` 节(`disable_fusion`/`fusion_blacklists`),面向自定义算子/NNIE 扩展场景

---

## 5. 鸿蒙 NPU 的三条路径(不可混淆)

### 5.1 Path A — 独立 MS Lite Kirin NPU delegate(HiAI DDK):仅 Android/EMUI,**排除**

- 启用:源码构建 `MSLITE_ENABLE_NPU=ON` + HiAI DDK 100.510.010.010(`export HWHIAI_DDK=...`),产物 `mindspore-lite-{version}-android-{arch}` 带 `libhiai*.so`;运行时 `KirinNPUDeviceInfo`(+`SetFrequency(1..4)`)为第一设备、CPU 第二,逐算子按序回退
- **构建文档明确排除 OHOS**:"MSLITE_ENABLE_NPU … only valid when the target OS is **not OpenHarmony**";"At present, **OpenHarmony only supports CPU reasoning, not GPU reasoning**" [official-doc] build.html
- 设备要求:EMUI ≥ 11,Kirin 9000/9000E/990/985/820/810 等(FAQ 列表早于 NEXT 时代芯片;9000s/9010/9020 支持情况无文档 [未验证])
- 结论:Android 应用路径,与 HarmonyOS NEXT HAP 无关

### 5.2 Path B — **NNRT(经系统 MindSpore Lite Kit):推荐路径**

- **NNRt 是 NEXT 的 NPU 硬件抽象层**:"NNRt functions as a bridge to connect the upper-layer AI inference framework and underlying acceleration chips";NNRt **只提供加速硬件**(非 CPU);当前 **56 个常用算子**(由硬件驱动实现);**仅同步推理**;支持模型缓存/priority/performance-mode/FP16 属性/厂商离线模型 [official-doc] https://gitee.com/openharmony/docs/blob/master/en/application-dev/ai/nnrt/Neural-Network-Runtime-Kit-Introduction.md
- MindSpore Lite Kit 对 NNRT 的调度:**"Model operators are preferentially scheduled to NNRT for inference. If certain operators are not supported by NNRT, then they are scheduled to the CPU"**(逐算子自动回退)[official-doc]
- C API 启用方式(官方指南代码):

```c
OH_AI_ContextHandle ctx = OH_AI_ContextCreate();
OH_AI_DeviceInfoHandle nnrt = OH_AI_CreateNNRTDeviceInfoByType(OH_AI_NNRTDEVICE_ACCELERATOR);
// 或 OH_AI_GetAllNNRTDeviceDescs(&num) + OH_AI_CreateNNRTDeviceInfoByName("...")
OH_AI_DeviceInfoSetPerformanceMode(nnrt, OH_AI_PERFORMANCE_HIGH);
OH_AI_ContextAddDeviceInfo(ctx, nnrt);
OH_AI_DeviceInfoHandle cpu = OH_AI_DeviceInfoCreate(OH_AI_DEVICETYPE_CPU);
OH_AI_ContextAddDeviceInfo(ctx, cpu);   // CPU 兜底
```

- API 级别:ArkTS `@ohos.ai.mindSporeLite` **@since API 10**(d.ts [source-code]);HarmonyOS NEXT 5.0=API 12 → 所有 NEXT 手机对三方应用可用;NNRt 自身 `OH_NN*` API 精确 since 级别 [未验证]
- **DevEco 模拟器:完全不支持**——官方 Kit 支持表(ARM 与 x86 镜像均标"否"):HiAI Foundation Kit ✗、**MindSpore Lite Kit ✗**、Neural Network Runtime Kit ✗ [official-doc 经转载 https://www.cnblogs.com/strengthen/p/18469870;佐证 https://ost.51cto.com/posts/47268]。含义:**`.ms` 路线连 CPU 冒烟都无法在模拟器做,全部验证需真机**
- 自带 `libmindspore-lite.so` 在 NEXT 上只有 CPU(见 §5.4);NPU 访问必须走系统 Kit 或 NNRt Kit 原生 API

### 5.3 Path C — HiAI Foundation Kit(华为自有 DDK):存在但另一套栈

- NEXT 上以 SDK Kit 暴露:NDK `libhiai_foundation.so` + `libneural_network_core.so`;模型用华为 `omg` 工具**离线**转换(如 Caffe 1.0),经 `OH_NNCompilation_ConstructWithOfflineModelBuffer` 在名为 **"HIAI_F"** 的 NNRt 设备上构建 [community,华为署名文章 https://huaweicloud.csdn.net/6684b36aba5a4d6394d28f93.html]
- 实践确认 NEXT(API 12+)可用 [community] https://www.ithome.com/0/872/936.htm
- 架构洞察:HiAI Foundation 的 NPU 在 NEXT 上**也以 NNRt 设备枚举**——NNRt 是统一 HAL
- MS Lite 侧另有 `--fmk=THIRDPARTY` 桥:把厂商离线模型包成黑盒 `.ms`,**仅 NNRt 后端可跑** [official-doc 镜像 https://www.seaxiang.com/blog/0cab7ffab42b46269c35cb35b32f4dae]
- 评估:绕开 `.ms` 生态、绑定华为工具链,不建议作为主路径;仅当某模型在 MS Lite 转换彻底失败且 HiAI 支持时作为个案备选

### 5.4 Standalone runtime 在 OHOS(如确需自带 .so)

- **无官方 OHOS 预编译包(设计使然)**——下载页仅云侧 Linux x64/aarch64、Android aarch64(CPU/GPU)、端侧 Linux x64/aarch64、Windows x64;HarmonyOS 上 runtime 是系统组件
- 交叉编译(2.10.0 官方支持,**仅 CPU**):

```bash
git clone -b r2.10 https://atomgit.com/mindspore/mindspore-lite.git
export OHOS_NDK=/path/to/ohos-ndk      # dcp.openharmony.cn daily builds ("ohos-sdk")
export TOOLCHAIN_NAME=ohos
bash build.sh -I arm64 -j32
# → output/mindspore-lite-{version}-ohos-aarch64.tar.gz(runtime 库 + benchmark)
```

- "Added OHOS cross-compilation support for CPU inference and on-device training"(2.10.0 release notes)[release-page];**自建 .so 无 NPU/GPU/NNRT delegate**——相对系统 Kit 只剩"runtime 版本可控"一个价值;standalone 包是否附 C API 头文件集 [未验证]

---

## 6. 集成方案设计(推荐路径)

**总原则:不打包推理运行时,链接系统 MindSpore Lite Kit;NNRT(加速器)+ CPU 双设备逐算子回退;与 ORT 按平台/按模型共存。**

### 6.1 运行时接入(本仓形态)

- 新建 `src-tauri/src/ms_lite/` 模块,`#[cfg(target_env = "ohos")]` 门控(镜像 `ohos_integration.rs` 的平台纪律;OHOS 满足 `target_os="linux"` 的既有 cfg 词汇表同样适用)
- **Rust 绑定:手写**,单文件 `extern "C"` 包装约 30 个 `OH_AI_*` 函数(bindgen 亦可,头文件在 SDK sysroot `mindspore/*.h`)。crates.io 无 `mindspore`/`mindspore-lite`/`mslite`/`ms-lite` 绑定(仅 `ohos-sys`——OpenHarmony native 绑定,未覆盖 MindSpore)[经 crates.io API + GitHub search 验证]。函数面(官方 API 参考 https://developer.huawei.com/consumer/en/doc/harmonyos-references/capi-context-h):
  - Context:`OH_AI_ContextCreate/Destroy/SetThreadNum/SetThreadAffinityMode/AddDeviceInfo`
  - Device:`OH_AI_DeviceInfoCreate(OH_AI_DeviceType)/Destroy/SetEnableFP16`(仅 CPU/GPU)/`SetDeviceId`(仅 NNRT)/`SetPerformanceMode`
  - NNRT:`OH_AI_GetAllNNRTDeviceDescs(size_t*)`/`OH_AI_CreateNNRTDeviceInfoByType(OH_AI_NNRTDeviceType)`(如 `OH_AI_NNRTDEVICE_ACCELERATOR`)/`OH_AI_CreateNNRTDeviceInfoByName(const char*)`;`NNRTDeviceDesc` 结构
  - Model:`OH_AI_ModelCreate/Build(buffer,size,OH_AI_MODELTYPE_MINDIR,ctx)/BuildFromFile/Predict(model,inputs,&outputs,NULL,NULL)/GetInputs/GetOutputs/Destroy`;`OH_AI_ModelResize` 签名需在 capi-model-h 页复核 [未验证]
  - Tensor:`OH_AI_TensorHandleArray{handle_list,handle_num}`/`OH_AI_TensorGetName/GetDataType/GetElementNum/GetDataSize/GetData/GetMutableData`
  - 类型:`OH_AI_DeviceType`(CPU=0,GPU,KIRIN_NPU,**NNRT=60**〔OHOS 区间 [60,80)〕,INVALID=100)、`OH_AI_MODELTYPE_MINDIR`(**覆盖 .ms 文件**)、`OH_AI_STATUS_SUCCESS`
- CMake 链接:`target_link_libraries(entry PUBLIC mindspore_lite_ndk)` → 系统 `libmindspore_lite_ndk.z.so`;工具链 `ohos.toolchain.cmake`(与现有 HAP 构建同链)
- `OH_AI_DEVICETYPE_KIRIN_NPU` 枚举存在但官方 NEXT 指南只演示 NNRT——NEXT 可用性 [未验证],不用它

### 6.2 模型分发

- 延续现有模式:HF 仓库(`CyberTimon/RapidRAW-Models`)增加 `.ms` 变体(每个可转换模型一个 `*_mslite.ms`),沿用下载 + sha256 + `app_data_dir()/models/` 管线;仅 OHOS 且用户启用 NPU 时拉取,节省桌面端流量
- 转换产物命名/注册表进 `ai_processing.rs` 的模型常量区,与 ONNX URL/SHA 并列

### 6.3 后端选择与共存

```
桌面(Windows/macOS/Linux):ORT(CPU,现状不变)
OHOS:
  ├─ 已转换模型(u2net/skyseg/NIND/depth/…):.ms via MindSpore Lite Kit(NNRT+CPU)
  ├─ 不可转换模型(LaMa,若 FFT 问题无解):ORT .so 兜底(可选)或功能降级
  └─ Kit 不可用(老系统/异常):功能降级(与现有 ORT 缺失降级同语义)
```

- Rust 侧抽象:现有各模型函数(SAM/U2Net/…)的推理调用点集中,可在 `AiModelRuntime` 枚举(Ort/MsLite)后按模型/平台分发;预处理/后处理(滑窗、sigmoid、mask 上采样)与运行时无关,保持共享
- CLIP 文本 tokenize 在 Rust 侧(`tokenizers` crate)已完成——`.ms` 只需接管张量计算部分,int32 化在导出端处理

### 6.4 转换流水线(仓库内固化)

- 在 CI 或本地脚本中固化转换命令(Windows converter_lite),保证可复现:

```bash
# 基线(fp32/fp16)
call converter_lite --fmk=ONNX --modelFile=u2net.onnx --outputFile=u2net_mslite
# 量化版(可选)
call converter_lite --fmk=ONNX --modelFile=u2net.onnx --outputFile=u2net_mslite_int8 --configFile=weight_quant.cfg
```

- 记录每模型的 converter 版本(2.10.0)、参数、产出 sha256 进 `ai_processing.rs` 常量区
- **先重导出再转换**(针对 ViT 系,见 §7):分解 LayerNorm/attention、int32 索引、避免 SDPA/Einsum/融合 MHA

---

## 7. 逐模型转换风险矩阵

依据:官方 ONNX 支持表(2.10.0)[official-doc] https://www.mindspore.cn/lite/cloud_docs/en/stable/reference/operator_list_lite_for_onnx.html + 端侧算子表(Kirin NPU 列)https://www.mindspore.cn/lite/docs/en/r2.10.0/reference/operator_list_lite.html。**注:未找到 SAM/SAM2/Depth-Anything/CLIP/LaMa/U2Net 的任何一手转换报告(成功或失败)[未验证];下表为算子覆盖推断 [inferred]。**

全局规则(影响所有模型):

- **任何算子都不支持 int64 输入**(int64→int32 强转;`export KEEP_ORIGIN_DTYPE=1` 实验性逃生门)
- 不支持:ONNX `LayerNormalization`(opset 17 融合算子——但 converter 融合注册表有 `OnnxLayerNormFusion/2`,**导出分解式即可**)、`Einsum`、`MultiHeadAttention`(opset 19)、`IsNaN`、`CastLike`、**`DFT/FFT`**、`RandomNormalLike`、`BitShift`
- 支持良好:MatMul/Gemm→`MatMulFusion`、Gather 全家、Softmax、全部 Reduce*、Reshape 族、Resize、Conv/ConvTranspose、BatchNorm/InstanceNorm、LayerNorm 原语(Mean/Sub/Mul/Pow/Sqrt/Div/Rsqrt)、Where/NonZero/TopK/OneHot/Range/CumSum、Erf(GELU 分解)、LSTM
- 社区实证失败案例:`scaled_dot_product_attention` 导出引入 `IsNaN`(Qwen/optimum,2025-09,https://discuss.mindspore.cn/t/topic/1184);`CastLike` + `Constant value_float not implemented`(BertModel,2.7.1,https://discuss.mindspore.cn/t/topic/1430);`Resize coordinate_transformation_mode` 不支持变体(https://discuss.mindspore.cn/t/topic/196)
- 不支持算子在 converter 日志表现为 `UNSUPPORTED OP LIST: FMKTYPE: ONNX, OP TYPE: …`

| 模型 | 转换风险 | NPU 收益预期 | 行动 |
|---|---|---|---|
| **U2Net**(320² 静态) | **低**(纯 CNN,算子全支持) | **好**——Conv2D/Pool/Concat/Softmax 是 NPU 核心 | POC 首选 |
| **skyseg**(320² 静态) | **低** | **好** | POC 首选 |
| **NIND 降噪**(3 种 tile) | **低**(每 tile 静态) | **中偏好**(滑窗本身 CPU 开销在 Rust 侧;注意 Pad/StridedSlice 对 NPU 不利) | 三份 .ms 或 -1 动态维 POC |
| **DepthAnything V2**(518²) | **中**(ViT 编码器:LayerNorm 须分解导出;DPT 头 Resize 模式需核) | **高回退比例**——`LayerNormFusion`/`Shape`/`PowFusion`/`GatherNd/GatherD` 不在 Kirin-NPU 列;NNRT 56 算子 conv/matmul 为主 | 重导出 + 真机 benchmark 决定去留 |
| **SAM 编码器**(1024²) | **中高**(同 ViT 系;避开 SDPA/Einsum/融合 MHA;position/token 张量 int64→int32) | **高回退比例**(transformer 算子大多 CPU;NNRT+CPU 异构可兜底但加速有限) | 排在 CNN 之后 |
| **SAM 解码器**(动态 pc_len) | **中高** + 动态维(`-1` 保留 + `OH_AI_ModelResize`,rank 约束需核) | 解码器本身轻,回退 CPU 可接受 | 动态 shape 专项 POC |
| **CLIP**(双分支) | **中**(**int64 token id 是已知雷区**,须 int32 导出) | 文本 transformer 大多 CPU;图像分支部分受益 | int32 重导出后转 |
| **LaMa**(fp16) | **高——FFC 块用 FFT,ONNX `DFT` 不在支持表**;除非无 FFT 变体/重导出,转换会失败 | 若不能转换则 N/A;卷积主干本身 NPU 友好 | 先确认官方 lama_fp16.onnx 是否含 DFT 节点;若含→找 FFT-free 变体或维持 ORT 兜底 |

---

## 8. POC 路线图(真机为前置)

> 硬约束:模拟器无 MindSpore Lite Kit/NNRT(§5.2)——**所有验证需真机**(带 NNRt 加速器的 NEXT 手机)。真机未到位前的可做项仅 §8.0。

**8.0 无真机可做(立即)**

- Windows converter_lite 下载 + U2Net/skyseg 转换冒烟(`CONVERT RESULT SUCCESS:0`)
- 对转换产物做**离线精度校验**:用独立 MS Lite x64 runtime(Linux/Windows 包自带 CPU)跑 `.ms`,与 ORT 跑 `.onnx` 对比输出(最大误差/PSNR)——不需要手机
- 确认 `lama_fp16.onnx` 计算图是否含 DFT/FFT 节点(Netron/pydump)
- 手写 Rust `OH_AI_*` 绑定 + `ms_lite` 模块骨架(cfg 门控,aarch64-ohos cargo check 过)

**8.1 真机到位后(按序)**

1. **Kit CPU 冒烟**:hdc 推 `.ms` 到真机,应用内(或先做最小 demo HAP)`OH_AI_DEVICETYPE_CPU` 跑 U2Net,验证系统 Kit 链路
2. **NNRT 点亮**:`OH_AI_GetAllNNRTDeviceDescs` 枚举加速器 → NNRT+CPU 双设备跑 U2Net,确认逐算子回退日志/耗时
3. **benchmark 矩阵**:每模型 × {CPU, NNRT+CPU} × {fp32/fp16, WEIGHT_QUANT int8} 记录延迟(对照 ORT CPU 基线);交叉编译包自带 `benchmark` 工具可先行做无应用测量
4. **逐模型扩展**:skyseg → NIND(tile 策略)→ DepthAnything(重导出版)→ SAM 编码器 → SAM 解码器(动态 shape)→ CLIP(int32);LaMa 视 DFT 结论
5. **集成收尾**:`AiModelRuntime` 分发 + `.ms` 下载管线 + 降级语义;全量门禁(fmt/clippy/host+aarch64 check)

**验收标准**:每模型 = 转换成功 + 精度达标(与 ORT 输出逐像素误差 < 约定阈值)+ 真机延迟数据入库 + 回退路径验证。

---

## 9. 风险与未验证项

**研究会话明确未能验证的 8 项**:

1. 历史 "--noFusion=true required for NPU conversion" 论断(无现存官方页面;现代等价物 `--optimize=none`/`set_no_fusion()`)
2. SAM2/SAM、Depth-Anything、CLIP、LaMa、U2Net 的一手转换报告(成功或失败均无)
3. `OH_AI_DEVICETYPE_KIRIN_NPU` 在 NEXT 上是否可用(枚举存在,指南从未在 NEXT 使用)
4. NNRt(`OH_NN*`)native API 的精确 since API 级别(Kit=API 10 已验证)
5. standalone 交叉编译 OHOS 包是否附带 C API 头文件集
6. `ohos-sys` crate 是否暴露任何 MindSpore 符号
7. NNRt 56 算子的具体清单(算子文档页已迁移 404;仅官方"56 common operators"数字;研究者推断其大致为 conv/pool/matmul/softmax 类、无 LayerNorm/attention——[inferred] 未验证)
8. Kirin 9000s/9010/9020(NEXT 时代芯片)在旧 HiAI delegate 中的支持(FAQ 列表早于它们)

**项目侧风险**:

- **真机依赖是硬阻塞**(与 Phase 3-3 性能基线同一阻塞点):`.ms` 路线在拿到 NEXT 真机前只能推进到 §8.0
- 模型生态单点:`.ms` 转换质量随 MS Lite 版本变化(2.x 有移除先例);需钉 r2.10 + 升级复测
- fp16 语义差:转换器 `--fp16` 与 context fp16 是两回事(§4.2);LaMa 已是 fp16 模型,转换兼容性未知
- NIND 三 tile 形状:若走单模型动态维,`Resize` rank 约束与 NNRt 驱动的 shape 支持需实测

---

## 10. 参考资料

**官方文档**

- 转换工具(2.10.0,全参数表):https://www.mindspore.cn/lite/docs/en/r2.10.0/converter/converter_tool.html
- 量化:https://www.mindspore.cn/lite/docs/en/r2.10.0/advanced/quantization.html
- 下载(2.10.0 全平台包):https://www.mindspore.cn/lite/docs/zh-CN/r2.10.0/use/downloads.html
- 构建(含 OHOS 交叉编译与 "OpenHarmony only CPU" 限制):https://www.mindspore.cn/lite/docs/en/r2.10.0/use/build.html
- Kirin NPU 集成信息(HiAI DDK,Android):https://www.mindspore.cn/lite/docs/en/r2.10.0/advanced/third_party/npu_info.md
- C++ 运行时(设备列表/回退语义):https://www.mindspore.cn/lite/docs/en/r2.10.0/infer/runtime_cpp.html
- ONNX 算子支持表:https://www.mindspore.cn/lite/cloud_docs/en/stable/reference/operator_list_lite_for_onnx.html
- 端侧算子表(含 Kirin NPU 列):https://www.mindspore.cn/lite/docs/en/r2.10.0/reference/operator_list_lite.html
- FAQ(NPU 芯片列表/动态 shape/UNSUPPORTED OP):https://www.mindspore.cn/lite/docs/zh-CN/r2.10.0/reference/faq.html
- MindSpore 2.10 发布公告(2026-07-30):https://www.mindspore.cn/version-updates/zh/2_10
- OpenHarmony NNRt 介绍(56 算子/仅同步/只提供加速硬件):https://gitee.com/openharmony/docs/blob/master/en/application-dev/ai/nnrt/Neural-Network-Runtime-Kit-Introduction.md
- OpenHarmony MindSpore 开发目录:https://gitcode.com/openharmony/docs/blob/master/en/application-dev/ai/mindspore/Readme-EN.md
- ArkTS API d.ts(@since API 10):https://gitcode.com/openharmony/interface_sdk-js/blob/master/api/@ohos.ai.mindSporeLite.d.ts
- MindSpore Lite Kit C API 参考(context.h):https://developer.huawei.com/consumer/en/doc/harmonyos-references/capi-context-h ;总览 https://developer.huawei.com/consumer/cn/doc/harmonyos-references/mindspore-lite-api

**仓库/发布**

- 主仓库(AtomGit):https://atomgit.com/mindspore/mindspore-lite ;镜像 https://github.com/mindspore-ai/mindspore-lite ;Release notes https://github.com/mindspore-ai/mindspore-lite/blob/master/RELEASE.md
- License(Apache-2.0):https://raw.githubusercontent.com/mindspore-ai/mindspore-lite/master/LICENSE

**社区/实践**

- 模拟器 Kit 支持表(经转载):https://www.cnblogs.com/strengthen/p/18469870 ;https://ost.51cto.com/posts/47268
- HiAI Foundation on NEXT(omg 工具/HIAI_F NNRt 设备):https://huaweicloud.csdn.net/6684b36aba5a4d6394d28f93.html ;https://www.cnblogs.com/HarmonyOSSDK/p/17697129.html ;https://www.ithome.com/0/872/936.htm
- 转换失败实证:IsNaN/SDPA https://discuss.mindspore.cn/t/topic/1184 ;CastLike/Bert https://discuss.mindspore.cn/t/topic/1430 ;Resize 模式 https://discuss.mindspore.cn/t/topic/196
- Kit C API 实战样例(全文镜像):https://www.seaxiang.com/blog/BU9916 ;https://www.seaxiang.com/blog/9xDL4h
