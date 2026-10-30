# npu-models — OHOS NPU 模型专用目录

> 本目录集中存放 **OHOS NPU(CANN `.om`)链路的模型资产**:源 ONNX、转换产物、图手术中间文件与精度评估数据。二进制文件**不入库**(见根 `.gitignore`),本 README 与评估脚本入库,保证可复现。
> 建立日期:2026-10-30 · 适用分支:`v1.6.7` · 目标平台:`kirinx90`(X90 与 XE90 实测等价,见 `docs/KIRIN_XE90_SUPPORT.md` §3.1)

## 目录结构

```
npu-models/
├── README.md            # 本文件(入库)
├── onnx/                # 源 ONNX(v1.6.7,SHA256 与代码常量一致;不入库)
├── om/
│   ├── kirinx90/        # 生产产物:omg --platform=kirinx90 转换的 .om(不入库)
│   └── poc/             # POC 期产物存档,仅作对照(不入库)
├── work/                # 图手术中间文件:onnxsim 输出、Resize 手术版等(不入库)
└── eval/                # 真实输入精度评估:输入/参考输出/设备输出/对比脚本与结果
    ├── *.py             # 对比脚本(入库)
    └── *.bin/*.npy      # 二进制数据(不入库)
```

## 1. 源模型(`onnx/`)

| 文件 | SHA256(与 `src-tauri/src/ai_processing.rs` 常量一致) | 用途 |
|---|---|---|
| `mediapipe_face_landmarks.onnx` | `6524d596…74b0bb4` | 人脸关键点(478 点) |
| `mediapipe_selfie_multiclass.onnx` | `a198470a…81b9a08e` | 人脸多类分割(6 类) |
| `nind_denoise_utnet_684.onnx` | `ee358627…017e7fe` | AI 降噪(504² tile) |

来源:`https://hf-mirror.com/CyberTimon/RapidRAW-Models/resolve/main/<file>`(hf-mirror 有 429 限流,失败重试即可)。

## 2. 生产产物(`om/kirinx90/`)

| `.om` | 源 | 转换配方 | 状态 |
|---|---|---|---|
| `nind_v2.om` | NIND ONNX | **Pad→Slice 手术** → omg(历史配方,`DENOISE_OM_FILENAME` 侧载名;504 tile,NCHW,perfmode 4) | ✅ 已在 X90 接入 |
| `face_landmarks.om` | face ONNX | onnxsim → omg,`--input_shape` 固定 batch=1 | 🚧 生产构建中(POC 版在 `om/poc/`,双机 Run 通过) |
| `selfie_multiclass.om` | selfie ONNX | **Resize 手术**(`tf_half_pixel_for_nn`→`half_pixel`、`floor`→`round_prefer_floor`)→ omg | 🚧 生产构建中(POC 版 `selfie_fix2.om`,双机 Run 通过) |

转换命令(WSL,CANN Kit DDK 6.1.1.0;loader 包装见 `docs/CANN_MODEL_MIGRATION.md` §6.3):

```bash
omg --framework=5 --model=<onnx> --output=<out> --platform=kirinx90 \
    --input_shape="<name>:<dims>"   # CNN 用 perfmode 4
```

## 3. 评估方法(`eval/`)

真实照片 → 按应用侧预处理契约生成输入(`face:/255`、`selfie:/127.5-1`,均 NHWC `[1,256,256,3]`;NIND NCHW `[1,3,504,504]`)→
① WSL onnxruntime 跑源 ONNX 得参考输出;② `.om` 在 X90/XE90 用 `hiai_probe`(HIAI_MR 直连)跑同输入;
③ 对比 `max_abs / mean_abs`,selfie 另算 argmax 类别一致率,NIND 另算 PSNR。结果记录在 `docs/CANN_MODEL_MIGRATION.md`。

## 4. 注意事项

- `.om` 按 SoC 编译,当前只有 `kirinx90`;X90/XE90 实测互通,其他 Kirin 必须单独验证。
- `.om` 体积大,通过 `scripts/build-ohos.ps1` 的注入流程或 CE 侧载分发,**不打进 git**。
- `om/poc/` 里的 `esam3_*.om`、`u2net.om` 是历史 POC 产物,不在生产路径上(结论见 `docs/CANN_MODEL_MIGRATION.md` §7)。
