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
6. **【2026-10-09 下午补充】Path C(HiAI `.om` 直连)真机验证通过**:CANN Kit `omg` 转换 + 设备侧 HIAI_MR_* C API(厂商库直连),5/7 模型 NPU 全通(u2net 83–100ms、sam3lang 161ms、sam3img 64–126s、lama 10.2s@768²、sam3dec 7.7–9.4s——RoiAlign 三连手术翻案);clip 仍阻塞于 NPUCL 未注册 Cast kernel。与 .ms 路线互补:**LaMa 唯一 NPU 通道在 .om**;详细矩阵与配方见 §8.4。

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
- **【2026-10-09 实测更新】**:此路线已真机深挖(§8.4)——`omg` 离线模型 + **HIAI_MR Model Runtime C API 直连**(非本节原文的 NNRt `OH_NNCompilation`/HIAI_F 路径,后者仍未验证),4/7 模型 NPU 全通,LaMa 因此首次上 NPU;评估由「个案备选」上调为「与 .ms 并列的第二路线」

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

> **2026-10-09 实测更新**:本表已由 §8.3 全模型真机实测验证——转换层预判大部分命中(u2net/skyseg/CLIP ✓;SAM 量化算子 ✗;DepthAnything 未映射算子 ✗)。**LaMa 预判证伪**:官方 lama_fp16.onnx **不含 DFT/FFT 节点**(fp32 重写后转换零不支持算子即证),真关卡是 fp16 `Clip` 属性解析;重写后虽可转换,但 .ms 在 Kit 2.7.0 runtime 全灭(§8.3)。运行层两处修正:①u2net/skyseg 直转 .ms 的 NPU build 崩溃,须 onnxsim 常量折叠后方可上 NPU;②CLIP 的 int64 输入 converter 可自行处理,真正卡点是动态维度(须 `--inputShape` 静态化)。

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
- ~~确认 `lama_fp16.onnx` 计算图是否含 DFT/FFT 节点(Netron/pydump)~~ **已确认不含**(2026-10-09:fp32 重写后转换零不支持算子即证;原始关卡是 fp16 `Clip` 属性解析,§8.3)
- 手写 Rust `OH_AI_*` 绑定 + `ms_lite` 模块骨架(cfg 门控,aarch64-ohos cargo check 过)

**8.1 真机到位后(按序)**

1. **Kit CPU 冒烟**:hdc 推 `.ms` 到真机,应用内(或先做最小 demo HAP)`OH_AI_DEVICETYPE_CPU` 跑 U2Net,验证系统 Kit 链路
2. **NNRT 点亮**:`OH_AI_GetAllNNRTDeviceDescs` 枚举加速器 → NNRT+CPU 双设备跑 U2Net,确认逐算子回退日志/耗时
3. **benchmark 矩阵**:每模型 × {CPU, NNRT+CPU} × {fp32/fp16, WEIGHT_QUANT int8} 记录延迟(对照 ORT CPU 基线);交叉编译包自带 `benchmark` 工具可先行做无应用测量
4. **逐模型扩展**:skyseg → NIND(tile 策略)→ DepthAnything(重导出版)→ SAM 编码器 → SAM 解码器(动态 shape)→ CLIP(int32);LaMa 视 DFT 结论
5. **集成收尾**:`AiModelRuntime` 分发 + `.ms` 下载管线 + 降级语义;全量门禁(fmt/clippy/host+aarch64 check)

**验收标准**:每模型 = 转换成功 + 精度达标(与 ORT 输出逐像素误差 < 约定阈值)+ 真机延迟数据入库 + 回退路径验证。

### 8.2 真机 POC 实测结果(NIND,2026-10-07)

**设备**:HUAWEI MateBook Pro(HAD-W32,Kirin X90 + Maleoon,HarmonyOS PC,API 26)。**§8.1 步骤 1-3(NIND)全部达成**——

| 运行时(同机同输入,504² f32 tile) | mean ms/tile | vs 应用现状(ORT CPU) |
|---|---|---|
| ORT CPU(app 现状,`libs/ohos` v1.26 .so,零 EP) | 1214.9 | 1.00× |
| MS Lite Kit CPU 4t / 8t | 1328.3 / 898.2 | 0.91× / 1.35× |
| **MS NNRT(KirinX90 NNRt 设备,perf=HIGH)** | **257.3** | **4.72×** |

精度(vs ORT Windows golden,762,048 f32):MS CPU max_abs=9.5e-07(等价 fp32);NNRT max_abs=1.6e-03(NPU 内部 fp16 级,降噪可接受)。NNRT 设备枚举:`NPU_ohos.boot.hardware.KirinX90_v2_0` + `HIAI_F`(后者 BuildFromFile 失败——需 omg 离线模型,§5.3 预判成立)。

**全管线复刻验证(同日续测,nindfull)**:1:1 复刻应用 tile 管线(镜像提取/seamless 混合/累加器/clamp)整图实测——**24MP(6000×4000,117 tiles)墙钟 ORT CPU 148.8s vs MS NNRT 32.3s(4.61×)**;6MP 44.2s vs 10.2s(4.33×;MS Kit CPU 8t 34.0s)。全图输出一致性(vs ORT 逐像素):NNRT PSNR 70.61dB / max_abs 2.9e-03;MS CPU PSNR 136.2dB(实质等同)。§8.1 步骤 4 的"NIND tile 策略"独立验证达成(尚未接入应用)。

**集成关键坑位(实测钉死,详见姊妹文档《AI_DENOISE_HW_ACCELERATION.md》§5.5 与研究日志)**:① onnx 负 Pad(裁剪)必须图手术为 Slice——MS Lite PadFusion 运行时 InferShape 失败;② **converter 版本必须 ≤ 设备引擎版本**(本机 Kit 引擎 = MindSpore Lite 2.7.0;2.10 .ms 报 "Wrong input size");③ `OH_AI_ModelPredict` inputs 按值传递;输出张量句柄跨 Predict 复用,循环间销毁会破坏后续输出;**退出收尾销毁输出句柄同样段错误**(hdc 吞信号为 exit 0,表现为尾部日志/输出文件消失)——收尾仅 ModelDestroy+ContextDestroy;④ converter 默认输入 NHWC/输出 NCHW;⑤ perf=HIGH 优于 EXTREME;⑥ 鸿蒙 PC 的 Kit 库在 `/system/lib64/ndk/`。

### 8.3 全模型 NNRT 实测矩阵(2026-10-09)

**任务**:§8.1 步骤 4 的执行——NIND 之外全部 AI 模型(SAM 编/解码器、U2Net、skyseg、CLIP、LaMa、DepthAnything V2)的 .ms 转换 + 真机 NNRT 实测,补齐 ORT CPU 基线,并验证 §7 风险矩阵的真实表现。工具:msbench v2(多输入全填/按 dtype 填充/`--shapes` 逐输入形状覆盖/正确的 API-26 `ModelResize`)+ ortbench v2(通用多输入 ORT 基线;填充仅 f32/int,fp16-IO 的 onnx 会拒载——LaMa 的 keep_io_types 导出实为 fp32-IO 故可得)。host 侧战役工件在 `%TEMP%\opencode\nnrt-campaign\`(7 模型 SHA 校验源、全部 .ms 变体、转换/运行日志、两 harness 源码、LaMa 的 fp32/sim onnx 与 fp16_to_fp32.py 重写脚本);设备侧保留 msbench2/ortbench/ORT 库于 `/data/local/tmp/`。

**转换层(converter_lite 2.7.0 直转)**:

| 模型 | 直转 | 失败原因(转换日志 `UNSUPPORTED OP LIST`) |
|---|---|---|
| u2net / skyseg / CLIP | ✓ | —(CLIP 的 int64 token 输入转出为整型 dtype 34,零填充可跑) |
| SAM 编码器 + 解码器 | ✗ | `ConvInteger` / `DynamicQuantizeLinear` / `MatMulInteger`——上游按**动态量化**导出;须未量化重导出才有下文 |
| DepthAnything V2 | ✗ | **非规范 ONNX 导出**:ATen 方言算子(`aten_add`/`aten_expand`/`aten_relu`/`aten_select`)+ torch 模块路径自定义算子块(`..._dinov2_layers_block_Block_pretrained_blocks_{0..11}_1`、`torch_nn_modules_normalization_LayerNorm_...`)——导出器未降级为标准 ONNX 算子,须上游规范重导出 |
| LaMa | ✗ | `onnx_clip_parser.cc:78 do not support data_type: 10`(fp16 `Clip` 属性解析)——§7 的 FFT 预判**证伪**(图内无 DFT);fp16→fp32 四件套重写(配方 3)后转换成功,但 .ms 在 Kit 2.7.0 runtime 不可用 |

**运行层(HAD-W32/KirinX90,4 线程对照,warmup=2 iters=5 mean)**:

| 模型(变体) | ORT CPU(4t) | MS Kit CPU | **MS NNRT NPU** | NPU/ORT | 首次 NPU 加载 | 判定 |
|---|---|---|---|---|---|---|
| NIND(Pad→Slice 手术) | 1557ms | 1392ms | **261ms** | **6.0×** | 6.0s | ✓(§8.2 延伸) |
| u2net(直转) | 582ms | 284ms | build ✗ | — | — | ✗ |
| u2net(**onnxsim**) | 582ms | 276ms | **65.9ms** | **8.8×** | 12.5s | ✓ |
| skyseg(直转) | 580ms | 289ms | build ✗ | — | — | ✗ |
| skyseg(**onnxsim**) | 580ms | 280ms | **68.4ms** | **8.5×** | ~12.5s | ✓ |
| CLIP(直转,动态维) | 448ms | Resize 成功但 predict -500 | build ✗ | — | — | ✗ |
| CLIP(**静态 inputShape**) | 448ms | 533ms | **150ms** | **3.0×** | 27.8s | ✓* |
| LaMa fp16(app 现状,768²) | **7842ms** | — | — | — | — | ORT 兜底维持 |
| LaMa fp32 重写(768²) | 7287ms | ✗(-500 / build -1) | ✗ build -1 | — | — | ✗ Kit 2.7.0 runtime |

\* CLIP 实测形状 `input_ids[64,20]` / `pixel_values[1,3,224,224]` / `attention_mask[64,20]`(batch=64 tags 假设,对齐 app 预处理 resize_to_fill 224 + tokenizer 批次最长编码);4 输出(logits_per_image/per_text、text/image_embeds)全部有效、无 NaN(全零 ids → 恒定 logit,属合成输入的正常退化)。**本轮为能力/计时冒烟(合成正弦输入),逐像素精度校验仍未做**(NIND 的精度结论见 §8.2)。

注:全表 4 线程同条件;ORT 默认线程(= app 实际行为,全核)仅补测过 u2net(596ms,≈4t);NIND 默认线程历史值 1214.9ms(§8.2),对应 NPU 加速 4.7×;CLIP/skyseg 默认线程未测。跨日数字含热状态/负载漂移,同日同进程比较最可靠。LaMa 两行以 app 最坏工作尺寸 768² 方张量实测(`run_lama_inpainting`:裁剪→768 限长→64 对齐方张量),其余模型原生尺寸;fp32 重写版输出分布与 fp16 版一致(max 254.125→254.175 / mean 95.939→95.945,零 NaN)——数值保真。

**三条新增已验证转换配方**(与 NIND 手术并列):

1. **onnxsim 常量折叠(u2net/skyseg 类)**:u2net.onnx 的 38 个 Resize 全部动态 sizes(`Shape→Gather→Unsqueeze→Concat` 链;节点直方图 Constant 266 / Concat 127 / Conv 119 / **Shape 114** / Gather 76 / Unsqueeze 76)→ 转出的 .ms 中间张量形状不可静态推理 → NNRt 编译期 `SetShape failed, shape is nullptr` ×4 → HIAI `BuildLiteGraph` 原生崩溃(有 DfxFaultLogger 栈)。**`python -m onnxsim` 折叠后 1055→371 节点**(Shape/Gather/Unsqueeze/Slice/Constant 全归零,Resize sizes 变常量),NNRT 立即可建,4.2×/4.1×。`--optimize=none` 无效且更差(SetShape 失败 ×7)。skyseg 同架构同配方复刻成功。
2. **静态形状转换(CLIP 类)**:动态输入维 .ms 的 NNRT build 直接失败;`OH_AI_ModelResize` 到具体形状返回成功后 predict 仍 -500(Kit 2.7.0)。转换期 `--inputShape='input_ids:64,20;pixel_values:1,3,224,224;attention_mask:64,20'` 静态化后 NNRT 可建,3.6×(vs MS CPU)。**集成代价**:文本 batch 维锁死——app 现按 tag 数动态,须 padding 到固定值或按形状缓存多份 .ms。
3. **fp16→fp32 重写(LaMa 类 keep_io_types 导出)**:fp16 body + fp32 IO 的模型死在 `onnx_clip_parser.cc:78 do not support data_type: 10`。python(onnx 1.23)四件套:① graph IO + 全部 value_info 的 `elem_type` 10→1(LaMa 3682 处);② fp16 initializer → fp32(614 个);③ **`Cast` 节点 `to` 属性 10→1(13 处——漏掉则 ORT 拒载 "does not match expected type (tensor(float16))"、MS runtime predict status 18)**;④ **拓扑排序(原始导出乱序:`node_Shape_1` 先于其生产者;`onnx.utils.sort_graph` 在 1.23 不存在,手写 Kahn ~20 行)**。产出须过 `onnx.checker.check_model(full_check=True)`;LaMa 重写版 ORT 可跑且输出分布与 fp16 版一致(数值保真证据见上注)。脚本:`%TEMP%\opencode\nnrt-campaign\models\fp16_to_fp32.py`。

**Kit 2.7.0 API/行为实测勘误**(harness 开发中钉死,修订本档案前文):

- **`OH_AI_ModelResize` 真实签名**:`(model, inputs, OH_AI_ShapeInfo *shape_infos, size_t shape_info_num)`,`OH_AI_ShapeInfo{size_t shape_num; int64_t shape[OH_AI_MAX_SHAPE_NUM=32]}`(sysroot `mindspore/model.h` L131-133)——**非** before/after 回调式;按回调式传 null 即 `-2 "model or shape_infos is nullptr"`。§6.1 的 [未验证] 标注就此解决。
- `OH_AI_TensorSetShape` 返回 **void**(tensor.h L137)。
- **teardown SIGTRAP**:.ms 运行后显式 `TensorDestroy/ModelDestroy/ContextDestroy` 收尾会在全部证据打印完毕后触发 `Signal 5` 退出码 133(NIND/u2net/skyseg 一致)——命令行基准工具直接 `exit(0)` 跳过收尾。§8.2 坑位③的细化。
- **无逐算子回退(实测修正 §5.2 表述)**:所有 NNRT build 均打出 `Disable heterogeneous inference in Delegate`——delegate 将整图作为单一全模型 kernel 交 NPU;全图 NPU 编译失败(如 u2net 直转)即整图 build 失败。context 中 CPU 第二设备仅覆盖非 delegate 算子,救不了 delegate 内部的编译崩溃。
- **HIAI_F 设备**:`OH_AI_CreateNNRTDeviceInfoByName("HIAI_F")` 后 delegate 仍走 `BuildKirinNPUModel`,且卡在 vendor 库命名空间(`libhiai_hcl_model_runtime.so`/`libhiai_ir_infershape.so`/`libai_single_op.so` 对 /data/local/tmp shell 进程不可加载)——§5.3「HIAI_F 需 omg 离线模型」预判部分成立;应用进程内(正常命名空间)行为待验。
- **LaMa .ms runtime 全灭(8 变体实测)**:fp32 重写版(配方 3)转出的 .ms——动态变体 predict -500,NNACL `matmul_infer.c:47 CheckMatmulInputShape` 在 `node_matmul_1` 拒绝(a_shape[-1]≠b_shape[-2]);静态变体编译期 `scheduler.cc:911` 在 `node_div_12`(DivFusion)InferShape 失败 → build -1。与布局(`--inputDataFormat=NCHW` 实测有效,声明保持 `[-1,3,-1,-1]`)、优化等级(`--optimize=none`)、onnxsim(3116→3095 节点)均无关。动态变体的 NNRT build 亦败于 NNRt `SetShape failed, shape is nullptr`(与 CLIP 直转同签名);静态变体未达 NNRt(MS 侧先败)。结论:converter 自身推理通过但产物形状元数据与 runtime 独立推理结构性矛盾——**此 LaMa 导出(乱序+混合精度+FFC)在 Kit 2.7.0 不可用,解锁须上游干净重导出(fp32、规范算子)**。
- **converter 布局可控(实测钉死,flag 本身见 §4.2 表)**:`--inputDataFormat`/`--outputDataFormat`(NHWC|NCHW,仅 4 维 IO)——默认空=自动,卷积模型被改写为 NHWC(LaMa `image[-1,-1,-1,3]`);显式 NCHW 保持源布局(实测声明 `[-1,3,-1,-1]`)。对 NPU build 的影响未单独验证。
- **首次 NPU 加载耗时**(DLSA `modelLoadTime`):NIND 6.0s / u2net_sim 12.5s / clip_static 27.8s——集成需异步预热;DLSA 模型缓存(`RestoreFromCacheFile`,需配缓存路径)未启用,是后续优化点。
- MS Kit CPU vs ORT CPU(同 4 线程):u2net/skyseg MS **快 ~2×**(284/289 vs 582/580ms);CLIP MS 慢 19%(533 vs 448ms);NIND MS 快 ~11%(1392 vs 1557ms)。
- 传参坑:`--shapes` 值经 PowerShell→hdc→设备 shell 时 `;` 在引号被吃后成为命令分隔符——两 harness 均改用 `+` 分隔。

**遗留(接入应用前)**:① u2net/skyseg/CLIP 逐像素精度校验(vs ORT golden);② CLIP 固定 batch 的 padding/多形状缓存策略;③ SAM/depth/LaMa 待上游重导出(SAM/depth:未量化/规范算子;LaMa:fp32 干净导出)后重试 .ms 路线;④ LaMa 现阶段维持 ORT 兜底——**fp32 重写版可直接作 ORT 直替**(7287ms vs 7842ms @768²,输出保真,免去 fp16 边界 Cast);⑤ .ms 输入 NHWC(u2net `[1,320,320,3]`)vs app 预处理 NCHW——集成需转置,或试 `--inputDataFormat=NCHW`(LaMa 实测可保源布局,NPU 影响未验);⑥ 若固化管线:onnxsim 与 `--inputShape` 步骤并入 `scripts/convert-*-ms.ps1` 模式。

### 8.4 Path C 实证:CANN Kit `.om` 直连 HIAI Model Runtime(2026-10-09 下午)

**动机**:§8.3 后 `.ms` 路线仍有死角(LaMa runtime 全灭、SAM3 系无数据)。§5.3 的 Path C 被重新激活:绕开 converter≤引擎版本约束(§8.2 坑位②),用华为 **CANN Kit DDK 6.1.1.0**(WSL,`~/cannkit/DDK-tools-next-6.1.1.0`)的 `omg` 转 `.om`,设备侧以 **HIAI_MR_\* Model Runtime C API**(`/vendor/lib64/passthrough/indirect/` 厂商库直连,无 SDK 无文档,签名全部反汇编定出)执行。设备:§8.2 同机(KirinX90,API 26),`HIAI_GetVersion()=800.636.120.010`。

**宿主转换配方**(实证):

```bash
LD_LIBRARY_PATH=$DDK/tools/tools_omg/master/lib64 omg \
  --framework=5 --model=<in>.onnx --output=<out> --platform=kirinx90 \
  --input_shape=image:1,3,768,768;mask:1,1,768,768 \
  [--weight_data_type=FP32]     # lama 必须,其余模型默认 FP16 即可
```

- omg 自身 7 个宿主库(`libomg.so`/`libfmk_common.so` 等)在 `tools_omg/master/lib64`,不设 LD_LIBRARY_PATH 直接 `error while loading shared libraries`
- 成功标志 `OMG generate offline model success`——但**宿主成功 ≠ 设备可跑**:`RestoreFromFile` 在设备端**重新做算子兼容检查+编译**(`RestoreFromBuffer→BuildModelByHcl→Compile→CheckOpSupported`),首个不支持算子即整体拒绝(`no runtime support the Model`)
- `--weight_data_type` 默认 **FP16**(fp32 权重自动降精度)——lama 必须显式 FP32(手术 #2,见下)

**八项模型手术**(python onnx,宿主侧;病灶均为设备 hilog 实证):

| # | 模型 | 病灶 | 手术 |
|---|---|---|---|
| 1 | lama | `BatchMatMul don't support` ×144:FFC-FFT 的 `MatMul(const[96,96] × 5D[1,192,49,96,1])`,CPUCL batch_matmul 限 x2 ∈ 2-4D | onnxsim 覆写输入形状 + **dim_param 算术表达式求值**(`((((((width-1)//8))+1)//2))+1`→49 @768,共 1656 处) + batch 折叠 x2 5D→3D(`[9408,96,1]`,Reshape 双侧包裹 ×144) |
| 2 | lama | 折叠后 restore 过但 `Init rc=1`:`Op[node_add_419,Add] Init error — input0Type_ ≠ input1Type_` | `--weight_data_type=FP32`:默认 fp16 权重转换会改写 MatMul 输出类型但**不穿透 Reshape 包装**,Add 两侧 fp16/fp32 撞型(烙死在 .om 内;`.om` 亦从 103MB→205MB) |
| 3 | sam3dec | `GatherV2D don't support`(node_GatherND_14_fix):Gather data 限 fp32/fp16/int32 | 全图唯一 uint8 口袋(Cast→Gather→3×Cast)一处 Cast `to` UINT8→FLOAT,一行手术 |
| 4 | clip | `Unsqueeze_10 ExpandDims don't support`:CPUCL expand_dims 限 FLOAT/INT32/UINT8/BOOL | 79× INT64 Unsqueeze + 1× INT64 Squeeze 全部 int32 Cast 双侧包裹(修掉了该 fatal,但撞上手术外的设备洞,见矩阵) |
| 5 | (harness) | sam3lang `Run rc=1` 即时失败:0.5f 填充 int32 tokens → `0x3F000000`=1056964608 越界 token id(词表 49408) | int32 输入必须 `--fill 0.0`(全零字节对 int32/fp32 均合法) |
| 6 | sam3dec | `ROIAlignV2 don't support`(固件类型级封杀,微模型定案) | `rl_surgery.py` 90 节点原语分解:K=4 固定采样 + torchvision aligned=False 精确语义 + 平铺 1-D int32 Gather 索引;mini 模型 vs numpy-torch = 2.9e-06(详见 §8.4 手术记录) |
| 7 | sam3dec | `Init rc=1`:`CPUCL Gemm_131(GemmD) input2 dtype should be fp32, but now it is fp16` | `--weight_data_type=FP32`(手术 2 同方;98.8MB→161.8MB) |
| 8 | sam3dec | `Run rc=1`:`node_select_88 GatherV2D run error`(Init 过、执行 8.8s 后炸) | 常量索引 `[-1]→[5]`(6 路多路复用末位选择;设备 GatherV2D 运行期不支持负索引,详见 §8.4) |

**结果矩阵**(shell 进程直连,md5 双端核验,Run×3;u2net 为 v4 harness 首测数据,余为本轮):

| 模型 | .om | restore | Init | Run | 判定 |
|---|---|---|---|---|---|
| u2net | 88MB | 532.8ms | ✓ | **82.9/96.5/100.5 ms** | ✓ 全通;**perfmode=4:48.8–50.6ms(1.9×,反超 `.ms` 65.9ms)** |
| sam3lang_v6 | 808MB | 1.9–3.8s | ✓ | **171.8/160.9/161.0 ms** | ✓ 全通(tokens int32,fill 0.0);perfmode=4:135ms(1.2×) |
| sam3img_v3 | 965MB | ~3.4s | ✓ | **63.9/126.3/77.8 s** | ✓ 全通但极慢、方差大(3 个 vision_pos_enc `.bin` 伴生随推,是否运行时依赖未单独验证);**perfmode=4 真图首跑 51.9–58.3s(冷/暖,§8.4 优化块)** |
| lama(手术 1+2) | 205MB | ~1.2s | ✓ | **10.7/10.2/10.2 s** | ✓ 全通;比 ORT CPU 7.3–7.8s(§8.3)慢 ~1.3×(perfmode=4 无益,保持默认) |
| nind(Pad→Slice 手术件,AI_DENOISE §5.6) | 62MB | 217.7ms | ✓ | **188.8/188.5/188.2 ms** | ✓ 全通;**三路线最快**(vs .ms NNRT 257.3ms 快 1.37×,vs ORT CPU 1214.9ms 快 6.45×),NCHW I/O 保持(免 .ms 的 NHWC 转置);**perfmode=4:125.8–130.6ms(1.8×,vs .ms 快 ~2×)** |
| clip_v6 | 361MB | ✗ ~1.3s | — | — | ✗ NPUCL `cast_strategy SelectKernel: kernelId not registered: 589827`(int32→int64 Cast):**算子检查通过但设备端编译缺 kernel** |
| sam3dec_v9 | 99MB | ✗ 355ms | — | — | ✗ `ROIAlignV2 don't support`(uint8 手术后前移至此;GridSample 尚在其后——2-strike 止) |
| sam3dec_roi_v2(手术 6+7+8) | 162MB | 1106.7ms | ✓ | **9408/8223/7722 ms** | ✓ **全通翻案**;fill-0 下 4 输出语义健康(presence_logit=-16.2 强"无物体"、boxes∈sigmoid 区间);2-strike 死刑经三连手术推翻,详见 §8.4 手术记录 |
| depth | — | — | — | — | ✗ 转换阶段判死,未获 .om(前次会话结论) |

**SAM3 溯源与 URL 原件形态(2026-10-09 补测)**:战役 sam3 三件套溯源自代码 URL 基座仓库(`CyberTimon/RapidRAW-Models`,上游 2026-10-07 新增):`sam3_vision.onnx`(498MB)/`sam3_text.onnx`(355MB)/`sam3_decoder.onnx`(33.6MB)+ `sam3_tokenizer.json`(CLIP 系 BPE,词表 49408——与 sam3lang 真机 Gather `limit[49408]` 精确互证;本体 = Meta SAM 3.1,gitcode 镜像克隆 `facebook/sam3.1` 证实)。**仓库原件全部为动态量化导出**(vision:MatMulInteger×192/DQL×135/ConvInteger×7;text:×145/×97+IsNaN×24;decoder:×260/×205/×4+RoiAlign×1+Einsum×1)——omg 预检查实证直转即拒(`DynamicQuantizeLinear/MatMulInteger is not supported`,与 app 现役 SAM ViT-B 同款量化病);战役全部测试件 = 这些原件的**反量化 fp32 重建**(尺寸比 3.6–4.0× 吻合 INT8→FP32;MatMulInteger 260→Gemm 228+MatMul;Einsum×1 对上),但**反量化脚本未在战役工件中找到**——固化管线前需重建。原件与战役件差异:vision 原件 4 输出(多 `fpn_pos_2`)+ 动态 batch,战役件 3 输出 + 静态 batch=1。该仓库同期还新增 `moge-2-vits-normal-onnx.onnx` 与 `vit_t_encoder/decoder.onnx`(fork 代码均未引用——上游模型集演进信号)。HF 原件存档:WSL `~/cannkit/hf-sam3/`(sha256 与 HF API LFS oid 逐一核验)。

**外部检索交叉印证(2026-10-09,网络检索)**:① 官方全链路无 SAM 系列支持——`cann-recipes-harmony-infer`(仅 6 个自定义算子 recipe:QuantMatmul/BandNorm/RmsNorm/SliceGelu/SobelCustom/GatherDequantInt8)、`cann-recipes-infer`(仅 LLM/多模态)、MindSpore 模型库(official+community)、ModelZoo-PyTorch 均无 SAM。② **decoder 阻塞根因实锤为芯片级内核缺失**:官方算子库 `gitcode.com/cann/ops-cv`、`ops-nn` 的 per-op 产品支持表中,`aclnnRoiAlign`(ops-cv/objdetect/roi_align)、`aclnnGatherNd`、`aclnnScatterNd`(ops-nn/index/*)支持 950/A3/910B/310P/910,**均不列 Kirin X90**——与真机 "ROIAlignV2 don't support" 互证;ops-cv 项目级宣称"支持 Kirin X90"但 per-op 表不含,且 harmony-infer issue #65/#67 记录过算子文档"虚标 Kirin 双芯片"先例(文档宣称不可信,真机实测为准)。③ 社区干净 fp32 源存在,反量化缺口不再是瓶颈:`danilobukvic/sam3-text-onnx`(三件套 fp32:vision 1.84GB/text 1.35GB/decoder 96MB,附导出/量化/校验脚本,与 PyTorch 位级对齐;**其 text-only decoder 导出无 RoiAlign/GridSample/Einsum/GatherND**,值得单独 census+omg+真机验证)、`wkentaro/sam3-onnx-models` 与 `eyepop-ai/sam3-onnx-models-opset-19/14`(decoder 246MB 臃肿版)、`kishanstar2003/SAM3_ONNX_FP16`(含 `geometry-encoder.onnx` 32MB——box/point 几何路径,即我们 RoiAlign 的来源);Meta 官方无 ONNX 导出,生态中**不存在**任何现成反量化工具(onnxruntime/onnxsim/PINTO 系列均无)。④ RoiAlign/GridSample 分解配方:TVM issue #18928(参考实现 `python/tvm/topi/testing/roi_align_python.py`,roi_align=坐标计算+bilinear 插值+pooling,即 Gather+MatMul+插值算术)、ncnn pnnx PR#3478;华为官方补救路径=**Ascend C 自定义算子**(harmony-infer 6 个 Kirin 先例)。⑤ `970334745/sam3.1-npu` 证明 SAM 3.1 可在 Ascend 910B 服务器栈经 torch_npu 全流程跑通(`npu_roi_align` 原生可用)——但 torch_npu 不存在于 HarmonyOS,该路径对本项目不可用。

**SAM3 decoder 异构算子全量 census 与分解设计(2026-10-09,roialign_probe.py)**:战役 fp32 decoder(2418 节点,14 输入含动态 box_coords[8,1,4]/box_labels[8,1]/box_masks[1,8],4 输出)的异构算子共 11 个,**全部为可分解的退化案例**:`RoiAlign×1`(8 ROI×7×7 avg、sampling_ratio=0 自适应、spatial_scale=1.0、boxes=动态 box_coords、72×72 FPN 图);`GridSample×1`(**常量网格** [1,1,1,2] 单点双线性,onnxsim 可整体常量折叠);`GatherND×4`(全部用常量索引 val_6=[0] 做 batch 维选择→Gather(axis,const)/恒等);`ScatterND×2`+`ScatterElements×2`(**索引全为常量**(0 与 0..8 与 const expand),仅更新值动态→Equal+Where one-hot / Concat 零片);`Einsum×1`(bqc,bchw→bqhw 掩码头≈8.5 GFLOP,sam3_einsum_fix.py 已有 Reshape+MatMul 手术)。RoiAlign 分解=sampling_ratio 固定为 k=2(自适应→静态,数值近似可接受)+ output_half_pixel 仿射坐标 + floor/Cast int32/平铺索引 Gather(axis=1 on [256,5184],fp32 data+int32 动态索引=已验证类)+ 双线性权重 Mul/Add + 逐 bin 平均(ReduceMean 未知则用 MatMul-ones 替代);Min/Max 钳位边界。全部落在 kirinx90 已验证算子集内。带 dtype 卫生(uint8 Cast、int64→int32 包裹)后,估计净增 ~80-120 节点(onnxsim 静态折叠后更少)。

**ROIAlignV2 文档矛盾仲裁(2026-10-09 深夜,变体矩阵+双栈实锤)**:官方算子表(developer.huawei.com/consumer/cn/doc/doccenter-capabilities/cannkit-supported-operators)列 ROIAlignV2/GridSampler2D/GatherNd/ScatterNd 属实——但该表描述的是 **CANN Kit 宿主编译栈能力**:DDK 6.1.1.0 的 `tools/platform/kirinx90/lib64/` 全套库为 **x86-64 宿主侧 kirinx90 编译后端**(readelf 实证;omg 需将其加入 LD_LIBRARY_PATH 才会加载——v1~v9 全部历史构建都因缺失走"内核选择推迟到设备"模式,构建日志 `dlopen so failed: libai_npucore_itf.so` 为证),其后端注册表**接受** ROIAlignV2(加载后端后宿主预检通过且 NPUCL 图优化 pass 真实执行)。**设备固件(2024-06,HIAI 800.636.120.010)在 restore 时做类型级封杀**:`model_compatibility_check.cpp CheckOpSupported` 对 ROIAlignV2 一律拒收——①变体矩阵实证配置无关(batch_indices int32 / X+rois fp16 包裹 / sampling_ratio=2 / int32+ratio 组合,4 变体同错);②构建模式无关(宿主后端构建的 sam3dec_v9h.om 同样被拒);③固件 elementary.so 内 roialign 内核代码存在(`run_executor_roialign_fp16_v210`/`CheckroiAlignTensorSupport`)但运行时类型注册表未注册(与 clip CastT kernelId 589827 同款"代码在、注册无"病);④`coordinate_transformation_mode` 属性为 omg 解析器必需(删除后宿主即拒)。**关键收敛:整图唯一 fatal 就是 ROIAlignV2**——完整 hilog 显示 GatherNd/ScatterNd/GridSample/Sign/Sub/ExpandDims 等仅产生 fe_lib store 分配警告(W 级,CPUCL/其他 store 接管),无第二个 fatal,故 decoder 手术范围收缩为**仅分解 RoiAlign**(常量 GridSample 由 onnxsim 折叠、Einsum 已有手术)。**omg 配方升级**:`LD_LIBRARY_PATH` 追加 `$DDK/tools/platform/kirinx90/lib64` 可加载 x86 后端、启用宿主预检与图优化(提速本地迭代;不改变设备端判定——设备判定权在固件)。剩余路径排序:①图手术分解 RoiAlign(主推,设计见上);②固件/OS 更新观察(未来 hiaiservice 注册该类型后原 .om 直接可用,无需重转);③Ascend C 自定义算子;④decoder CPU 混合。⑤**微模型孤立验证定案(2026-10-09)**:单节点 RoiAlign 微模型(mmcv 属性,ratio=0/2 两版,batch_indices 作图输入)与 Conv 控制组对照上真机——控制组 PASS(104.8ms,管线无恙),两个 RoiAlign 微模型同报 `type ROIAlignV2 don't support`——**本机固件不支持 RoiAlign 为类型级判定,无图上下文/配置/构建模式任何依赖,证据链闭合**。附带两个 omg 工具坑:mmcv RoiAlign 的 batch_indices 用 const initializer 会让宿主 omg 在内存分配阶段 SIGSEGV(必须作图输入);标准 ONNX RoiAlign 属性集(无 coordinate_transformation_mode)不可解析——omg 只认 mmcv 变体且该属性必填,不存在映射到 "ROIAlign V1" 的通道。

**RoiAlign 算子实现与 sam3dec 全通(2026-10-09 深夜,手术 6+7+8,三连翻案)**:

- **病灶 census**:fold 源图(opset 18,2078 节点)node_roi_align 为 mmcv 变体(attrs:output_half_pixel/avg/7×7/sampling_ratio=0 自适应/spatial_scale=1.0),X=view_2 [1,256,72,72] fp32,R=val_208 [8,4](即动态输入 box_coords),OUT→node_conv2d:Conv——单实例单消费链,分解后原位拓扑替换。
- **分解实现**(`rl_surgery.py`,90 节点):K=4 固定采样(自适应→静态);**torchvision aligned=False 精确语义**(对照 roi_align_kernel.cpp 源码逐条核验):无 +1、无 round、退化框强制 1×1(Max)、采样 `y=y1+i·bh+(s+0.5)·bh/K`、越界 `y<-1||y>H` 贡献 0(GreaterOrEqual/LessOrEqual 构造合法掩码);平铺 1-D int32 Gather 索引(N=8×49×16=6272,fp32 data+int32 动态索引=已验证类);逐 bin 平均 ReduceMean(axis=3)+Transpose 出 [8,256,7,7]。全部算子落在 kirinx90 已验证集(Gather/Mul/Add/Sub/Div/Max/Min/GreaterOrEqual/LessOrEqual/And/Floor/Cast/Reshape/Expand/ReduceMean/Transpose/Slice/Squeeze/Unsqueeze)。开发期三修:形状/轴常量必须 int64(ONNX 规范)、ReduceMean 轴=3 非 4、拓扑原位插入。
- **数值验证**(`rl_validate.py`):mini 模型 vs numpy-torch 参考实现 = **2.9e-06**(实现精确);K=4 vs 原自适应采样在随机噪声数据上 0.43 max/0.03 mean(最坏情形——真实平滑特征远小,且 K=4 比 torchvision 常用 ratio=2 更细)。**约定仲裁**:ORT 标准 RoiAlign 与两种半像素约定均不符(自有怪癖,不可作参考);mmdeploy 的 output_half_pixel=torchvision 约定(论证性,残留风险已记录);mmcv 文档明示半像素差异对 Conv 下游无影响。
- **真机三连排障**(每轮 hilog 实证):①ROIAlignV2 类型级封杀→手术 6 分解(上);②`sam3dec_roi.om`(默认 fp16 权重)restore PASS 910ms 但 `Init rc=1`——CPUCL `Gemm_131(GemmD): input2 dtype should be fp32, but now it is fp16`→手术 7 `--weight_data_type=FP32`(lama 手术 2 同方;v9 从未活到 Init,病灶首暴露);③FP32 版 Init rc=0、执行 8.8s 后 `Run rc=1`——`node_select_88 GatherV2D run error`→census 实锤**常量索引 [-1]**(PyTorch 末位选择语义:decoder 算 6 路输出变体后取末位,4 个输出选择 Gather node_select_88/90/94/95 共享同一常量)→手术 8 `[-1]→[5]`(dim=6,数学等价)。**副产品结论**:结构性负轴(Squeeze[-1]/[-2]、Slice axes[-1]、Concat axis=-1、ReduceSum axes[-1])设备全部正常执行——omg 归一化结构性轴,唯 GatherV2D 的负索引**数据**在运行期炸。
- **终局**:`sam3dec_roi_v2.om` 161,751,994B:restore 1106.7ms / Init rc=0 / Run×3 = 9408/8223/7722 ms 全 rc=0 **PASS**;fill-0 输入下 4 输出语义健康(pred_boxes∈[0.009,0.803] sigmoid 区间、pred_logits -1.60~-1.14、pred_masks -4.33~0.20、presence_logit_dec=-16.23 强"无物体"——非 NaN 非垃圾)。管线:`rl_surgery.py → onnxsim(2113 节点,RoiAlign:0)→ fix_negidx.py([-1]→[5])→ omg --weight_data_type=FP32`。
- **残留**:①~~全 decoder 端到端数值对齐仍未验~~ **已收口**(见下块张量级误差分解:E1 纯执行误差 relL2≈1e-4,top-5 查询与 fp32 参照完全一致);②此前"omg 构建期折叠 5 个图输入(14→9)"经 census 更正:vision_pos_enc_0/1、language_embeds、original_height/width 在 ONNX 里是 **DANGLING(悬空无消费者)**——omg 只是丢弃未用输入而非折叠,反量化重建件本身就没接这些线(decoder 语义上不依赖它们);应用集成按 .om 实际 9 输入契约喂数;③decoder 计算 6 路多路复用后仅取末位(index 5),约 6× 冗余计算,后续可探索裁剪;④dma_heap `SetBufferName failed errno:9` 对微型输出 ION 缓冲(4 字节)出现但分配实际成功(无害,记录在案)。

**RoiAlign 设备数值误差实测(2026-10-09 深夜,harness v5)**:方法=harness v5 新增 `--infile idx:path`(输入按文件喂数)+`--dump prefix`(输入输出缓冲落盘);`rl_mini.onnx`(产线同形:X [1,256,72,72]+ROIS [8,4] 动态输入→make_rl 90 节点,ORT 已验 2.9e-06)以 `--weight_data_type=FP32` 转 .om 上真机;测试向量=rl_validate 同套(rng42 正态 X + 8 分支场景框:内部/小框/大框/全图/跨界外/退化零宽/角落/细长);三方对比。**结果:设备(CANN) vs ORT-CPU 同图同输入 = max 3.26e-02 / mean 1.67e-03 / p99.9 1.82e-02;ORT-CPU vs 语义参考 = 2.92e-06(复核通过)——误差全部来自设备执行,分解语义零漂移**。归因=**fp16 有效计算精度**(典型相对 ~0.5%;bf16 会大 ~8× 排除;NPUCL 向量核原生 fp16,固件内核名 `roialign_fp16_v210` 同证;`--weight_data_type=FP32` 只保权重不保中间张量);omg 工具链 strings 实证**无 precision_mode/allow_fp32/force_fp16 类旗标**,不可收紧。结构正确性:零值一致 100352/100352、无 O(1) 级索引错位(gather 错位会表现为 O(1) 误差);输入落盘字节级一致(ION 写路径无恙);8 分支场景误差同量级(角落小框最差 3.26e-02、退化零宽框 4.38e-03)。**结论:分解在设备上以 fp16 有效精度执行,对下游 Conv 掩码头与整图 fp16 化同阶,可接受**。副产物:①harness v5 文件喂数/落盘成为后续全 decoder 对齐与真实数据验证的基建;②omg 输入序=拓扑首用序(mini 实测 in[0]=X/in[1]=ROIS,decoder 亦然)。

**RoiAlign 速度实测(2026-10-09 深夜,同图同输入,稳态)**:

| 平台 | 实现 | 耗时(med) | 倍率 |
|---|---|---|---|
| CPU(i7-8700 12t,ORT 1.30) | 90 节点分解图 | **28.2 ms** | 1× |
| CPU(同上,单线程) | 90 节点分解图 | 98.3 ms | 3.5× |
| CPU(同上,ORT 原生) | 单算子 RoiAlign(ratio=4) | 2.11 ms | 0.075× |
| CPU(numpy 纯 Python) | K=4 语义参考 | 83.5 ms | 3.0× |
| **设备(Kirin X90 .om)** | **90 节点分解图** | **2208.8 ms**(10 轮 2124–2259,无冷热差) | **78×** |

- **设备比桌面 CPU 慢 ~78×**(vs 单线程 22×,vs 纯 Python numpy 参考仍慢 26×)——远超桌面↔移动SoC 的典型差距,且设备自身 CPUCL 片段仅需 ~2ms(见下),差距本质在执行栈形态而非算力。
- **归因(hilog 实证)**:图被自动切分为 ~5 个 NPU 子图 + 4 段 CPUCL 片段交替执行;CPUCL 片段实际计算合计 **~2ms/run**(`cpu_runtime: 121/264/216/1655us` 等)——**~2.2s 全部耗在 NPU 侧**;NPU 侧 stub 名实锤机制 = fp16 向量核(`executor_mul_cw_broadcast_half`)+ **`four2five_nchw2nc1hwc0`/`five2four_nc1hwc02nchw` 布局转换 + `fp162fp16`/`fp162fp32` dtype 转换**(每次 CPU↔NPU 段切换都付费)。计算量仅 ~10 MFLOP 量级,2.2s 意味有效吞吐 ~5 MFLOPS——**布局/dtype 转换与小算子派发主导,非计算主导**;fp16 计算与误差实测结论互证(stub 名自带 `_half`)。
- **CPU 侧分解税**:原生单算子 2.11ms → 90 节点 28.2ms(13.4×,ORT 每小算子 ~0.3ms 开销);12 线程仅提速 3.5×(小算子链并行度有限)。
- **decoder 语境**:RoiAlign 分解 ≈ 2.2s,占 decoder 总时长 7.7–9.4s 的 **~25–28%,为最大单一开销块**。提速路径按性价比排序:①**RoiAlign 段切出 .om、设备 CPU 执行(见下块,~5ms vs 2.2s)**;②裁 6× 多路复用冗余(砍 decoder 其余计算);③分解重构(合并 4×Gather、避免 5D 广播形态、减少段切换);④固件注册 RoiAlignV2 后原生直用;⑤Ascend C 自定义融合算子。

**RoiAlign 分解性能归因与优化空间定量(2026-10-10,X90 CPU 参照系下的再分析)**:

- **"分解性能差"成立且可定量**:decoder 终版 2113 节点 8.02s = 3.8ms/节点平均;RoiAlign 分解段 90 节点独占 2.2s = **24.5ms/节点——单位节点成本是其余段(~2000 节点 5.8s ≈ 2.9ms/节点)的 ~8.5×**,以 4% 节点数吃掉 27% 时长,为 decoder 内最差性能密度段(注:ONNX 节点数 ≠ NPU 派发数,omg 有融合,但 8.5× 的差距远超融合度差异所能解释)。根源非算法——数值 2.9e-06 精确、计算量仅 ~10 MFLOP;hilog 实证 CPUCL 片段仅 ~2ms,**2.2s 全耗在 NPU 侧小算子派发 + fp16 广播向量核(`mul_cw_broadcast_half`)+ 段切换 four2five/five2four 布局/dtype 转换**——90 个小算子的"千次派发税",恰是 NPU 执行栈最反感的形态(对照:桌面 ORT 跑同一 90 节点图仅 28.2ms = 派发成本低 78×;设备 CPU 特化 5.14ms)。
- **但分解不是 NPU 输掉 SAM3 的主因**:即使 RoiAlign 零成本,NPU decoder 5.8s vs X90 CPU 1.32s(原生 RoiAlign ~2-5ms 含在内)仍慢 4.4×——transformer 段自身就输;encoder 51.9s vs 8.3s 更与 RoiAlign 无关。
- **优化空间存在,按杠杆排序**:①已收割:perfmode=4 → 1.08s(2×);②**Gather+BatchMatMul 重构**(主方案):权重/索引数学全部限制在小张量(≤8×49×784),数据通路收敛为 **1×Gather(平铺索引,已验证类)+ 1×BatchMatMul**(8×[49,784]@[784,256]≈78M MAC,NPU 矩阵单元强项;BatchMatMul x2 限 2-4D 已满足),90 → ~35 节点、段切换 9 → ~3——按每节点派发税经验模型估算 2.2s → **0.5-0.9s(再 2-4×)**(税模型为经验拟合,实测为准);③次级:合并 4×Gather、消 5D 广播形态、消 4 段 CPUCL 回退;④decoder 级更大杠杆:裁 6× 多路复用冗余(作用于输出头附近,收益上限 <全图 6×)。
- **战略判定:不值得再投入**——CPU 路线全链真图 9.6s 已赢 NPU 60s 达 6.2×,NPU 侧 RoiAlign 重构天花板(decoder 8.0→~6.5s)仍慢 CPU 路线 ~5×;分解件价值重定位 = **NPU 兜底路径技术储备 + 未来 NPU 栈修复派发税后的复用构件**(彼时 Gather+MatMul 化方案与全套验证工具链直接可用)。

**6000×4000 真照片端到端基准与全链路验证(2026-10-09 深夜)**:

- **基准件**:合成 24MP 照片(6000×4000,天空/山峦/草地/房屋/树,房屋轮廓位置已知;`bench/photo6000x4000.png`);提示词=房屋归一化 cxcywh `[0.62, 0.675, 0.174, 0.25]`。
- **金标准(桌面 ORT + HF 原件)**:原件量化三件套全部可被 ORT 直接加载(含"mmcv"RoiAlign——其属性集实为标准 ONNX 属性 `output_half_pixel`,omg 与 ORT 双认)。vision(22.4s)→ fpn_0/1/2 + pos_2;decoder 经验扫参锁定推理约定:**box_labels∈{0,1}、1=正框**(label_embed 仅 2 行,label=2 直接越界崩);语言侧 box-only 填 ones+全真 mask 最佳(top logit 4.73)/zeros+全假亦可(2.21,均 IoU>0.94);输出选择=argmax(pred_logits)→`pred_masks[0,q]>0`,288×288 归一化图像坐标。金标准 mask IoU vs 解析轮廓=0.953。
- **设备全链路**:照片→PIL 双线性 1008×1008 **uint8 原始像素(零归一化——模型输入 dtype 即 uint8,图内无 mean/std 节点,头链直入 patch ConvInteger)**→ sam3img_v3.om(restore 4.61s + run **67.4s 冷 / 125.8–128.2s 持续**,热节流主导)→ FPN dump 直留设备喂 sam3dec_roi_v2.om(restore 0.95s + run **8.08/7.63/7.68s**)→ mask 上采样回 6000×4000。encoder dump 直喂 decoder 免 111MB 往返。
- **验证结果全 PASS**:设备 mask(288 原生)IoU vs 解析房屋轮廓 **0.954**(金标准 0.953);**设备 vs 金标准 mask IoU 0.985**(top 查询序号 157≠25 属正常——查询无序近并列,mask 才是本体);presence_logit **+9.11**(fill-0 时 -16.2,强激活);恰好 1 个查询 logit>0.5;**全分辨率 6000×4000 IoU 0.978**(764,982 vs 764,951 预期像素)。encoder 特征交叉核对:device(反量化+fp16 执行)vs 金标准(int8 量化)corr 0.93/0.69/0.66——量化↔fp16 双向近似差,decoder 对此稳健(mask 一致);pos_2 常量与原件输出数值同一(maxdiff 5.96e-08,corr=1.0)。
- **端到端基准(24MP 单次)**:设备冷 **~82s**(4.6+67.4+0.95+8.1)、持续 **~140s**(热节流后 encoder 126–128s 主导);桌面金标准(i7-8700,量化 ORT)~26s。**encoder 是绝对瓶颈(占 86–91%)**,RoiAlign 手术段(~2.2s)仅占全链 ~2.6%——此前"切 .om 上 CPU"优化的全链路收益上限确认为 ~2%。
- 集成注记:mask 后处理(查询选择+阈值+上采样)宿主侧秒级,应用侧将由 Rust 承担;vision_pos_enc_2 用模型自带常量(.bin);prompt 槽位 padding=box_coords 0/labels 0/box_masks false;语言 ones 变体在无文本提示时可直接复用。工件:WSL `~/cannkit/bench/`(photo/gold_*/enc_out*/dec_out*/final_overlay.png/final_mask6000.png)、脚本 gen_photo/gold_sweep/device_prep/final_verify/fix_overlay。
- **外部约定交叉核对收口(2026-10-09,librarian 检索 facebookresearch/sam3 源码 + 实测验证)**:①Meta 参考预处理=uint8→resize→0-1→Normalize(0.5,0.5)→[-1,1],HF 导出把该仿射**折叠进量化尺度**(ConvInteger 的 scale 吸收了 2/255 与 -1)——故模型直吃 uint8 原始像素,实证与源码互洽;分辨率 1008=Meta 处理器默认(FPN 288 由 ViT/16→63×63→FPN 上采样而来,非 stride-4 直除);②box=cxcywh 归一化、labels∈{0,1}(1=正框,label_embed 2 行,encode_boxes_as_points=False 因 RoiAlign/boxes_pool 在图内)——与实证一致;③**box-only 语言侧权威约定=文本编码器跑字面量 `"visual"`(哨兵串)**,tokenize=[49406,220,17004,49407] 补零到 32;text.onnx 原件双输入(input_ids+attention_mask int64),输出 text_mask **True=有效 token(前 4 真 token)/False=padding——ONNX 导出为翻转前约定,与 PyTorch 源码的翻转后约定相反,以实测为准**;实测金标准(visual 语言)q=157/IoU=0.953/presence=9.54,**与设备 top 查询 157 完全一致,设备 vs visual-金标准 mask IoU=0.994**(ones 填充捷径 vs visual-金标准 0.985,亦可接受);④评分公式=σ(pred_logits)×σ(presence)>0.5(单提示下与 argmax+logit>0 等价;σ(x)>0.5⟺x>0);mask 上采样=bilinear align_corners=False;boxes=cxcywh→xyxy×[W,H,W,H];⑤溯源更正:sam3 三件套上传于 ~2026-05(非 10-07,该日期是 moge 上传);danilobukvic/eyepop-ai/kishanstar 三仓 webfetch 404(私有或已删),HF 卡片为空——**权威源=facebookresearch/sam3(源码级)+ 本战役实证**。

**6000×4000 张量级误差分解与性能汇总(2026-10-09 深夜,sam3dec_ortfix 参照)**:

- **fp32 同源参照构建**:`ortfix_dec.py` 迭代修复 sim2 的 ORT 加载障碍——5 处 Cast 插桩收敛(1×Add int32/int64 混型 + **4×ScatterND int32 索引违反 ONNX 规范**——规范只许 int64,设备宽容能跑、ORT 严格拒收,又一例证;Cast 保值不伤语义)→ `sam3dec_ortfix.onnx` 即设备 .om 的 fp32 同源图,ORT 可跑(3.4s/次)。**长期残留"全 decoder 数值对齐未验"至此收口。**
- **误差分解**(同 6000×4000 照片、同提示词,relL2 对 pred_logits[200]):

| 分量 | 对比 | logits relL2 | masks relL2 | 备注 |
|---|---|---|---|---|
| **E1 纯执行误差** | 设备 .om vs ortfix(同设备特征) | **1e-4** | 0.11* | presence 差 8.8e-05;**top-5 查询完全一致**;选中 mask IoU 0.9925;boxes relL2 1e-4 |
| E2 编码器特征传播 | ortfix(设备特征) vs ortfix(金标特征) | 0.11 | 0.46 | 编码器量化↔fp16 执行差 |
| E3 激活量化 | 原件 int8 vs ortfix(同金标特征) | 0.32 | 1.94 | **最大误差源=原件自身 int8 量化** |

  \* masks 的 relL2 由 199 个被弃用查询的噪声主导(近随机输出,阈值后丢弃);有效度量=选中 mask IoU 0.9925。**结论:设备执行罚项 ~1e-4 可忽略;vs 量化原件的总差异由原件量化主导(E3≫E2≫E1);四种跑法 IoU_house 全部 0.9515–0.9541。**
- **性能汇总(24MP 单张,X90 设备)**:

| 阶段 | 耗时 | 说明 |
|---|---|---|
| 宿主预处理(resize 1008) | 0.09s | PIL 双线性;应用侧 Rust 等价 |
| encoder restore / run | 4.61s 一次性 / **67.4s 首跑,125.8–128.2s 后续** | 热节流主导(首跑反而最快) |
| decoder restore / run | 0.95s 一次性 / **7.63–8.08s** | 含 RoiAlign 手术段 ~2.2s |
| 宿主后处理(上采样+阈值) | 0.08s | |
| **单张端到端** | **~81s(首次)/ ~139s(热稳态)** | restore 计入;免 restore 后 ~67s/126s |
| (参照)i7-8700 桌面 ORT | ~26s(量化原件) | enc 22.4 + dec 3.2 |
| (参照)sam3lang 文本侧 | 161ms | "visual" 哨兵串,可忽略 |

  **encoder 占 86–91% 为绝对瓶颈**;harness 的 ~180MB 文件往返为测试专用开销,应用内全在设备内存无此成本。

**同芯片(Kirin X90)CPU vs NPU 实测(2026-10-09 深夜,rl_cpu_test.c + harness v6)**:

| 执行路径 | 耗时 | 精度(vs numpy 参考) |
|---|---|---|
| 设备 CPU 单线程(特化 C:预计算双线性参数,torchvision 语义,同输入) | **5.14 ms**(best-of-5) | max 5.3e-06(fp32 精确) |
| 设备 CPU 多线程(2/4/8/16/20t) | 7.9–11.5 ms | 同上;**工作量太小,线程 spawn/同步开销反噬,多线程反不如单线程** |
| NPU via .om(CANN,同计算 90 节点分解) | **2208.8 ms** | max 3.26e-02(fp16 有效精度) |

- **同芯片差距 ~430×(NPU 慢),且 NPU 版本同时损失精度**——该工作负载(小算子链、~10 MFLOP 低算术密度)与 NPU 执行栈(布局/dtype 转换+小算子派发主导)完全错配;SoC 为 20 核(4×0xd42 + 16×0xd43,/proc/cpuinfo)。对照:桌面 i7-8700 跑泛型 90 节点图 28.2ms——设备 CPU 特化实现比桌面泛型快 5.5×。
- **后端戳实验阴性**(harness v6 `--backend 0/1` 移写 ModelInitOptions 首字段 backend_type,disasm 注:2=HCL/NPU):两值均 Init/Run 照常、~2.6s 不变、输出与 backend=2 逐位一致——**.om 的 NPU 子图划分在构建/restore 期已烙死,执行期无 CPU 重路由杠杆**。
- **集成最优路径浮出**:**把 .om 在 RoiAlign 边界切开**(前半 .om 以 view_2 [1,256,72,72] + val_208 [8,4] 为新增输出;设备 CPU 侧 Rust/C ~5ms 算 roi_align [8,256,7,7];再喂后半 .om 从 node_conv2d 起)——省 ~2.2s(decoder 总时长 -25~28%),代价仅 ~5.7MB 设备内张量搬运一次;且该段回归 fp32 精确(精度红利)。比等固件注册/Ascend C 都现实。

**SAM3 速度优化:facade 合法性能旋钮清查与扫参(2026-10-10,harness v7)**:

- **旋钮发现与验证**:`dev_facade.so` 导出经全量清查实为 **260 个**(71×HIAI_MR_* + 189×HIAI_HCL_*),其中藏有真实性能旋钮:`HIAI_HCL_ModelInitOptions_SetPerfMode/GetPerfMode/SetNpuCoreNum/SetBandMode`、`HIAI_HCL_Config_SetComputeSpeedMode/SetProcessPerfMode/SetModelPriority`、`HIAI_HCL_ModelManager_SetPriority` 等。harness v7(=v6 增 `--perfmode/--npucore/--bandmode N`,`ModelInitOptions_Create` 后、`Init` 前调用)以 **Get/Set/Get 三点验证**写入生效:默认 get=2(与加载日志 `prio[7],perf[2]` 互证),set(4) 后 get_after=4。**setter 返回值为无意义垃圾整数**(逐次调用不同),不可作成败判据;`perfmode=5` 拒收(get_after 仍 2)→ 有效域 0–4。
- **mini(rl_mini.om,RoiAlign 90 节点小算子链)全档扫参**:0=2595/2507ms、1=2657/2574ms(省电档反慢 15–20%)、**2(默认)≈2140–2260ms**、3=1196/1202ms、**4=1076/1100ms(2.0×)**;npucore 1/2/4/8 全档无感(纯链式图无并行度)。语义:0/1=省电、2=默认、3=高性能、4=最高。
- **encoder(sam3img_v3.om,24MP 真图)**:perf4 首跑 **58.3s(基线 67.4s,1.16×)**,后续 102.7/132.6s(热节流仍主导,但同位 ≤ 基线 125.8/128.2);输出 4 位小数统计与默认档完全一致(数值影响见下)。restore 不受旋钮影响(旋钮在 restore 之后的 Init 路径)。npucore 未获干净测试(PS 引号剥离坑吞掉一轮;mini/decoder 双阴性,期望低)。
- **decoder(sam3dec_roi_v2.om)干净 A/B(120s 冷却,逐档 45s 间隔)**:**默认 perf2 最优 8001.9ms**;perf4=9127.5ms(另一热态首跑 9150.9 复现——**真实劣化 ~14%,与热无关**);perf4+npucore2=10246.3、+npucore4=10904.2(npucore 有害);bandmode(默认 0)0/1/2=10014/11412/13081ms(热污染叠加但单调劣化,无益)。**perfmode 跨模型非单调,须按模型选档**。
- **生产配置定型(终测复核)**:**encoder=perfmode 4、decoder=默认(2)**;RoiAlign 链型小算子图用 perf4 可 2×。终测(重启后全冷态):encoder run **51.9s** + decoder run 8.02s = **run 合计 59.9s(基线 75.5s,1.26×)**;端到端含 restore(2.68+0.95s)**~63.5s vs 基线 ~81s(1.28×)**。热态下增益收窄(暖机 58.3s/深热 102.7s,仍优于基线同位)——encoder 占 88% 使整体增益被稀释。
- **"FP16 权重重转"杠杆已天然用尽**:v3 构建命令(日志实证)无 `--weight_data_type` 旗标——encoder .om 权重本就是 fp16(1.945GB fp32 ONNX → 965MB .om=0.50×),且无 CPUCL Gemm fp16 病(Init/Run 全通过)。
- **数值影响(已收口,2026-10-10 设备恢复后全链复验 4/4 PASS)**:①**确定性**——两次 perf4 运行(隔一次设备重启)`cmp` 三输出逐位一致,NPU 同配置执行完全确定;②perf4-vs-默认档差异定量——fpn0/1/2 relL2=1.08e-04/1.81e-04/1.08e-04、corr≈0.99999999、meanabs 6.8e-07~5.4e-05(fp16 执行噪声档,~1000× 小于管线本已容忍的 E2 编码器特征传播误差 0.11),差异 100% 归因性能档(调度/融合顺序改变)而非运行噪声;③**全链 parity——perf4 链 vs 原链 mask IoU=1.0000**,top-5 查询完全一致 [157,165,179,4,58],presence delta 1.24e-05,288 IoU_house=0.954、全分辨率 6000×4000 IoU=0.978(双双等于基线);④**生产配置终测(重启后全冷态)**:encoder perf4 restore 2.68s + run **51.9s**、decoder 默认档 restore 0.95s + run 8.02s。掉线事件经过:最终验证首跑中设备 USB 掉线(hdc 空、仅 RNDIS 枚举、1142 端口扫描仅 53/DNS 开——网络控制通道排除,根因=重启后 USB 模式回落网络共享),设备侧解锁/重插后恢复,/data 工件全部无损。
- **死路与受阻项收档**:npucore(mini 零效应/decoder 劣化)、bandmode(decoder 劣化)、perfmode 5(拒收)、backend 戳(v6 阴性,.om 子图划分构建期烙死)、omg 优化旗标(strings 实证不存在);**dopt 工具链发现但受阻**——DDK `tools/tools_dopt/dopt_onnx_py3/`(graph_optimize: constant_fold/eliminate/fuse 图优化)因 Python ABI 不可用(`dopt.so` 依赖 `_PyUnicode_Ready`,py3.12 已移除;WSL 仅 3.12.3,需 py≤3.11 环境方可评估),且其增益未证(omg 转换期已跑 NPUCL 图优化)。
- **残余优化路径**(未动工,按性价比):①分辨率手术(1008 静态形状全链耦合,侵入大);②decoder 裁 6× 多路复用冗余(仅占 12%);③Ascend C 自定义融合算子;④等固件注册 RoiAlignV2。encoder 58s 的 NPU 侧构成(派发 vs 内核)未分解——hilog 缓冲被系统噪声冲掉,仅捕到 restore 期放置协商(158×reshape+15×split check 拒绝,非运行期回退)与 0 条运行期 CPUCL 行(全 NPU 执行的弱证据)。

**perf 旋钮推广:全模型扫参与路线排名反转(2026-10-10,harness v7)**:

| 模型 | 默认(perf2) | perfmode=4 | 增益 | 数值抽查(fill-0 stats) |
|---|---|---|---|---|
| u2net(320²,7 输出) | 79.5–100.5ms | **48.8–50.6ms** | **1.9×** | min/max/mean/first 全一致 |
| nind_v2(504²) | 226.8–248.4ms | **125.8–130.6ms** | **1.8×** | 同上 |
| sam3lang_v6(text) | 160.6–171.8ms | **135.1–163.2ms** | **1.2×** | text_memory/text_embeds 全一致 |
| lama_fp32w(768²) | 10.17–11.77s | 11.06–11.82s | ✗ 无益(稳态 ~9% 反慢) | —(不推荐 perf4) |

- **规律收敛**:perf4 收益与"派发开销占比"正相关——小算子密集图(u2net 嵌套卷积链、nind、RoiAlign 链)大赚 1.8–2.0×;大计算量图(lama FFC、sam3dec transformer)不领情甚至反慢——与 SAM3 战役的 encoder/decoder 分化同构。**全模型统一策略:CNN/小算子密集图用 perf4,transformer/FFC 大算子图用默认(2)**。
- **路线排名反转**:**u2net `.om`@perf4 ~50ms 反超 `.ms` 65.9ms**(默认档 83–100ms 曾落后 1.3×)——`.ms` 的最后一块速度优势消失;nind 领先从 1.37× 扩至 ~2×。
- **数值安全**:三模型 perf4 输出 stats 与默认档逐位一致(SAM3 战役已实证机制:性能档只改调度/融合顺序,数值差为 fp16 噪声档 ~1e-4,全链 IoU 无影响)。
- **残余差距盘点(2026-10-10,优化后)**:①**lama**:10.2s vs 设备 CPU ORT 7.3–7.8s(1.3×)——perf 旋钮无效,结构性(大计算量带宽受限),剩余路径=接受功耗/CPU 卸载权衡或模型手术;②**sam3img**:51.9s vs 桌面 ORT 22.4s(2.3×)——剩余为派发/转换开销,路径=分辨率手术/dopt(受阻)/Ascend C;③**sam3dec**:8.0s vs 桌面 3.2s(2.5×)——默认最优,路径=裁 6× 多路复用冗余、RoiAlign 段切出;④**clip**:`.om` 阻塞于 NPUCL Cast 洞(kernelId 589827),唯一路线 `.ms` 150ms,若要 `.om` 须全图 int64→int32 手术;⑤**depth**:转换期死,无 `.om`,须走 `.ms` 或转换手术。

**X90 CPU 参照补测与 SAM3 路线策略翻转(2026-10-10 下午,ortbench v2.1 bool/uint8 填充补丁)**:

参照系修正:前述各表以桌面 CPU(i7-8700)或跨路线互比为参照;应用实际运行在 X90 上,**同芯片 CPU 才是 NPU 路线的真实对手**。§8.3 已有 u2net/nind/lama/clip 的 X90 CPU(ORT v1.26 零 EP)数据;本块补齐 SAM3 三件(HF 量化原件——正是 §8.3 当时因量化算子转换死路而缺席 CPU 实测的那批)并统一重排。工具:ortbench(补 Bool/Uint8 零填充两 case 重编,增量 6.4s;模型推至设备 `/data/local/tmp/sam3cpu/`)。

| 模型 | X90 CPU(ORT,默认全核) | X90 CPU(8t) | X90 NPU(.om 最优档) | NPU vs CPU |
|---|---|---|---|---|
| sam3 vision(1008²,HF 量化) | 9.28s | **8.34s** | 51.9s(perf4 冷) | **慢 6.2×** |
| sam3 decoder(HF 量化) | 1.59s | **1.33s** | 8.02s(默认) | **慢 6.0×** |
| sam3 decoder(ortfix fp32 同源) | 1.96s | — | 8.02s | 慢 4.1× |
| sam3 text(HF 量化) | 50ms | — | 135ms(perf4) | 慢 2.7× |
| u2net | 582ms(4t,§8.3) | — | **50ms(perf4)** | **快 11.6×** |
| nind | 1215ms(默认线程,§8.2) | — | **130ms(perf4)** | **快 9.3×** |
| clip | 448ms(4t,§8.3) | — | **150ms(.ms NNRT)** | **快 3.0×** |
| lama(768²) | 7287ms(fp32,4t,§8.3) | — | 10.2s | 慢 1.4× |

- **结论翻转:SAM3 三件全部 X90 CPU 大胜**——全链 CPU(8t)≈**9.7s**(8.34+0.05+1.33)vs NPU 60s(51.9+0.14+8.02)= **CPU 快 6.2×**;"SAM3 走 `.om`"策略作废,修正为 **SAM3 走 ORT CPU(HF 量化原件直用——零手术、零转换、精度已被金标准管线验证 IoU 0.953)**。X90 20 核 + int8 dotprod NEON 把量化 transformer 跑到 8-9s,比 i7-8700 桌面(22.4s)快 2.4×——X90 CPU 是被此前参照系低估的猛兽。
- **8t > 全核**(vision 8.34 vs 9.28s、decoder 1.33 vs 1.59s,快 11-16%):ORT 默认全核超订阅反噬——应用集成设 `intra_threads=8`(更快且留核给 UI/渲染)。
- **NPU/CPU 分野定型**:NPU 赢 CNN 卷积堆(u2net/nind 9-12×);输 transformer 量化矩阵乘(SAM3 2.7-6.2×)与 FFC(lama 1.4×)——NPU 子图派发/布局转换税对大 matmul 图是净损失,唯 conv 堆的融合收益盖得过税。**且 NPU 栈吃不了量化模型**(omg 拒 DynamicQuantizeLinear/MatMulInteger)→ NPU 路线被迫 fp32 重建(965MB vs 量化件 498MB),带宽雪上加霜——结构性劣势。
- 注记:①CPU 数据为合成输入计时(形状一致、稠密计算,数据无关);量化件精度=金标准管线已验(IoU 0.953/0.954 级);②CPU 连续多张热行为未测(2-3 iters 无衰减迹象;NPU 同条件 51.9→102.7s 剧烈衰减);③lama 全核未测(表用 4t,全核只会更快,CPU 胜局不变);④SAM3 CPU 管线内存:量化三件共 887MB ONNX(常驻可只留 vision+decoder 532MB)。
- **战役遗产重定位**:`.om`/HIAI_MR 执行配方、图手术工具链、perf 旋钮(对 NPU 路线模型 u2net/nind 生效)、全套对标数据 = 未来固件/NPU 栈演进时的复测基线;SAM3 的 `.om` 件降级为能力证明与备份路线。

**X90 CPU 真照片端到端验证(2026-10-10,6000×4000 基准件,ortbench v2.2 `--infile/--dump`)**:

- 管线:enc_in_f32(真图 1008² 像素值)→ 量化 vision(8t)→ 真 FPN → 量化 decoder(真 FPN + 同套 in_* 提示件 + pos2 常量,labels int32→int64 转换)→ mask → 上采样 6000×4000。
- **计时(真数据,8t)**:vision **8.28s**(8.07–8.50,与合成输入 8.34s 一致——稠密计算数据无关再证)+ decoder **1.32s** → **全链 run 9.6s vs NPU 60s(CPU 快 6.2×)**;宿主预处理(6000×4000→1008 resize ~0.1s)两路线同价另计。
- **语义验证 PASS**:IoU_house(288)= **0.953**(=金标准 ones 变体同值);全分辨率 = **0.982(> NPU 0.978)**;presence 6.75 / top logit 4.78(ones 变体特征值,金标准同款 4.73);q=25(ones 变体查询号,≠NPU 的 157 属近并列正常——战役定论"mask 才是本体");**CPU-vs-NPU mask IoU 0.9853**——三路线(桌面金标准/设备 NPU/设备 CPU)mask 互验全部 0.985+。
- fpn 交叉核对注记:cpuvis vs gold_fpn relL2=0.11/0.27/0.33(corr 0.99/0.96/0.94)——与 NPU-vs-golden(0.11;corr 0.93/0.69/0.66)同阶且 CPU 更近;归因=金标准 fpn 生成代际(桌面输入准备/ORT 版本)与设备执行的混合差,最终 mask 质量不受影响(上条已证)。
- 工件:设备 `/data/local/tmp/sam3cpu/`(三件量化 onnx + ortfix)、`bench/cpuvis_*`/`cpudec_*` dumps;WSL bench/ 同步;ortbench v2.2(新增 `--infile idx:path` 按 dtype 装载 + `--dump prefix` 输出落盘 + Bool/Uint8 填充)。

**X90 CPU 路线剩余优化空间盘点(2026-10-10,线程扫描补全)**:

- **线程曲线定案(量化件,8t 已是全局最优)**:vision 4t=13.08s / **8t=8.28s** / 12t=9.22s / 16t=9.08s / 20t(ORT 默认)=9.28s;decoder **8t=1.32s** / 12t=1.44s / 16t=1.48s / 20t=1.59s——8 线程即饱和,超过纯加争抢;**线程杠杆已穷尽**,`intra_threads=8` 定案(核心亲和性 pinning 未测,期望 ≤5-10%)。
- **剩余杠杆按性价比排序**:
  1. **自建完整 ORT(首推)**:现役 .so 为 csukuangfj v1.26 削减版(fp16/bf16 MLAS 全删,"纯 fp32 NEON,比同硬件 Android ORT 慢")——int8 核虽存但版本旧;以新 NDK clang 自建完整 MLAS 或升 arm64-OHOS 1.27.0 现货 → **估 10-30%**,工作量中,数值风险低(同引擎族);
  2. **动态→静态量化(QDQ)**:HF 件为动态量化(每个 MatMulInteger 前有运行时 DynamicQuantizeLinear 算激活 scale)——静态化(校准集)消灭运行时量化算子并解锁融合 int8 核 → **估 1.2-1.5×**,须 6000×4000 基准 IoU 复验精度;
  3. decoder 裁 6× 多路复用冗余:~0.3-0.6s(段本身仅 1.32s,收益封顶);
  4. GPU 预处理(wgpu resize)/IOBinding:~0.1s 级,应用集成期顺手做;
  5. **分辨率手术(1008→756)**:计算量 -2.25×,但需上游重导出 + 质量未验——受阻于上游;
  6. fp16 混合精度:受阻于杠杆 1(现役构建无 fp16 核,恢复后再评估)。
- **汇总**:现实可达(杠杆 1+2)全链 9.6 → **~6-7s(再 1.4-1.6×)**;激进(+杠杆 5)~3-4s 但质量/重导出为前置条件。**架构红利已内置**:多提示词场景 encoder 8.28s 每图仅付一次(缓存 FPN 特征),后续每提示词仅 decoder ~1.4s——交互式抠图的体感延迟是 **1.4s/词** 而非 9.6s。
- 其他模型无优化压力:u2net/nind/clip 已在 50-150ms 交互级(NPU 路线)。

**与 .ms/NNRT/ORT-CPU 三路线对比(2026-10-10 参照系修正后终版)**:①CNN 类(u2net/nind)`.om`@perf4 为王(50/130ms,胜 `.ms` 1.3-2×、胜同芯片 CPU 9-12×);②CLIP:`.ms` 150ms(胜 CPU 3×;`.om` 阻塞于 NPUCL Cast 洞);③**SAM3 三件与 LaMa:X90 CPU(ORT,HF 量化原件,8t)全面获胜**(SAM3 全链真图实测 **9.6s vs NPU 60s = 6.2×**,IoU 0.953/0.982 不劣反优;lama 7.3s vs 10.2s)——量化原件零手术直用,`.om` 降级为能力证明。**路线策略终版:u2net/nind → `.om`+perfmode 4;CLIP → `.ms`;SAM3/LaMa → ORT CPU(量化原件,`intra_threads=8`);NPU 路线保留为未来固件演进的对标基线**。

- **应用集成落地(2026-10-10)**:u2net + nind 两条 `.om` 路线已接入应用本体——`ohos_integration.rs` 新增 `om` 模块(HIAI_MR_* dlopen 直连 + perfmode 4 + ION 缓冲配方,完整复刻本节 harness 生命周期),`ai_processing.rs` 增 `DenoiseSession::Cann`/`MaskSession` 双枚举 + `.om`→`.ms`/onnx→ORT 三级回退路由;`.om` 侧载进 app models 目录即生效(`u2net.om` sha 与 app 模型严格一致、`nind_v2.om` 单入单出 [1,3,504,504] NCHW——此前笔记误记为 lama 式双输入,真机 restore-only dump 已订正)。三道门禁(fmt/clippy/aarch64-ohos check)全过;**真机端到端已全通(2026-10-11,8 轮装机迭代——详见本节末"应用进程内端到端验通"条与 `HARMONYOS_PORTING.md` §6.16)**。

**设备算子兼容约束清单**(真机 CheckOpSupported/CheckSupport/SelectKernel 实证,供后续模型体检):

- **BatchMatMul**:x2 必须 2–4D(5D 折叠 batch 即过)
- **GatherV2D**:data 限 fp32/fp16/int32(uint8 拒);**int64 常量标量索引可过**(lama 72× 实证)
- **ExpandDims(CPUCL)**:限 FLOAT/INT32/UINT8/BOOL(int64 拒,clip 79 处)
- **ROIAlignV2**:固件类型级不支持(微模型定案)——90 节点原语分解可绕过(手术 6,sam3dec 已全通);**GridSample(常量网格)实际可跑**(CPUCL 接管执行,onnxsim 不折叠 GridSample 但设备端无恙)
- **GatherV2D 负索引**:运行期不支持——常量索引 `[-1]`(PyTorch 末位选择)通过 check/Init,执行期报 `run error`;修复=按静态维度换算正值([-1]→[5]);结构性负轴(Squeeze/Slice/Concat/ReduceSum 的 axis=-1/-2)omg 自动归一化,无恙
- **计算精度**:omg 无 precision_mode/allow_fp32/force_fp16 类旗标(omg+全部工具库 strings 实证)——中间张量按 **fp16 有效精度**执行(RoiAlign 分解实测 vs CPU fp32 参考:mean 1.7e-03 / max 3.3e-02);`--weight_data_type=FP32` 只保权重,不保中间张量
- **负 Pad(crop 语义)**:omg 宿主 IR 推断即拒(`ir_infer_shape_optimizer: failed to infer ge_default graph shape`)——与 MS converter 的 PadFusion 同款死法;AI_DENOISE §5.6 的 Pad→Slice 手术件(nind_sliced,哈希钉死)直接复用即过
- **NPUCL Cast 有 kernel 注册洞**:int32→int64(kernelId 589827)未注册——设备缺 `libai_npucore_ascendc.so` 插件(DDK 两份插件均为 x86-64 宿主,无 aarch64 可推);int64 密集图落 NPUCL 子图必踩
- 兼容检查**在首个 fatal 即 abort**(整份 hilog 仅一条 `don't support`)——修一个才见下一个,须按 hilog 迭代
- npucl elementary_lib 不含 Transpose/Reshape/Squeeze/StridedSlice/MatMul 等仅是**警告级**(CPUCL 兜底);且检查通过后**编译阶段仍可再抛 kernel 未注册**(clip 实证)——restore 失败的 hilog 取证配方:`hilog -r` → 跑 → 立即 `hilog -x | grep -E 'C0215[0-9]|C0216[0-9]' | grep -v -E 'DeviceOpen|DeviceClose'`(缓冲区被系统噪声快速冲掉)

**HIAI_MR_\* 执行配方**(签名反汇编定出、真机跑通;harness:`%TEMP%\opencode\cannkit\hiai_mr_test5.c` **v5**→`hiai_mr_test6.c` v6(+`--backend N` 戳)→`hiai_mr_test7.c` **v7**(+`--perfmode/--npucore/--bandmode N` 合法性能旋钮,§8.4 优化块),NDK clang 链 core+tensor+hcl_model_runtime 三库 + `-Wl,--allow-shlib-undefined`(PS 传参该旗标须整体引号包裹,裸逗号被 PS 解析器拆坏;**hdc shell 命令内带管道符的 grep 模式串会被引号剥离拆坏——一律裸跑或 PS 侧过滤**),设备 `/data/local/tmp/hiai_mr_test7 <model.om> [--iters N] [--restore-only] [--fill F] [--infile idx:path]... [--dump prefix] [--perfmode N] [--npucore N] [--bandmode N]`——v5 输入文件喂数/缓冲落盘为数值对齐基建):

- `HIAI_MR_BuiltModel_RestoreFromFile(path)→HIAI_BuiltModel*`;IO 查询 `GetInputTensorNum/GetInputTensorDesc/GetInputTensorName`(Output 同);`ModelInitOptions_Create`;`ModelManager_Init(mgr,opts,model,NULL)`;`ModelManager_Run(mgr,in**,nIn,out**,nOut)`;`ModelManager_Deinit(mgr)`
- **Run 的输入输出必须是 ION/DMA 缓冲区**:普通 tensor.so 的 `HIAI_NDTensorBuffer_CreateFromNDTensorDesc` 造宿主内存(fd=-1),Run 内部 `GetFd_V2→IONTensorBuffer::GetFd` 必崩;须走 facade `libai_fmk_hcl_model_runtime.so` 的 `HIAI_MR_NDTensorBuffer_CreateFromNDTensorDesc`(产真 ION fd 25–39)
- teardown 顺序(实证干净退出):逐缓冲 `MR_Destroy(**)` → `Deinit(mgr)` → `ModelManager_Destroy` → `ModelInitOptions_Destroy(**)` → `BuiltModel_Destroy`;**MR_Destroy 与 Options_Destroy 均双指针**(单指针即崩,三份 cppcrash 栈实证)
- `HIAI_IsSupportNpu()=-1`(文件探测式,非阻塞——返回负值照样能跑)

**遗留**:① 全部结果为合成填充输入的能力/计时冒烟,逐像素精度未验(输出分布合理性检查通过);② sam3img 63.9–126.3s 方差未归因(热/竞争),实用性存疑;③ ~~应用集成路径未定~~ **已落地并真机验通**(2026-10-11:HIAI_MR_* Rust 绑定随 `om` 模块入库;应用进程内厂商库加载经 38 库 HAP 打包闭包解决——8 轮装机迭代定版,详见 `HARMONYOS_PORTING.md` §6.16);④ clip 若救:须全图 int64→int32 化绕开 NPUCL Cast 洞;sam3dec 已翻案全通且**张量级误差分解收口**(纯执行误差 relL2≈1e-4,top-5 查询与 fp32 参照完全一致,见 §8.4);⑤ 战役工件:WSL `~/cannkit/`(fixed/*.onnx 手术产物含 sam3dec_ortfix.onnx、out/*.om+build 日志、fix3/fix4/analyze_ops/inspect/rl_surgery/rl_validate/roi_build/fix_negidx/rl_dev_prep/rl_dev_compare/rl_speed_cpu/rl_cpu_verify/ortfix_dec/err_decomp 系列脚本、bench/ 全链路基准件与落盘数据、dumps/ 设备落盘数据),宿主侧 `%TEMP%\opencode\cannkit\`(harness v5/v6/v7 源码与二进制、rl_cpu_test 设备 CPU 基准件、hilog 取证、崩溃栈、dumps/);⑥ ~~速度优化收尾缺口~~ **已收口**(2026-10-10 设备恢复后 verify_perf4 一键链 4/4 全 PASS,详见 §8.4 优化块"数值影响(已收口)"条):perf4 确定性(跨重启逐位一致)+差异定量(relL2~1e-4,fp16 噪声档)+全链 parity 1.0000+全分辨率 IoU 0.978+终测计时(冷机 enc 51.9s/dec 8.02s,端到端 1.28×)全部落定;掉线事件诊断存档(重启后仅 RNDIS 枚举、hdc 接口未暴露、1142 端口扫描仅 53/DNS 开——网络控制通道排除,根因=USB 模式回落,解锁/重插后恢复,/data 无损)。残余(非阻塞):encoder npucore 无干净数据(mini/decoder 双阴性,期望低);dopt 待 py≤3.11 环境评估。

**应用进程内端到端验通(2026-10-11,8 轮装机迭代;设备 3XB0124617000013)**:u2net 前景蒙版在 RapidRAW 应用进程内全链路跑通——38 库 HAP 打包闭包(应用目录 `moduleNs_default` + load_api 绝对路径把 indirect/ tensor+core 与 vendor 计算闭包拉入 default ns 的双通道;命名空间四条铁律与库清单见 `HARMONYOS_PORTING.md` §6.16)下,HCL 模型加载 → HDI ION buffer 分配 → NPU 推理 → 全分辨率蒙版 PNG 落盘,触发到 `.rrdata` 保存 **4.85s**(含 0.76s 模型上板)。NPU 执行三重实证:应用进程内 NPUCL `SetVote "extreme high"`、hiaiserver 为应用 pid 提升 NPU 时钟(SetNPUPerf:400/SetDDRPerf:3)、CPUCL 执行 CPU:1 分区节点(208us);反向证据排除 ORT 回退(125s hilog 捕获无 `timeout free resource`、无 runtime Finalize/teardown、病毒扫描无 `u2net.onnx` 读取)。蒙版内容 sanity:9202×12122 全分辨率 8-bit 灰度 PNG、deflate 41.2:1(结构化)。本节 harness 数据(u2net `.om` perfmode4 ~50ms)至此在应用形态兑现;同期 ORT CPU 回退路径获 6 轮真机完整验证(T1-T7 每轮优雅回退、正常出蒙版、无崩溃)。

---

## 9. 风险与未验证项

**研究会话明确未能验证的 8 项**:

1. 历史 "--noFusion=true required for NPU conversion" 论断(无现存官方页面;现代等价物 `--optimize=none`/`set_no_fusion()`)
2. ~~SAM2/SAM、Depth-Anything、CLIP、LaMa、U2Net 的一手转换报告(成功或失败均无)~~ **已验证**(2026-10-09 真机实测,见 §8.3)
3. `OH_AI_DEVICETYPE_KIRIN_NPU` 在 NEXT 上是否可用(枚举存在,指南从未在 NEXT 使用)
4. NNRt(`OH_NN*`)native API 的精确 since API 级别(Kit=API 10 已验证)
5. standalone 交叉编译 OHOS 包是否附带 C API 头文件集
6. `ohos-sys` crate 是否暴露任何 MindSpore 符号
7. NNRt 56 算子的具体清单(算子文档页已迁移 404;仅官方"56 common operators"数字;研究者推断其大致为 conv/pool/matmul/softmax 类、无 LayerNorm/attention——[inferred] 未验证)
8. Kirin 9000s/9010/9020(NEXT 时代芯片)在旧 HiAI delegate 中的支持(FAQ 列表早于它们)

**项目侧风险**:

- ~~**真机依赖是硬阻塞**~~ **已解除**:真机 2026-10-05 到位(MateBook Pro/Kirin X90),2026-10-07 完成 NIND 全链路 POC(§8.2)
- 模型生态单点:`.ms` 转换质量随 MS Lite 版本变化(2.x 有移除先例);**实测新增硬约束:converter 版本必须 ≤ 设备引擎版本**(本机 Kit 引擎 2.7.0 → 2.10 .ms 运行时失败,§8.2),转换管线须按目标设备引擎钉版本 + 升级复测
- fp16 语义差:转换器 `--fp16` 与 context fp16 是两回事(§4.2);LaMa 的 fp16 已实测——关卡是 `Clip` 属性解析而非 `--fp16` 开关,fp32 重写后可转但 Kit 2.7.0 runtime 不可用(§8.3)
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
