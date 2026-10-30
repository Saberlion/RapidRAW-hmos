# RapidRAW AI 降噪硬件加速调研报告

> 调研日期:2026-10-07 · 分支:`feature/harmonyos-port` · 服务对象:AI 降噪(NIND UTNet)全平台加速决策
> 证据标注:`[official-doc]` 官方文档 / `[source-code]` 源码 / `[release-page]` 发布页 / `[community]` 社区 / `[inferred]` 推断 / **[未验证]** 研究未能确认 · Phase 3 桌面验证与 OHOS 真机 POC(.ms+NNRT)均已完成:结果见 §4.4/§5.3/§5.5
> 外部结论锚定版本:ONNX Runtime 1.22.0(桌面,build.rs 拉取)/ `ort` crate =2.0.0-rc.10 / MindSpore Lite 2.10.0(2.11.0 已于 2026-10-05 发布)/ NIND UTNet `generator_684` 检查点
> 姊妹文档:`docs/MINDSPORE_LITE_NPU_EVAL.md`(OHOS NPU 路线前置调研,本文承接不重复)

---

## 1. 背景与结论速览

**背景**:本仓 AI 降噪(NIND UTNet,滑窗 504×504 tile,ORT **CPU EP**)在全平台都很慢——桌面 24MP 约 18~22s(高画质档)、50MP 35~42s;OHOS 因社区 ORT 构建剥离 fp16/bf16 NEON 内核而进一步降级。上游 issue #997 明确"AI-based Denoising is not GPU accelerated"。

**结论速览**:

1. **桌面/Android 最大杠杆 = ORT 执行提供器(EP)注册**,且工作量可控:上游作者顾虑的"打包 GPU ORT 极其复杂"(issue #576)实际已被其自己的 HF 仓库化解——`onnxruntimes-v1.22.0/` 目录下**已备好 DML 风味 dll**(`onnxruntime-windows-x86_64-dml.dll` + `directml-windows-x86_64.dll`),build.rs 换 URL+sha256、Session 注册 DirectML EP、打包 DirectML.dll 即是全部改动。预期加速:独显 3~6×、Apple NE ~10×;社区补丁版对同款模型的实测为 **10~20×**(RTX 4060 Ti vs CPU ~200s)。⚠️ Phase 3 本机实测(老旧 iGPU UHD 630)DML 反而慢 2×(§4.4)——**DML 收益硬件相关,须按 GPU 能力门控/用户可切换**(对齐现有"处理后端/优先独显"设置)。
2. **当前桌面 dll 确证为 CPU-only**(Phase 3 运行时 `is_available(DirectML/CUDA) = false`;-dml 变体 `is_available(DML) = true`,比 CPU 版大 4MB 即 DML EP 代码,两 dll 同版本号 1.22.20250508.2)——**启用 DML 必须换 dll**;姊妹文档"实测含 CUDA/NNAPI/DML/OpenVINO"表述系字符串级误读,应订正。
3. **OHOS 主路线 = MindSpore Lite `.ms` + NNRT,真机 POC 已全链路验证(2026-10-07,MateBook Pro/Kirin X90,§5.5)**:Maleoon NPU **257ms/tile vs 应用现状 ORT CPU 1215ms/tile = 4.72×**(24MP BALANCED 估算 ~30s vs ~142s),NNRT 输出 fp16 级偏差(max_abs 1.6e-03,降噪可接受);csukuangfj ORT 构建降级事实(§2.3)坐实"OHOS 上 ORT CPU 是下限而非基线"。集成要点:负 Pad 图手术、converter 版本 ≤ 设备引擎(本机 2.7.0)、输入 NHWC/输出 NCHW、输出句柄跨 Predict 复用(§5.5)。
4. **新发现第三条 OHOS NPU 路线**:CANN Ascend C 已支持 Kirin 9030/X90(Mate 80/90 系列),经 ATC→`.omc`→HiAI Foundation→NNRt "HIAI_F" 设备——推翻姊妹文档"CANN 不适用"的结论,但**不覆盖 Mate 60/70(9000s/9010/9020)**,定位为远期备选(§6)。
5. **管线侧存在与 EP 正交的免费收益**(§9):tile 批维打包、IOBinding、移除跨 `run()` 的 `Mutex<Session>`、取消支持、降噪结果直接回流 GPU(当前要经磁盘往返)。
6. **模型侧**:保持 fp32(fp16 有同族溢出实证——darktable-ai UtNet2 Bayer 变体 FP16 出 NaN 被钉死 CPU);INT8 低杠杆;**NAFNet 不适合本仓**(SIDD 智能手机噪声训练,DSLR 真实噪声上每 ISO 垫底);SCUNet real-psnr 是最强替补但 17.9M 参数。
7. **用户预期预算**(竞品校准,§10):桌面 GPU 加速后 <10s/24MP 进入"快"档(Adobe 口径专业笔电 15~30s、消费级 30~90s);OHOS NPU 目标 ≤30s/24MP(可接受档)。

---

## 2. 本仓现状盘点(AI 降噪栈)

### 2.1 降噪管线解剖 `[source-code]`

调用链(`denoising.rs:52` → `ai_processing.rs:989`):`apply_denoising` → `tokio::spawn_blocking` → `denoise_image`(全分辨率 Rgb32F)→ `run_ai_denoise` → `run_native_denoise`(**串行 tile 循环**,`ai_processing.rs:910`)→ accumulator → `DynamicImage::ImageRgb32F`。

| 维度 | 事实 | 位置 |
|---|---|---|
| 会话创建 | `ort::init().with_name("AI-Denoise").commit()` + `Session::builder()?.commit_from_file()` —— **零 EP/零线程/零优化配置**;上游 `4ba3e25` 提交同样零 EP(与 fork 逐字符一致) | `ai_processing.rs:612-614`;上游 commit 4ba3e25 |
| tile 参数 | TILE_FASTER 504/504/0 · BALANCED 504/480/6 · HQ 504/448/12(`cs/ucs/overlap`);"684"=PyTorch 训练迭代数(`generator_684.pt`),非 tile 维度 | `ai_processing.rs:787-800` |
| tile 数 | 24MP:96/117/140;50MP:192/221/266(三档) | 由网格公式推得 |
| 串行化点 | ① `for i in 0..total` 串行循环;② `Mutex<Session>` **跨整个 `sess.run()` 持锁**(ort `Session` 本身 `Sync`,该锁是过度保守);③ 无 batch 维(每 tile `[1,3,504,504]` 单独 run);④ 提取/推理/累加三阶段无流水 | `ai_processing.rs:910-972` |
| 每 tile 开销 | ~9MB 瞬态分配(输入 Array4 零初始化 3MB + `to_owned` 3MB + 输出 `to_owned` 3MB),`get_pixel` 散射读取;非推理开销 ~5-10ms/tile,**推理占墙钟 ≥90%** | `ai_processing.rs:813-931` |
| 大缓冲 | 24MP:源 Rgb32F + accumulator + 输出 ≈ **864MB**;50MP ≈ 1.8GB | `ai_processing.rs:999-1011` |
| 取消 | **无**(无 AtomicBool/JoinHandle 保留;关闭 modal 后台任务继续跑完)——`ai_commands.rs` 的 `AiTaskGuard` 模式未覆盖降噪 | `denoising.rs:51-88` |
| 结果流向 | `state.denoise_result` **仅**被 `save_denoised_image` 消费(写盘);**不回流 GPU**——编辑降噪结果需"存盘→重新加载→to_rgba_f16 上传"完整往返 | `image_loader.rs:906-909` |
| 桌面估算 | CPU ~80-300ms/tile(默认 intra-op 线程,未调)→ 24MP BALANCED ~18.5s / HQ ~22s;50MP ~35-42s | `[inferred]` 代码推算 |
| 插桩 | 无任何计时/日志;进度每 10 tile 发字符串 | `ai_processing.rs:918-921` |

### 2.2 ORT 集成与各平台运行时 `[source-code]`

- `ort = { version = "=2.0.0-rc.10", features = ["ndarray", "load-dynamic"] }`(`Cargo.toml:38`)——**注意:未禁用 default features**(`download-binaries`/`copy-dylibs` 仍开启,构建时静默从 cdn.pyke.io 拉取 pyke 预编译并拷贝 dylib 到 target/,包括 DirectML.dll——pyke #470;运行时实际加载的仍是 resources/ 下 HF dll,pyke 产物为死重)。
- 桌面 ORT **v1.22.0**:build.rs 从 `huggingface.co/CyberTimon/RapidRAW-Models/resolve/main/onnxruntimes-v1.22.0/` 按平台下载 sha256 校验,落 `src-tauri/resources/`,经 `tauri.conf.json:41` resources 打包;运行时 `ORT_DYLIB_PATH` 指向资源绝对路径(`lib.rs:1986-2006`)。
- Android:v1.22.0 android-arm64 → jniLibs;仅 aarch64。**无 NNAPI EP 注册**(上游 #1090 PR 未合并)。
- OHOS:**不下载**,各机自备 `libs/ohos/<abi>/libonnxruntime.so`(x86_64 模拟器 v1.28.2 / aarch64 真机 v1.26.0,csukuangfj onnxruntime-libs "ohos" 系;§2 风险登记表"停在 1.16.3"已过时);`ORT_DYLIB_PATH="libonnxruntime.so"`;HAP 打包后经 `build-ohos.ps1:129-180` zip 注入 + libc++_shared.so + 重签名。
- 全仓 grep `with_execution_provider|DirectML|CUDA|XNNPACK|NNAPI|CoreML|OpenVINO|NNRt` = **0 处**(vendor/ash 的 Vulkan 扩展名除外)。4 个 Session 创建点(`ai_processing.rs:543-547/614/683/746`)。

### 2.3 OHOS CPU 路线的降级事实 `[source-code]`

csukuangfj/onnxruntime-libs 的 `harmony-os-shared.yaml`:所有非 CPU EP 显式 OFF,**且因 OHOS clang 15.0.4 不支持 fp16/bf16 NEON 内在函数,sed 删除全部 fp16/bf16 MLAS 内核源文件**(halfgemm/sbconv/softmax/eltwise/gelu/pooling 等 ~15 个)+ `MlasBf16AccelerationSupported` 强制 false → **纯 fp32 NEON,比同硬件 Android ORT 慢**。工具链 5.0.3.906 限 ORT≤1.16.3,6.0.1.251 可至 1.26.0。v1.30.0 的 OHOS workflow **运行失败**,HF 镜像实际 OHOS 资产:arm64-v8a 仅 1.27.0、armeabi-v7a/x86_64 至 1.28.2。

**含义**:OHOS 上"留在 ORT CPU"不是保守选项而是降级选项——这加强了向 `.ms`+NNRT 迁移的紧迫性(姊妹文档路线)。

### 2.4 GPU 侧可复用资产 `[source-code]`

- 整个调整管线已是 **wgpu compute**(非 render pass);`blur.wgsl`(可分离高斯,workgroup 256×1)是最干净的计算着色器模板;OHOS 后端 `VULKAN|GL`,启动预热吸收 ~11s WGSL 编译。
- 上传 `to_rgba_f16` / 回读 `read_texture_data_roi`(同步,无 buffer 池);analytics 的**异步回读线程模式**(`gpu_processing.rs:2092-2226`)是 AI 回读可抄的模板。
- ORT 推理(spawn_blocking + session.lock)**不持有 GPU 锁** → 今天就能与渲染并发;wgpu 同队列提交天然串行(计算排在渲染后,填空闲)。
- `GpuContext` 全 Arc 可克隆;`denoise_result` 直接变基座纹理 = 清 GpuImageCache + 内存重上传,跳过磁盘(`[inferred]` 现有代码无此路径,新增工作量小)。

---

## 3. 方案总览矩阵

| 方案 | 平台 | 预期加速 | 工作量 | 风险 | 证据强度 |
|---|---|---|---|---|---|
| **A. ORT EP 注册**(DirectML/CUDA/CoreML/NNAPI) | 桌面/Android | 独显 3-6×,NE ~10×;补丁版实测 10-20×;老旧 iGPU 反例(慢 2×,§4.4) | 小(换 dll+注册+打包;§4) | dll 已确证 CPU-only,须换 -dml;DML 维护模式 | 高(多源实测) |
| **B. MindSpore Lite .ms + NNRT** | OHOS | **实测 4.72×**(Kirin X90,257 vs 1215ms/tile,§5.5) | 中(POC 全套已验证;集成 = msbench 绑定入库 + 转换管线固化) | 负 Pad 须图手术;converter ≤ 引擎版本 | **真机实测** |
| **C. CANN ATC→.omc→HiAI** | OHOS(Kirin 9030+ 机型) | ~10-15×(Ascend 310 数据) | 中高(ATC 工具链+libhiai) | 仅 Mate 80/90 芯片;华为工具链绑定 | 中(官方仓库实证) |
| **D. 替代引擎**(ncnn/burn) | 全平台 | 同 GPU 量级 | 高(新 FFI+模型转换) | 双栈维护 | 中(ncnn OHOS+Vulkan 官方文档) |
| **E. 模型侧**(fp16/INT8/换模型) | 全平台 | fp16 GPU +~1.5-2×(仅 CUDA/OV);INT8 CPU 1.5-2× | 小-中 | fp16 溢出实证;INT8 质量敏感 | 中-高 |
| **F. 管线侧**(batch/IOBinding/锁/取消) | 全平台 | tile 批化+IOB 合计 ~1.3-2×(EP 之上) | 小-中 | batch 维静态=1,批化需重导出模型(§4.4) | 高(ORT 官方点名) |

**推荐组合**:桌面 = A+F;OHOS = B(+F)+ 远期 C;全平台可选 E(fp32 起步)。

---

## 4. 方案 A:桌面/Android ORT 执行提供器

### 4.1 EP 选择 `[official-doc]`

| 平台 | 首选 EP | 理由与注意 |
|---|---|---|
| Windows | **DirectML** | 任意 DX12 GPU(含 iGPU——但 Phase 3 实测老旧 iGPU 反慢 2×,须门控,§4.4);无 CUDA 依赖;pyke rc.10 有 `directml` feature;**fp16 对 conv U-Net 无益甚至慢 5-10%**(#23340,RTX2080 Conv3D U-Net)→ 保持 fp32 |
| Windows(N 卡) | CUDA(可选叠加 TensorRT) | CUDA fp16 ~2×;TRT 对 UNet 再 +25-31%(arXiv 2607.11356)但 per-GPU engine+重依赖 → v1 不做 |
| macOS | **CoreML** | `ComputeUnits` 默认 ALL;**必须 `ModelFormat=MLProgram`**(默认 NeuralNetwork 会在 GPU 静默转 fp16;且 CoreML EP 不支持 fp16 输入,否则节点回退 CPU 慢 8×,#17033/#17448) |
| Android | NNAPI | 上游 #1090 PR(waxm134)已验证方向;`.so` 大概率含 NNAPI EP(待 `is_available` 确认) |

EP 注册顺序即优先级;失败静默回退 CPU,用 `.error_on_failure()` 或 `is_available()`+`register()` 双层检测。

### 4.2 dll 现实与打包

- Microsoft 发布**互斥风味包**:GitHub release(CPU-only)/ GPU(CUDA+TRT 伴生 dll)/ DirectML(NuGet-only since 1.20.x)。NNAPI 为 Android 专属。`[official-doc]`
- 本仓桌面 dll(v1.22.0,12,418,080 字节)字符串扫描含全部 EP 名(含 SNPE/VSINPU 等 Android/Web 专属)——**这些是加载器注册表字符串,任何标准构建都有,不构成"编译入"证据**;CUDA/TRT/OpenVINO 等带显式 "Failed to load shared library" 插件存根。**Phase 3 运行时已定论:`is_available(DirectML/CUDA) = false`,dll 为 CPU-only**(§4.4 CLAIM A)。
- **CyberTimon HF 仓库已备 DML 风味**:`onnxruntime-windows-x86_64-dml.dll` / `onnxruntime-windows-aarch64-dml.dll` / `directml-windows-{x86_64,aarch64}.dll`(onnxruntimes-v1.22.0/ 下;R1 经 HF API 确认,仓库 sha daec18e7)。
- 打包规则:`DirectML.dll` 必须**置于 exe 旁**(pyke #94:不会被 ORT_LIB_LOCATION 拾取);Tauri 用 bundle resources,MSI bundler 自动收集 out-dir `*.dll`(tauri commit f66bc3c);注意当前 ort 依赖未禁 default features 会在构建时额外拉 pyke 产物(含 DirectML.dll)——**顺手在 Cargo.toml 加 `default-features = false, features = ["std","ndarray","load-dynamic","directml"]`** 可同时消除死重与 #470 包体膨胀。
- Android NNAPI:nnapi feature + 平台条件注册即可,无额外伴生文件。

### 4.3 DirectML 工程细节 `[official-doc]`

- 会话选项:`with_memory_pattern(false)` + 顺序执行(DML 不支持 mem pattern/并行执行,否则报错;Phase 3 实测 rc.10+ORT 1.22 未设此项会话也能正常创建,§4.4——保守起见落地时仍建议设置);
- `device_id 0` = 主显(笔电常为 iGPU;`with_device_id(1)` 选独显,与现有"prioritize dGPU"设置对齐);
- 首跑 shader 编译卡顿:**固定 504×504 tile 形状使其成为一次性成本**并被缓存——现有 tile 方案天然契合;
- fp16 正确性雷(BatchNorm/InstanceNorm iGPU 垃圾输出,DirectML #362/#486)→ 另一个保持 fp32 的理由;
- **DML 处于维护模式**(Microsoft Learn:投资转向 WebGPU EP 与 IHV EP)——可接受:当下仍是最务实的 Windows 全 GPU 选项,且我们锁定 ORT 1.22 版本面;
- ORT 官方文档点名 "tile based inference for high resolution images" 为 IOBinding/设备张量的目标用例(与方案 F 联动)。

### 4.4 本机可执行验证(Phase 3,已完成)`[source-code]`

独立 crate(`ort =2.0.0-rc.10, default-features=false, [std,ndarray,load-dynamic,directml]`,temp 目录,未触仓库;**双进程设计**——ort 会在 init 时缓存 dylib,故 cpu/dml 两模式各自设 `ORT_DYLIB_PATH` 独立进程运行)。测试机:Intel UHD Graphics 630 iGPU(1GB VRAM,driver 31.0.101.2115)。

**CLAIM A — CONFIRMED:仓库桌面 dll 为 CPU-only**
- 仓库 dll(v1.22.0,12,418,080 字节):`DirectML is_available() = false`、`CUDA is_available() = false`;
- -dml dll(16,471,584 字节,+4MB = DML EP 代码):`DirectML is_available() = true`;
- 两 dll 文件版本号相同(1.22.20250508.2.f217402)——同源 ORT 1.22.0,差异仅 DML 编译入;
- **结论:桌面启用 DML 必须换用 -dml 风味 dll**;`is_available()`(GetAvailableProviders)是比字符串扫描权威的判据。

**CLAIM B — 本机 REFUTED(DML 慢 2×),但 EP 链路本身 CONFIRMED 可用**
- DML 会话:device_id=0 一次成功(**无需 `with_memory_pattern(false)`**——rc.10+ORT 1.22 下未触发该约束);warmup 5 轮充分吸收 shader 编译(实测 iter1 与 iter20 耗时无差异);
- 计时(504²×3 fp32 tile,仅计 `session.run()`):**CPU 均值 1162ms/tile(10 次,1139~1185)vs DML 均值 2331ms/tile(20 次,2322~2337)→ 0.50×,慢 2.0×**;
- 解读:UHD 630 为 2017 年代 Gen9.5 iGPU(1GB VRAM),fp32 conv U-Net 算力不足 + 共享内存架构传输开销 → **该结果仅代表老旧 iGPU,不推翻独显 3-6× 的多源证据与 #576 补丁版 10-20× 实测**;工程含义 = DML 激活须门控(默认仅独显/用户可切换);
- 本机 CPU 基线 1162ms/tile × 117 tile(24MP BALANCED)≈ **136s/张**——与 #576 用户报告 ~200s/24MP 同量级,坐实"分钟级"体感。附带:模型加载 185ms。

**CLAIM C — CONFIRMED:输入 (1,3,504,504) Float32,全静态维**
- SymbolicDimensions 全为空串——**batch 维静态 = 1**;
- 含义:①固定 504 tile 与模型静态形状严格匹配(DML shader 缓存友好的判断成立);②**§9.1 tile 批维打包需先重导出动态 batch 模型**,无重导出时的替代 = 会话池并行(§9.3)。

验证产物:`C:\Users\fxre\AppData\Local\Temp\opencode\dml-verify\`(Cargo.toml + main.rs + 下载物),完整原始输出见会话日志 `.omo/ultraresearch/20261007-112629/verify-dml.md`。

### 4.5 ort crate 版本策略 `[source-code]`

- **留 rc.10 即可启用 DML**(EP feature 为纯 API 门控,rc.10 的 `DirectMLExecutionProvider` 可用);
- **若升级:直接 rc.13,跳过 rc.12**(rc.12 有 DML 崩溃 bug——ORT 1.24 `GetExecutionProviderApi("DML")` 返回零填充 OrtDmlApi→空指针,PR#551 于 rc.13 修复);
- rc.11+ 连锁:`ndarray` 0.16→0.17、`execution_providers`→`ep` 模块、MSRV 1.88(本仓 1.98 满足)、rc.13 起 EP **编译期门控**(需 `#[cfg(feature)]` 守卫)+ 仅 CUDA 13;
- OHOS `.so` 供给与 crate 版本解耦(load-dynamic 无强校验,C-ABI 向后兼容),但 csukuangfj OHOS arm64 仅到 1.27.0——升级 crate 到 rc.13(ORT 1.28)后 OHOS 侧需自建或沿用旧 `.so` 验证。

---

## 5. 方案 B:OHOS MindSpore Lite `.ms` + NNRT(主路线,承接姊妹文档)

姊妹文档(`MINDSPORE_LITE_NPU_EVAL.md`)已完成该路线的完整调研(系统 Kit `libmindspore_lite_ndk.z.so` + `OH_AI_*` C API、NNRT 加速器 + CPU 逐算子回退、converter_lite 2.10 离线转换、逐模型风险矩阵)。本文只补**新证据与修正**:

### 5.1 承接条件已就绪 `[source-code]`(HARMONYOS_PORTING.md)

- **真机已到位**(2026-10-05,HAD-W32 = **HUAWEI MateBook Pro**,Kirin X90 + Maleoon,HarmonyOS PC,API 26——早期"平板/916"表述系误记)——姊妹文档 §8 的硬阻塞解除;**2026-10-07 POC 已完成(§5.5)**;
- §6.12 已在模拟器打通 NIND 单模型全链路(124MB 模型→FUSE 持久化→ORT dlopen→tile 推理→前后对比 UI→保存),`.ms` 路线可完全复用该管线形态;
- §6.13 真机 Vulkan 渲染全链路已通(交互预览 286-353ms/趟)——真机可用性不再存疑。

### 5.2 NNRt NPU 加速比证据链(新增) `[community]`

| 来源 | 芯片/设备 | 模型 | CPU→NPU |
|---|---|---|---|
| Paddle Lite 官方基准 | Kirin 810 / 990(Android HiAI) | mobilenet_v1 | 6.5× / 10.6× |
| 同上 | 同上 | resnet50 | **12.4× / 24.4×** |
| 同上 | 同上 | ssd_mobilenet | 2.7× / 3.2× |
| 华为云博客 | Kirin 9000S(Da Vinci 2.0,>20 TOPS INT8) | INT8 MobileNetV2 | 10×+(1-2ms vs 30-50ms) |
| 51CTO 实测 | Mate 60 Pro(HarmonyOS ML Kit) | MobileNetV2 | 3.7×(0.33s vs 1.21s;GPU 0.65s) |
| CANN recipes | Ascend 310(鸿蒙手表) | MobileNetV3-S/ResNet-18 INT8 | 15×(8-12ms vs 120-180ms) |

NIND UTNet 为纯 conv U-Net(算子面见 §8.1),与 resnet/mobilenet 同类 → **8-15× 为合理预期** `[inferred]`。注意:ssd 仅 2.7-3.2× 提示**含检测头/异常分支的模型回退多**——NIND 无此问题。

### 5.3 NNRt 算子清单(已关闭)`[source-code]`

权威源 = HDI IDL:`drivers_interface/nnrt/v2_1/NnrtTypes.idl` 的 `NodeType` 枚举(**84 个算子**,非文档所称 56——56 为 v2_0 基线数,v2_1/OpenHarmony 5.0 扩充)+ `NodeAttrTypes.idl` 每 struct 自带 `@since` 标注(gitee raw 可直取):
- `https://gitee.com/openharmony/drivers_interface/raw/master/nnrt/v2_1/NnrtTypes.idl`
- `https://gitee.com/openharmony/drivers_interface/raw/master/nnrt/v2_1/NodeAttrTypes.idl`

**对 NIND/UTNet 的判定——全算子覆盖,NPU 驻留预期从"中偏好"上调为"好"**:Conv2DFusion(35) / Conv2dTransposeFusion(36) / MaxPoolFusion(92) / AvgPoolFusion(17) / Concat(31) / **PadFusion(107,支持 CONSTANT/REFLECT/SYMMETRIC)** / **PReLUFusion(112)** / AddFusion(5) / MulFusion(99) / FusedBatchNorm(68) / LayerNormFusion(75) / Reshape(119) / Resize(120) / Softmax(138) / Split(145) / StridedSlice(151) / Transpose(162) / Activation(2,含 ReLU/Sigmoid/LeakyReLU/GELU 等 20 子类)——**全部 @since 3.2 基线**(每台 NEXT 设备都有)。姊妹文档 §9-7"无 LayerNorm"的推断**有误**(LayerNormFusion 存在且 @since 3.2);Resize 暴露 coordinateTransformMode/NearestMode,覆盖 MindSpore Lite 侧已知的 Resize 模式雷区。

保留告警:IDL 明言"NNRt 算子无具体实现,由 AI 硬件驱动实现"——**枚举是词汇表,驻留仍取决于 Kirin 驱动逐算子支持**,真机回退日志(姊妹文档 §8.1 步骤 2)仍是最终判据。另:OH_NN API since 级别已定论——`neural_network_runtime.h`(模型构建/编译/执行)@since **API 9**;`neural_network_core.h`(NN_Tensor 零拷贝 + RunSync/RunAsync)@since **API 11** → NEXT(API 12+)全量可用,Rust 绑定应面向 core.h 新 API。

### 5.4 版本动态 `[release-page]`

- MindSpore Lite **2.11.0 已于 2026-10-05 发布**(repo.mindspore.cn 构建产物;release notes 尚 404)——变更内容 **[未验证]**;建议 POC 仍钉 r2.10,升级复测;
- OpenHarmony SIG 对"增加 ONNX Runtime EP"的正式请求(Gitee I8637H,2023)**已被拒绝**;grep_app 全公开 C/C++ 无 `NNRTExecutionProvider` 实现 → ORT 直连 NNRt 无社区先例,`.ms` 路线无捷径竞争者。

### 5.5 真机 POC 实测记录(2026-10-07)`[source-code/实测]`

**设备**:HUAWEI MateBook Pro(HAD-W32,Kirin X90 + Maleoon,API 26)。**方法**:converter 2.7.0 转换(手术版 onnx)→ 手写 Rust `OH_AI_*` FFI 基准(msbench)→ hdc 直推 `/data/local/tmp` 运行;同正弦输入、同 504² tile,与 ORT Windows golden 逐像素对比。

| 运行时(同机同输入) | mean ms/tile | vs 应用现状 |
|---|---|---|
| ORT CPU(应用现状,`libs/ohos` v1.26 .so,零 EP) | 1214.9 | 1.00× |
| MS Lite Kit CPU,4t / 8t | 1328.3 / 898.2 | 0.91× / 1.35× |
| **MS NNRT(Maleoon NPU,perf=HIGH)** | **257.3** | **4.72×** |
| MS NNRT(perf=EXTREME) | 399.3 | 3.04×(反低于 HIGH) |

精度(762,048 f32 vs golden):MS CPU max_abs=9.5e-07(等价 fp32);**NNRT max_abs=1.6e-03**(NPU 内部 fp16 级,[0,1] 域 0.16%,降噪可接受);ORT 设备 vs ORT Windows max_abs=3.6e-07。24MP BALANCED 估算:NNRT ≈30s vs 现状 ≈142s。NNRT 模式 build(NPU 图编译)638ms 一次性。

**全管线复刻验证(同日续测,nindfull)**:1:1 复刻应用 `run_native_denoise` tile 管线全语义(镜像提取、`apply_seamless` ×0.5 条带、累加器、clamp;BALANCED 504/480/6),单二进制双后端(ort 原版 onnx NCHW 直喂 vs 2.7 .ms NHWC→NNRT),合成噪声图(渐变+色块+高频纹理+luma/chroma 噪声)整图实测:

| 全图墙钟(含 IO/tile 前后处理) | ORT CPU(现状) | MS Kit CPU 8t | **MS NNRT** | NNRT 加速比 |
|---|---|---|---|---|
| 24MP 6000×4000(117 tiles) | **148.8s** | — | **32.3s** | **4.61×** |
| 6MP 3000×2000(35 tiles) | 44.2s | 34.0s | 10.2s | 4.33× |

逐 tile mean 1256.6 vs 257.9ms(4.87×),与单 tile POC 的 4.72× 吻合;非推理开销仅 ~2.1s(24MP)/~1.2s(6MP)——上段"≈30s vs ≈142s"估算获证。全图输出一致性(vs ORT 逐像素,7200 万 f32):**NNRT PSNR 70.61dB / max_abs 2.9e-03**(与单 tile fp16 级偏差一致);MS Kit CPU PSNR 136.2dB / max_abs 1.3e-06(实质等同)。

**2026-10-09 追加(.om 直连路线,姊妹文档 §8.4)**:`nind_sliced.onnx`(即本节手术件,sha `ad157c91…` 一致)经 CANN Kit omg(`--platform=kirinx90`,默认 fp16 权重,62MB)→ 真机 HIAI_MR_* 直连:**188.8/188.5/188.2 ms/tile = 现状 6.45×,比 .ms NNRT(257.3ms)快 1.37×**,restore 217.7ms、teardown 干净;且 `.om` 保持 NCHW I/O(omg 日志实证,免 `.ms` 的 NHWC 转置——集成可复用 ORT 路径的 NCHW tile 填充)。负 Pad 在 omg 与 MS converter 同款失败(宿主 IR 推断即拒),手术件直接复用即过。**路线选择由此更新:逐模型实测择路**(u2net `.ms` 更快 65.9 vs 83–100ms;NIND `.om` 更快)。`.om` 逐像素精度未验(`.ms` NNRT max_abs 1.6e-03 为参照;必要时 `--weight_data_type=FP32` 复测)。

**集成关键发现**(复现脚本与完整原始数据:`.omo/ultraresearch/20261007-112629/mslite-poc-results.md`):

1. **负 Pad 图手术**:onnx 出口 Pad(pads=[0,0,-2,-2,0,0,-2,-2],裁剪语义)在 MS Lite 运行时 InferShape 失败(转换成功但执行死;`--optimize`/`--inputShape`/`--inputDataFormat` 均不救)→ 替换为等价 Slice(axes=[2,3],starts=[2,2],ends=[-2,-2]),ORT 字节级零差异验证;
2. **converter 版本必须 ≤ 设备引擎版本**:本机 Kit 引擎 = 2.7.0(hilog 实证);2.10 .ms 报 "Wrong input size";2.10 运行时对 2.7 .ms 前向兼容 ✓;
3. **ABI**:`OH_AI_ModelPredict` 的 inputs **按值传递**;输出张量句柄跨 Predict **复用**(循环间销毁 → GetData NULL/大模型崩溃;**退出收尾同样不可销毁**——Predict 返回的输出句柄归 runtime 所有,收尾销毁会在打印/落盘前段错误,且 hdc 把信号退出码吞成 0,表现为"exit 0 但尾部日志与输出文件消失";收尾只做 ModelDestroy + ContextDestroy);
4. **布局**:converter 默认输入 NHWC `[1,504,504,3]`、输出 NCHW `[1,3,504,504]`;
5. NNRT 设备枚举:`NPU_ohos.boot.hardware.KirinX90_v2_0` + `HIAI_F`(后者不构建 .ms,需 omg 离线路线,§6 预判成立);perf=HIGH(3) 优于 EXTREME(4);
6. 鸿蒙 PC Kit 库在 `/system/lib64/ndk/`(手机为 lib64 直下);hdc shell 二进制直接链接 OK,无需 LD_LIBRARY_PATH。

### 5.6 ONNX → .ms 可复现转换管线(2026-10-08 入库)`[source-code/实测]`

**背景**:`.ms` 正身此前是手工侧载产物,`DENOISE_MS_URL` 占位(上游无 `.ms`,真机实测 404)——新设备/新机器无复现获取路径。现已固化为**入库脚本** `scripts/convert-nind-ms.ps1`(手术助手 `scripts/nind-fix-pad.py`),每步哈希门禁:任何环境漂移(onnx 包版本、converter 版本、模型修订)立即报错,绝不静默产出偏差模型。

**管线**(全哈希钉死,与 `ai_processing.rs` 模型常量对齐):

| 步骤 | 工具 | 产物 | 哈希 |
|---|---|---|---|
| 1. 源模型 | `-OnnxPath` 本地或自动下载(DENOISE_URL,与应用运行时同源) | `nind_denoise_utnet_684.onnx` | `ee358627…`(=DENOISE_SHA256) |
| 2. 图手术 | `scripts/nind-fix-pad.py`:负 Pad(pads=[0,0,-2,-2,0,0,-2,-2])→ 等价 Slice(axes=[2,3] starts=[2,2] ends=[-2,-2];§5.5-1;ORT 字节级零差异验证) | `nind_sliced.onnx` | `ad157c91…` |
| 3. 转换 | converter_lite **2.7.0**(WSL Linux x64;版本硬约束见下) | `nind_denoise_utnet_684.ms` | `05d7580e…`(=DENOISE_MS_SHA256)✓ |

- **可复现性实证**(2026-10-08):全管线重跑,手术与最终产物哈希字节级复现(onnx 1.23.2 + numpy + mindspore-lite-2.7.0-linux-x64);
- **converter 版本硬约束**:converter ≤ 设备 Kit 引擎(本机 2.7.0,hilog 实证);2.10 `.ms` 设备报 "Wrong input size"(§5.5-2);设备 Kit 引擎升级前勿升 converter 版本;
- **converter 仅 Windows/Linux x64**(姊妹文档 §4.1,无 macOS/Android/OHOS 转换器);脚本走 WSL Linux 路径——正身即出自该工具链(字节级一致);`-SetupWsl` 自动装包(~66MB,ms-release OBS 官方源,`~/mslite/mindspore-lite-2.7.0-linux-x64`);
- **前置**:Windows 主机 + WSL(任意发行版)+ python(onnx+numpy);
- **分发路径**:产物上传 `DENOISE_MS_URL` 所指仓库(上游 PR 或 fork,待定)后,应用内自动下载链路即活;托管建成前按脚本尾部提示手动侧载(`hdc file send` + `cp` 到 `appdata/models/`)。

Linux 原生等价(免 WSL,产物哈希应一致):

```bash
python3 scripts/nind-fix-pad.py nind_denoise_utnet_684.onnx nind_sliced.onnx
export LD_LIBRARY_PATH=~/mslite/mindspore-lite-2.7.0-linux-x64/tools/converter/lib:$LD_LIBRARY_PATH
~/mslite/mindspore-lite-2.7.0-linux-x64/tools/converter/converter/converter_lite \
  --fmk=ONNX --modelFile=nind_sliced.onnx --outputFile=nind_denoise_utnet_684
# 期望尾部日志 "CONVERT RESULT SUCCESS:0"
sha256sum nind_denoise_utnet_684.ms   # 05d7580e5a6e3c18bd286705f7524090cd69abe4b78067c2a173b322ef151b3b
```

---

## 6. 方案 C:OHOS CANN/HiAI 路线(新发现,远期备选) `[official-doc]`

**推翻姊妹文档"CANN 不适用"**:华为官方 `cann-recipes-harmony-infer`(GitCode)证明 CANN Ascend C 算子已可运行于 **Kirin 9030 / Kirin X90**(`cann/runtime` 平台配置 `Kirin9030.ini`,NPU 引擎 "STARS"):

```
ONNX →(ATC,需 CANN mobile-station 工具链,Linux 主机)→ .omc
   →(部署进 HAP rawfile)→ HiAI Foundation Kit(libhiai_foundation.so + libneural_network_core.so)
   → OH_NNCompilation_ConstructWithOfflineModelBuffer + SetDevice("HIAI_F") → NNRt 设备 → NPU
```

- 要求:HarmonyOS 5.0.1 Release+、DevEco 6.0.0+;有 `harmony_acl` 模块提供统一 ACL 接口;
- **适用面:仅 Kirin 9030(Mate 80/90 系列,2026-10-01 起)/X90;Mate 60/70(9000s/9010/9020)不在平台配置中**;
- 定位:与方案 B 终点相同(都经 NNRt 上 NPU),差异是编译前端(ATC 离线图切分 vs MindIR 运行时切分)。**仅当某模型 .ms 转换失败且目标机型为 9030+ 时作为个案备选**——与姊妹文档对 HiAI Foundation 的既有定位一致,只是从"存在但另一套栈"升级为"有 CANN 官方配方支撑"。

---

## 7. 方案 D:替代推理引擎

| 引擎 | OHOS GPU 路径 | U-Net conv/deconv | Rust 集成 | 生产先例 | 评价 |
|---|---|---|---|---|---|
| **ncnn** | ✅ 官方鸿蒙交叉编译文档 + `-DNCNN_VULKAN=ON`;OHOS 有 libvulkan(VK_OHOS_surface) | ✅ UNet 在支持列表;PR#6564 统一 Vulkan 反卷积;KIT 论文:ONNX→Vulkan U-Net 降噪在 OIDN 上**胜过 TensorRT** | 🟡 ncnnrs/tpoisonooo(FFI+vulkan feature) | ✅✅ nihui SR 全家/RealSR-NCNN-Android/LumaSR | **最强替补**;但 Kirin 手机 GPU(Maleoon)Vulkan compute 推理零实证(ncnn#6660 仅 CPU) |
| **burn** | 🟡 依赖 wgpu:gles-on-OHOS 已并(PR#7085),Vulkan-on-OHOS master 已解锁(ash #983/#1016)但**未随任何 tag 发布** | ✅ burn-onnx 0.21:Conv2d+ConvTranspose2d(26 模型验证;ConvTranspose 仅对称 padding) | ✅✅ 纯 Rust,CubeCL+SPIR-V | 🟡 增长中 | 最佳 Rust 原生;OHOS 编译性需 spike 验证 |
| MNN | 🟡 官方 harmony 构建脚本但**仅 CPU**(ARM82/SME2/BF16);HiAI NPU 后端"适用于 Android 系统" | ✅(Vulkan 后端有 Deconvolution) | ❌ 无成熟绑定 | 🟡 Alibaba | RealSR 作者实测"MNN 比 ncnn 慢" |
| TNN | 🟡 社区 lycium 移植,OpenCL 关闭;**无 Vulkan** | 🟡 #1433 分割模型转换出错 | ❌ | 🟡 | 淘汰 |
| liteRT | ❌ 平台表无 OHOS;GPU 加速库闭源 | ✅(DewarpNet 先例;Mali 拒 TRANSPOSE_CONV 需补丁) | ❌ | ✅ Google | 淘汰(OHOS) |
| tract | ✅ 编译到 ohos;GPU 仅 CUDA/Metal | ✅ | ✅✅ | 🟡 Sonos | **conv 密集 U-Net 上跑不过 ORT CPU**(blazen:"2-4× slower";polyvoice 分模型 0.74-6.3×)→ 仅兜底 |
| candle | ❌ wgpu 后端未合并(维护者:"wgpu Vulkan compute abysmal") | 🟡 Metal 成熟 | ✅✅ | 🟡 | 淘汰 |
| rten | ✅(CPU only) | ✅ | ✅✅ | ❌ 单人 | 淘汰 |

**结论**:D 不作为主路线。ncnn 是"GPU 兜底"(若 ORT-DML 与 .ms 均不可行且目标硬件 Vulkan 可靠);burn 是长期 Rust 原生期权。**WebGPU on OHOS 不可用**(ArkWeb 仅 WebGL1/2,`navigator.gpu` undefined;"6.x 之后可能逐步开放")→ ORT WebGPU EP 在 OHOS 无出路 `[community]`。

---

## 8. 方案 E:模型侧优化

### 8.1 模型事实 `[source-code]`(trougnouf/nind-denoise,UtNet.py blob c8aca802)

- 纯 CNN U-Net:**ConvTranspose 上采样**(非 Resize);funit=64,通道 3→64→128→256→512→1024→…→3(~15M 参数);算子面 = Conv / ConvTranspose / MaxPool / PReLU(默认) / Concat / ReflectionPad / Slice——**无 Resize/StridedSlice/LayerNorm/attention**,对全部 EP、NNRt、INT8 内核覆盖友好;
- 作者源码注释明确预期量化("quantizable methods…ELU, Hardswish, ConvReLU2d");
- tile 整除约束 `(((cs/2-4)/2-4)/2-4)/2-2`:504 合法;模型可接受满足约束的任意 tile(684 非形状参数)。

### 8.2 fp16:有同族溢出实证,fp32 起步 `[community]`

- **darktable-ai RawNIND UtNet2 Bayer 变体中间激活溢出 FP16**(Apple ANE/GPU 输出 NaN/Inf)被钉死 CoreML CPU;Linear(3ch)变体无碍——RapidRAW 3ch sRGB 接近 Linear 画像,但 1024 通道底层 + PReLU 无界激活仍有风险;
- 收益矩阵:DML fp16 无益甚至慢 5-10%;CUDA fp16 ~2×;CoreML fp16 慢 8×(输入不支持→回退);OpenVINO GPU fp16 ~2×;
- **CPU EP fp16 通常更慢**(x86 无原生 fp16 计算)——fp16 与 GPU EP 耦合,且仅 CUDA/OpenVINO 值得;
- 若做:darktable-ai `denoise-nind/convert.py` 的 onnxconverter-common 配方 + `keep_io_types=True` + block 最后 1×1 Conv/底层 ConvTranspose。

### 8.3 INT8:低优先 `[community]`

PReLU 非标准融合内核;无削顶激活→PTQ 校准难;静态 INT8 对噪声输入更敏感(YOLO 研究:中噪下相对掉点 >60%);SR/denoise PTQ 典型掉 0.24-0.66dB(可见);VNNI CPU 仅 1.5-2×。**仅当目标为 CPU-only 部署且能接受 QAT 时考虑**。

### 8.4 替换模型候选 `[community]`

| 模型 | 参数/算力 | 优势 | 致命伤 | 结论 |
|---|---|---|---|---|
| NAFNet-SIDD-width32 | 65 GMACs,MIT | SIDD SOTA 40.30dB,Pixel8a GPU 46ms@256² | **SIDD(手机噪声)训练→DSLR 真实噪声泛化差**:NIND 上 width64 每 ISO 垫底(-3.0dB vs SCUNet) | 不推荐 |
| SCUNet real-psnr | 17.9M,Apache-2.0 | **NIND 最强盲降噪**(+0.70dB over Restormer@ISO25600),ONNX 动态轴现成 | 大(71.8MB);算力高于 UTNet | 质量升级期权 |
| PMRID | 0.4M | 移动端 70ms/MP(SD855 GPU),k-Sigma ISO 不变式 | 传感器特定合成训练,泛化存疑 | 观察 |
| RawNIND UtNet2(darktable-ai) | 512² 静态 | 同族新模型(raw Bayer+线性 RGB),GPL-3.0 | Bayer 变体 fp16 溢出;license 与 AGPL 兼容需查 | 观察(若做 RAW 域降噪) |

### 8.5 onnxsim

常量折叠/BN 融合对固定 shape 模型常规收益;已知 UNet fp16 模型上有挂起案例。跑一次 onnxsim + Netron 检查算子面是 POC 的标准步骤,不作为独立加速项。

---

## 9. 方案 F:管线侧优化(与 EP 正交,全平台收益)

1. **tile 批维打包**(SD TiledDiffusion `MultiDiffusion` 模式):N 个 tile 堆成 `[N,3,504,504]` 单次 run——Phase 3 已确证模型 batch 维**静态 =1**(§4.4 CLAIM C),需先按 rustupsacler `make_dynamic_batch.py` 思路**重导出动态 batch 模型**(离线一次完成,校验输出一致性后入 HF);不愿重导出时的替代 = 会话池并行(见 3);
2. **IOBinding**(ORT 官方文档点名 tile 推理用例):输入 bind 一次持久、输出预分配复用——消灭每 tile 的 CPU↔GPU 往返与 ~9MB 分配;ort rc.10 完整暴露 `IoBinding`/`OutputSelector`;
3. **移除跨 `run()` 的 `Mutex<Session>`**(ort `Session` 是 `Sync`):解锁 rayon tile 并行;注意 accumulator 的 `+=` 需分区或原子化(overlap 区);
4. **tile 提取/混合优化**:`extract_tile_mirror` 的 `get_pixel` 散射读改行切片;`apply_seamless` 保持(或采用 diffusers PR#1441 的线性渐变混合替代 0.5 硬乘,进一步消缝);
5. **取消支持**:`RunOptions::terminate()`(tile 间生效)+ 前端 `task_id`/`AiTaskGuard` 改造(模式已在 `ai_commands.rs`,降噪未接入——补齐时机);
6. **降噪结果直接回流 GPU**(E3):`denoise_result` → 清 `GpuImageCache` → `to_rgba_f16` 内存重上传,跳过"存盘→重载"往返;
7. **进度粒度**:每 tile 发事件(现每 10 tile),为取消与 UX 服务;
8. OHOS 专属:`.ms` 路线的 tile 前后处理保留 Rust 侧共享(姊妹文档 §6.3 的 `AiModelRuntime` 抽象)。

**预期**:1+2 合计在 EP 之上再 ~1.3-2×;3 在纯 CPU 平台(CPU EP 兜底)提供并行收益;6 消除一次全图磁盘往返(24MP ~几十 MB TIFF 写+读+解码)。

---

## 10. 竞品对照与用户预期预算 `[community]`

| 产品 | 加速栈 | 门槛 | 速度(24MP 级) |
|---|---|---|---|
| Lightroom AI Denoise | 联合去马赛克+去噪 CNN,目标 TensorCores/ANE,输出线性 DNG | 官方"完整 AI 特性"8GB 专用/16GB 共享 | RTX4090 ~4s;4070 ~6s;3060 ~7-8s;M4 Max(NE 重启后)~5s;GTX1060 ~159s |
| DxO DeepPRIME/XD2 | 多厂牌 GPU,FP32 吞吐为最佳预测(≥3.5TFLOPS),~2MP/s 基线 | RTX 4GB/GTX 8GB/RX6000 4GB/ARC 8GB | RTX4060 DP ~5s(XD2s ~38s) |
| Topaz Photo AI | 本地 Neuroserver(torch+cu128)+云兜底;Snapdragon Hexagon NPU | 6-8GB VRAM | M2 Pro ~4.7s(比 Adobe 快 2-4×) |
| Capture One 16.8 | GPU(后台异步,每图一次跨变体复用) | — | M5-Pro 10.9s;RTX5080 6-10s |
| darktable | OpenCL 每模块内核+透明 CPU 回退;`restore_rgb.c` 为 tiled AI 驱动模板 | 1GB+ GPU | 非神经降噪 |
| **本仓现状** | **ORT CPU EP** | — | **18-22s(桌面 CPU)/分钟级体感** |

**预期预算分档**(50+ 用户报告综合):<10s=快;10-30s=可接受;30-90s=能忍;>90s=慢;>5min=不可用。
**含义**:桌面 DML/NE 后 <10s 可期(独显)、iGPU 落 10-30s 档;OHOS NPU 目标 ≤30s(8-15× 于当前 OHOS CPU 基线 `[inferred]`)。移动先例:Lightroom Android **无** AI Denoise(16 万票 feature request);iPad M1+8GB ~20s;Snapseed HDRnet 证明手机端神经增强可至 14ms/1080p(低清预测+高清应用架构)。

---

## 11. 推荐路线图

### 11.0 立即可做(无真机,桌面)

1. Phase 3 已定论(§4.4):**dll 必换、EP 链路可用、iGPU 须门控**——落地:
2. `Cargo.toml`:ort 依赖改 `default-features = false, features = ["std","ndarray","load-dynamic","directml","coreml","nnapi"]`(消除 pyke 死重,顺带解决 #470);
3. build.rs:桌面 Windows 下载切 `onnxruntime-windows-x86_64-dml.dll`(+aarch64 DML)+ `directml-windows-*.dll` 进 resources,更新 sha256;
4. `ai_processing.rs`:抽公共 `build_session(path)`(4 个创建点合一),平台条件注册 EP(Windows: DML + **GPU 能力门控/用户开关**——Phase 3 证明老旧 iGPU 上 DML 反慢 2×,默认仅独显启用或对齐"处理后端"设置;实测无需 `with_memory_pattern(false)`;macOS: CoreML+MLProgram;Android: NNAPI;OHOS: 无——留给 .ms 路线)+ `is_available()` 探测日志;
5. 全量门禁(fmt/clippy/host+aarch64-ohos check)+ bench/ 增加降噪墙钟相位(§E4 方案);
6. 管线侧第一批:IOBinding + 移除跨 run 锁 + 每 tile 进度/取消(§9.2/9.3/9.5)。

### 11.1 OHOS(真机,承接姊妹文档 §8.1)

1. ~~converter_lite 转换冒烟 + 离线精度校验~~ **已完成**(2.7.0 转换;负 Pad 图手术;MS CPU 精度 9.5e-07,§5.5);
2. ~~Kit CPU 冒烟 + NNRT 点亮~~ **已完成**(KirinX90 NNRt 设备点亮,257ms/tile,§5.5);
3. benchmark 矩阵:**fp32 主结果已入库**(§5.5);余量:{fp16 标志, WEIGHT_QUANT int8} × 三 tile 档(504×480/448 两档需按 tile 形状分别转换或动态维 POC);
4. `AiModelRuntime` 抽象 + `.ms` 下载管线 + 降级语义(姊妹文档 §6)——**msbench FFI 模块可直接演进为仓内 `ms_lite` 绑定**(POC 已排雷全部 ABI 陷阱:按值传参/句柄复用/布局)。

### 11.2 远期

- CANN 路线评估(目标机型到 Mate 80/90 时,§6);
- SCUNet/RAW 域降噪质量升级(§8.4);
- TensorRT RTX / burn / ncnn 视 Phase 1-2 结果决定。

**验收标准**:桌面 = 24MP 降噪进入 <10s(独显)/<30s(iGPU)档 + 取消可用;OHOS = 真机 NPU 加速比数据入库(目标 ≥5×)+ 回退路径验证;每项变更走全量门禁 + bench 对照。

---

## 12. 风险与未验证项

**本研究会话未能验证的项**:

1. ~~仓库桌面 dll 的 EP 集~~ **已验证:CPU-only**(Phase 3,§4.4 CLAIM A);
2. ~~DML 本机实测~~ **已验证:EP 链路可用,但老旧 iGPU(UHD 630)上慢 2×**(§4.4 CLAIM B)——**独显实测仍缺**(本机无独显;#576 补丁版 10-20× 为间接证据,POC 需在独显机复测);
3. ~~NIND onnx batch 维~~ **已验证:静态 =1**,批维打包需重导出模型(§4.4 CLAIM C);
4. ~~NNRt 算子清单~~ **已关闭:84 算子,U-Net 全覆盖 @since 3.2;NNRT 真机运行成功**(§5.5——逐算子驻留率未逐条记录,时延/精度证据间接确认);
5. MindSpore Lite 2.11.0 变更内容(release notes 404);
6. ~~ORT 在 OHOS 真机上的 CPU 推理 ms 级数据~~ **已实测:1214.9 ms/tile**(Kirin X90,`libs/ohos` v1.26 .so,§5.5);
7. wgpu Vulkan-on-OHOS 随 tag 发布的时点(burn 路线前提);
8. Kirin 手机 GPU(Maleoon)上 Vulkan compute 推理的可用性(ncnn 路线前提,零实证);
9. 本机以外 GPU 上的 DML 表现(单机验证,外推需谨慎;iGPU/AMD 未测)。

**项目侧风险**:

- Cargo.lock 载重(archmage 三件套/windows 钉版)——ort 依赖 feature 变更需显式包列表重验 host+aarch64-ohos 双 check;
- MINDSPORE_LITE_NPU_EVAL.md 的"桌面 dll 含 CUDA/NNAPI/DML/OpenVINO"表述需按 Phase 3 结论**订正**(其引用亦源于字符串级检查的可能性);
- HARMONYOS_PORTING.md §2 风险登记表"社区预编译停在 1.16.3"已过时(实际 1.26/1.28.2/OHOS arm64 至 1.27.0);
- DML 维护模式:长期(2 年+)可能需迁移 WebGPU EP(ORT ≥1.24.4,ort crate rc.13+)或 IHV EP;
- 上游合并策略:本仓为 port 分支,桌面 EP 改动宜同步上游 PR(作者已认知需求 #997/#576,"AI Connector"是其替代方案——我们的"换 dll+注册"方案更轻,PR 时可作论据)。

---

## 13. 参考资料

**官方文档**
- ORT DirectML EP(约束/设备选择/维护模式):https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html ;https://learn.microsoft.com/en-us/windows/ai/new-windows-ml/webgpu-ep
- ORT IOBinding/设备张量(tile 推理用例):https://onnxruntime.ai/docs/performance/tune-performance/iobinding.html ;https://onnxruntime.ai/docs/performance/device-tensor.html
- ORT CoreML EP(MLProgram/fp16):https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html
- ort crate(rc.10 EP 注册/API/打包):https://ort.pyke.io/perf/execution-providers ;crate 源码 pykeio/ort@d1ebde95(EP 模块 src/execution_providers/)
- ORT 量化:https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html
- ncnn 鸿蒙交叉编译:https://github.com/Tencent/ncnn/blob/master/docs/how-to-build/how-to-build.md
- burn-onnx(ONNX 算子支持):https://github.com/tracel-ai/burn-onnx/blob/918ec96e/SUPPORTED-ONNX-OPS.md
- CANN 鸿蒙推理配方(Kirin 9030/X90):https://gitcode.com/cann/cann-recipes-harmony-infer ;https://gitcode.com/cann/runtime(platform_config/kirin)
- wgpu gles-on-OHOS:https://github.com/gfx-rs/wgpu/pull/7085 ;ash VK_OHOS_surface:https://github.com/ash-rs/ash/pull/1016
- MindSpore Lite 2.11.0 产物:https://repo.mindspore.cn/mindspore/mindspore-lite/version/202610/ (r2.11_20261005)
- csukuangfj/onnxruntime-libs OHOS workflow(fp16/bf16 剥离补丁):https://github.com/csukuangfj/onnxruntime-libs/blob/master/.github/workflows/harmony-os-shared.yaml

**源码/仓库(SHA 锚定)**
- 本仓管线:`src-tauri/src/ai_processing.rs`(tile 循环 894-974、会话 612-614)、`denoising.rs`、`gpu_processing.rs`(to_rgba_f16:552、read_texture_data_roi:481、异步回读 2092-2226)
- 上游 NIND 提交:https://github.com/CyberTimon/RapidRAW/commit/4ba3e25a1d16359f6a1cb2d17c042ffbc0038d41
- UtNet 架构源码:https://github.com/trougnouf/nind-denoise/blob/c8aca802672dcada364cbc6f9b5b815a6f8fedd3/src/nind_denoise/networks/UtNet.py
- darktable-ai 模型卡+转换脚本:https://github.com/darktable-org/darktable-ai/blob/9ec0bf11/models/rawdenoise-nind/README.md ;.../685420eb/models/denoise-nind/convert.py ;tiled AI 驱动:https://github.com/darktable-org/darktable/blob/9a61cd65/src/common/ai/restore_rgb.c
- HF 模型仓库(DML 风味 dll 在列):https://huggingface.co/CyberTimon/RapidRAW-Models(sha daec18e7;镜像 hf-mirror.com 同路径)

**Issue/讨论**
- 上游 #997(降噪未 GPU 加速)/#576(NIND 集成与作者"打包复杂"表态,含补丁版 10-20× 实测)/#1090(Android NNAPI PR):https://github.com/CyberTimon/RapidRAW/issues/997 等
- pyke-io/ort:#94(DirectML.dll 须在 exe 旁)/#222/#470(默认构建携带 DirectML.dll)/#551(rc.12 DML 崩溃)/#560(load-dynamic 死锁):https://github.com/pykeio/ort/issues/94 等
- ORT:#23340(DML fp16 无益)/#17033+#17448(CoreML fp16 回退)/#25266(ORT+OpenVINO vs 直连):https://github.com/microsoft/onnxruntime/issues/17033 等
- OpenHarmony SIG 拒绝 ORT EP 请求:https://gitee.com/openharmony/ai_neural_network_runtime/issues/I8637H
- XNNPACK OHOS 交叉编译:https://github.com/google/XNNPACK/pull/8649

**社区/学术**
- Paddle Lite Kirin NPU 基准:https://www.paddlepaddle.org.cn/inference/v2.5/demo_guides/huawei_kirin_npu.html
- Mate 60 Pro ML Kit NPU 实测:https://ost.51cto.com/posts/45779 ;Kirin 9000S NPU 博客:https://bbs.huaweicloud.cn/blogs/479687
- UNet 引擎对比(TensorRT vs ORT vs OpenVINO):arXiv:2607.11356
- TiledDiffusion(批维+线性混合):https://github.com/shiimizu/ComfyUI-TiledDiffusion ;diffusers PR#1441(overlap lerp)
- KIT ONNX→Vulkan U-Net 降噪(胜 TensorRT/OIDN):https://publikationen.bibliothek.kit.edu/1000195383/185641879
- tract vs ORT:https://github.com/ekhodzitsky/polyvoice/blob/e35db215/benchmarks/results/tract-backend-verdict.md
- 竞品速度汇总:Adobe community GPU benchmark 线程、dpreview AI Denoise timings、DxO 论坛、pugetsystems LrC 15.2 分析
- NIND 数据集论文:arXiv:1906.00270;RawNIND:arXiv:2501.08924

---

*本报告由 ultraresearch 会话产出(15 worker / 3 波次 / 详录见 `.omo/ultraresearch/20261007-112629/`);Phase 3 可执行验证结果将以 §4.4 回填形式落地。*
