# LightRaw — AppGallery（华为应用市场 / AGC）上架文案

> 用途：AGC 控制台「应用信息 / 版本信息」各文本字段的填写稿。
> 事实来源：`README.md`、`src/i18n/locales/{zh-CN,en}.json`、`docs/HARMONYOS_PORTING.md`（§6.15 / §6.17 / §6.18）、
> `src-tauri/gen/ohos/entry/src/main/module.json5` + `resources/base/element/string.json`、
> `src-tauri/gen/ohos/AppScope/app.json5` + `AppScope/resources/base/element/string.json`、`src/utils/ohosPrivacy.ts`。
> 品牌名以**包内 `app_name` / `EntryAbility_label`（均为 `LightRaw`）**为准。凡仓库中查不到的字段，本文件显式标注「仓库中未找到」，不作推测。

---

## 1. 应用名称（AGC：应用名称）

> LightRaw

- 依据：`src-tauri/gen/ohos/AppScope/resources/base/element/string.json` → `app_name = "LightRaw"`；
  `entry/.../resources/base/element/string.json` → `EntryAbility_label = "LightRaw"`、`module_desc = "LightRaw"`。
- AGC 控制台应用名称 / 图标必须与包内一致；包内已无 `RapidRAW` 字符串（§6.17 二进制复核）。
- bundleName 仍为 `io.github.CyberTimon.RapidRAW`（`AppScope/app.json5`），品牌重塑时刻意保留以维持签名 / 备案绑定（§6.17）；store 名称与 bundleName 不一致属预期。

<sub>脚注：§6.15（2026-10-06）曾记录「上架材料要点：应用名称须填 "RapidRAW"」；该条已被 §6.17（2026-10-11）的品牌重塑取代，现以 **LightRaw** 为准（§6.17 提审包 v3 原文明示「AGC 控制台应用名称 / 图标须同步改为 LightRaw」）。</sub>

---

## 2. 一句话简介 / 应用简介（AGC：应用简介 ≤ 100 字）

**中文（简体）**

> 一款极速、GPU 加速的非破坏性 RAW 图像编辑器，内置 AI 降噪、AI 蒙版与 AI 标签，支持镜头校正、预设与批量导出，界面提供简体中文等 15 种语言。

**English（≤ 80 words）**

> A blazingly fast, GPU-accelerated, non-destructive RAW image editor with built-in AI denoise, AI masking (subject, sky, foreground, depth) and AI tagging, plus lens correction, presets and batch export. Available in 15 UI languages.

<sub>术语依据：应用自身欢迎页文案 `splash.descriptionDesktop` =「一款极速、GPU 加速的 RAW 图像编辑器。打开文件夹以开始。」/ "A blazingly fast, GPU-accelerated RAW image editor."（zh-CN.json / en.json）；「非破坏性」取自 README（非破坏性工作流，编辑写入 `.rrdata` 侧车文件）。
字数实测（AGC 上限 100 字）：该中文简介共 **81 个字符**（含空格与标点），其中汉字 **50 个**，去空白后 **71 字符**——均在 100 字上限内。</sub>

---

## 3. 应用介绍 / 详细描述（AGC：应用介绍 / 详细描述）

### 3.1 中文版

**LightRaw（鸿蒙 PC 版）** 是一款面向摄影师的极速、GPU 加速、非破坏性 RAW 图像编辑器。它把专业调色、蒙版与细节处理收进一个轻量、现代的工作流：所有调整都写入侧车文件 `.rrdata`，原始文件始终保持不变，每一步都可随时回退。

图像处理管线基于 Rust 与 wgpu 在 GPU 上完成（鸿蒙端使用 Vulkan 后端），编辑过程实时预览，拖动滑杆立刻看到结果。图库支持递归文件夹、星级评分、颜色标签、自定义标签、筛选视图与批量操作，导入、整理、比较、导出一气呵成。

内置 AI 能力可在设备本地运行：AI 降噪（NIND）、AI 主体 / 天空 / 前景蒙版、AI 深度蒙版、AI 自动标签（CLIP）与本地图像修复（LaMa）；在支持的设备上，AI 降噪与 AI 蒙版还可选择设备端 NPU 加速，NPU 或模型不可用时自动回退 CPU。需要生成式替换时，可连接用户自托管的生成式后端（AI 连接器）。

应用界面提供简体中文、繁體中文、English、日本語、한국어 等 15 种语言，并可跟随系统深浅色主题切换。

**主要功能**

- **非破坏性 RAW 编辑**：所有调整保存在 `.rrdata` 侧车文件，原始图像永不被改动
- **广泛的格式支持**：通过 rawler 解码主流相机 RAW（DNG / CR2 / CR3 / NEF / ARW / RAF / ORF / RW2 等），同时支持 JPEG / PNG / WebP / TIFF / JXL / EXR 等常见格式
- **专业调整与创意效果**：曝光、AgX 色调映射、对比度、高光 / 阴影 / 白色 / 黑色、参数曲线、色彩分级（色轮与 HSL）、锐化与清晰度；以及镜头模糊（散景）、LUT、去雾、暗角、辉光、光晕、胶片颗粒与模拟胶片
- **AI 能力**：AI 降噪（NIND）、AI 主体 / 天空 / 前景蒙版、AI 深度蒙版（Depth Anything V2）、AI 自动标签（CLIP）、本地图像修复（LaMa）
- **蒙版与润饰**：画笔 / 线性 / 径向 / 颜色 / 亮度 / 深度蒙版，克隆与修复工具，液化与润饰
- **镜头校正**：基于 Lensfun 的畸变、横向色差（TCA）与暗角自动校正，支持镜头配置自动识别
- **合并与合成**：包围曝光 HDR 合并、无缝全景拼接、焦距合成、拼贴画、胶片负片转换
- **图库与批处理**：递归文件夹、评分、颜色标签、标签搜索、筛选视图、虚拟副本、批量应用与批量导出
- **导出与元数据**：导出 JPEG / PNG / WebP / AVIF / TIFF / JXL / LUT，支持水印、EXIF 保留与拍摄时间戳保留
- **系统集成**：通过系统文件选择器打开文件夹与导入图片，编辑结果可保存到系统图库 / 相册；跟随系统深浅色主题

<sub>字数实测：本节正文（不含「主要功能」列表）**约 430 字**（去空白计，其中汉字 289 个），落在 300–600 字区间；「主要功能」列表共 **10 条**。若 AGC 字段有整体字数上限，可删去「导出与元数据」「系统集成」两条列表项。</sub>

### 3.2 English

**LightRaw (HarmonyOS PC edition)** is a blazingly fast, GPU-accelerated, non-destructive RAW image editor for photographers. Professional color, masking and detail tools live in a lightweight, modern workflow: every adjustment is written to a `.rrdata` sidecar file, your originals are never modified, and any step can be reverted at any time.

The image pipeline runs on the GPU via Rust and wgpu (Vulkan on HarmonyOS), so previews stay live while you drag a slider. The library offers recursive folders, star ratings, color labels, custom tags, a culling view and batch operations — import, organize, compare and export in one flow.

Built-in AI runs locally on your device: AI denoise (NIND), AI subject / sky / foreground masking, AI depth masking, automatic AI tagging (CLIP) and local inpainting (LaMa). On supported hardware, AI denoise and AI masking can use the on-device NPU, falling back to CPU automatically when the NPU or model is unavailable. Generative replace is available by connecting your own self-hosted generative backend (AI Connector).

The interface ships in 15 languages, including English, Simplified Chinese, Traditional Chinese, Japanese and Korean, and follows the system light/dark theme.

**Key features**

- **Non-destructive RAW editing**: all adjustments are stored in a `.rrdata` sidecar; originals are never touched
- **Broad format support**: mainstream camera RAW via rawler (DNG / CR2 / CR3 / NEF / ARW / RAF / ORF / RW2 …) plus JPEG / PNG / WebP / TIFF / JXL / EXR
- **Professional adjustments & creative effects**: exposure, AgX tone mapping, contrast, highlights / shadows / whites / blacks, parametric curves, color grading (color wheels and HSL), sharpening and clarity; plus lens blur (bokeh), LUTs, dehaze, vignette, glow, halation, film grain and analog film emulations
- **AI capabilities**: AI denoise (NIND), AI subject / sky / foreground masking, AI depth masking (Depth Anything V2), automatic AI tagging (CLIP), local inpainting (LaMa)
- **Masking & retouching**: brush / linear / radial / color / luminance / depth masks, clone and heal tools, liquify and retouch
- **Lens correction**: automatic distortion, transverse chromatic aberration (TCA) and vignetting correction powered by Lensfun
- **Compositions & merging**: bracketed HDR merge, seamless panorama stitcher, focus stacking, collage maker, film negative conversion
- **Library & batch**: recursive folders, ratings, color labels, tag search, culling view, virtual copies, batch apply and batch export
- **Export & metadata**: export to JPEG / PNG / WebP / AVIF / TIFF / JXL / LUT with watermarking and EXIF retention
- **System integration**: open folders and import images through the system file picker, save results to the system gallery, follow the system light/dark theme

---

## 4. 本次更新 / 更新说明（AGC：版本更新说明）

版本绑定：`AppScope/app.json5` → `versionName = "1.0.0"`、`versionCode = 1`（鸿蒙 PC 首发版本）。

### 4.1 中文

> **v1.0.0（鸿蒙 PC 首发）**
>
> - 鸿蒙 PC（2in1）首个正式版本，基于 wgpu / Vulkan 的 GPU 加速编辑管线
> - 支持通过系统文件选择器打开文件夹、导入图片，并将导出结果保存到系统图库 / 相册
> - 内置 AI 降噪（NIND）、AI 主体 / 天空 / 前景蒙版、AI 深度蒙版与 AI 自动标签，支持设备端 NPU 加速并可自动回退 CPU
> - 完整的非破坏性编辑链路：RAW 解码、镜头校正、蒙版与润饰、预设、批量导出
> - 界面支持简体中文等 15 种语言，并可跟随系统深浅色主题

### 4.2 English

> **v1.0.0 (first HarmonyOS PC release)**
>
> - First release for HarmonyOS PC (2in1), with a GPU-accelerated editing pipeline built on wgpu / Vulkan
> - Open folders and import images through the system file picker, and save exports to the system gallery
> - Built-in AI denoise (NIND), AI subject / sky / foreground masking, AI depth masking and automatic AI tagging, with optional on-device NPU acceleration and automatic CPU fallback
> - Full non-destructive editing: RAW decoding, lens correction, masks and retouching, presets and batch export
> - 15 UI languages including Simplified Chinese, and system light/dark theme support

<sub>脚注：包版本号 `versionName` 为 `1.0.0`（app.json5）；跨平台应用自身的版本字符串为 `1.6.4`（`src-tauri/tauri.conf.json` 的 `version`，§6.4 记录的模拟器首亮截图中欢迎页也显示 1.6.4）。两者不一致属已知情况，提审时以包内 `versionName` 为准；若 AGC 要求版本号与展示一致，建议统一后再出包。</sub>

---

## 5. AI 功能声明（AGC：AI 功能声明）

> **本应用包含 AI 功能，声明如下：**
>
> 1. **AI 降噪（NIND）**：对 RAW / 图像进行 AI 降噪，推理在设备本地完成（ONNX Runtime）。在支持的设备上可选择设备端 NPU 后端加速；NPU 或模型不可用时自动回退本地 CPU 推理。
> 2. **AI 蒙版（AI 主体 / 天空 / 前景）**：自动识别并生成主体、天空、前景蒙版，推理在设备本地完成（本地 ONNX 模型；设备端 NPU 可用时同样支持本地 NPU 推理）。
> 3. **AI 深度蒙版（Depth Anything V2）**：在设备本地生成单目深度图，用于按景深选择区域。
> 4. **AI 自动标签（CLIP）**：可选功能，默认关闭；启用后在设备本地对图像生成关键词标签并写入本应用的 `.rrdata` 侧车文件，用于图库搜索。
> 5. **本地图像修复 / 内容感知填充（LaMa）**：在设备本地 CPU 上运行，用于移除小型杂物、填充选区；不会上传图像。
> 6. **生成式替换（可选，需用户自行配置后端）**：当用户主动连接**自托管的生成式后端（AI 连接器 / ComfyUI）**时，所选的图像区域、蒙版与提示词会被发送到该用户自行配置的后端进行处理。本功能不默认开启，不依赖本应用服务器。
> 7. **AI 镜头模糊（散景）**：基于本地深度估计在设备上生成散景效果。
>
> **数据处理说明：**
>
> - 上述第 1–5、7 项在设备本地完成推理，推理过程不上传用户照片；
> - AI 模型文件在首次使用时从网络下载到应用数据目录（`app_data_dir()/models/`），此为应用联网的主要用途之一；
> - 仅第 6 项在用户主动配置生成式后端后，才会把待处理图像数据发送到用户自己的后端；
> - 应用不收集人脸等生物特征数据，不进行云端训练。

<sub>事实依据：README「AI Roadmap」——内置 AI 工具「run entirely on your computer」；自托管 ComfyUI 为用户自有服务器；`ai_processing.rs` 中模型均自 `hf-mirror.com` 下载到本地（models：SAM / U2Net / DepthAnything V2 / CLIP / LaMa / NIND）；`zh-CN.json` 中 `settings.processing.aiDenoiseBackendDesc`（「NPU 在设备端 NPU 芯片上运行……自动回退 CPU（ONNX Runtime）」）；docs §6.16（CANN `.om` NPU 全链路在应用进程内、设备端完成）。
> 仓库中**可选云端生成式服务**在 README 的 AI 路线图状态表中标注为 **Coming Soon**（$TBD / 月订阅），故本声明未把「云端 AI」列为已上线能力；若提审版本已启用云端服务，需据实补充说明。
> 说明：README 中 `Camera Tethering`（联机拍摄）与 `headless CLI batch export` 为桌面（macOS / Linux）专属，不属鸿蒙版本能力，未在此声明中列出。</sub>

---

## 6. 权限使用理由（AGC：权限使用理由）

以下为 `src-tauri/gen/ohos/entry/src/main/module.json5` 中**实际声明**的全部权限，「用途说明」基于包内 `reason` 字符串（`resources/base/element/string.json` 的 `$string:perm_*`）并作必要中文化，与 AGC 权限一致性检测逐项对应。

| 权限名 | 用途说明 |
| --- | --- |
| `ohos.permission.INTERNET` | 用于下载 AI 模型文件（SAM / U2Net / DepthAnything / CLIP / LaMa / NIND 等）以及连接用户自行配置的生成式 AI 后端。（包内未配置 reason 字符串） |
| `ohos.permission.FILE_ACCESS_PERSIST` | 持久化保存用户通过系统文件选择器授予的文件夹授权，使应用重启后仍可访问用户指定的图片文件夹。（包内未配置 reason 字符串） |
| `ohos.permission.WRITE_IMAGEVIDEO` | 将编辑后的照片与导出的图片保存到系统媒体库（相册）。 |
| `ohos.permission.READ_WRITE_DOWNLOAD_DIRECTORY` | 浏览 Download 文件夹中的照片，并将导出的图片保存到该文件夹。 |
| `ohos.permission.READ_WRITE_DOCUMENTS_DIRECTORY` | 浏览 Documents 文件夹中的照片，并将导出的图片保存到该文件夹。 |
| `ohos.permission.READ_WRITE_DESKTOP_DIRECTORY` | 浏览 Desktop 文件夹中的照片，并将导出的图片保存到该文件夹。 |
| `ohos.permission.MEDIA_LOCATION` | 读取照片中内嵌的位置信息，并在元数据面板中展示。 |

- 声明总数：**7 项**（2 项非 user_grant：`INTERNET`、`FILE_ACCESS_PERSIST`；5 项 user_grant 权限，均带 `reason` + `usedScene`，`when = inuse`，作用范围 `EntryAbility`）。
- ACL 受限权限（`WRITE_IMAGEVIDEO`、`READ_WRITE_*_DIRECTORY`、`FILE_ACCESS_PERSIST`）需在 release Profile 的 `allowed-acls` 中覆盖，否则安装 / 上传期被拦截（§6.15）。
- AGC 会逐项比对「包内 user_grant 权限」与「隐私声明中申报的权限」，任一方向多出即被拒（§6.15）。

<sub>脚注 1（权限数量矛盾）：§6.15 与 §6.17 提审包 v2 均记录 **8 项**权限（含 `ohos.permission.READ_IMAGEVIDEO`）。§6.17 提审包 v3（2026-10-11 16:10）已依审核意见**移除** `READ_IMAGEVIDEO`（应用经系统选择器访问图片，代码零引用），收敛为 **7 项**定版；当前 `module.json5` 亦即 7 项。本表以 7 项为最新事实。
脚注 2：包内 `string.json` 仍保留未被引用的 `perm_read_imagevideo` 字符串资源（§6.17 记为「闲置保留，无引用，无害」），它不对应任何已声明权限，不应填入 AGC 权限表单。</sub>

---

## 7. 隐私政策与隐私权利 URL（AGC：隐私政策 URL / 隐私权利 URL）

**以下 URL 为本次上架使用的取值（用户 2026-10-25 提供，以本条为准）**：

| AGC 字段 | 取值 | 来源 |
| --- | --- | --- |
| 隐私政策 URL（标准化隐私声明托管） | `https://agreement-drcn.hispace.dbankcloud.cn/index.html?lang=zh&agreementId=2068880948149116608` | 用户提供（2026-10-25） |
| 隐私权利 URL | 同上（复用该标准化隐私声明链接） | 同上 |
| 托管协议 agreementId | `2068880948149116608` | 同上 |
| 用户协议 URL | 同一托管协议页面 | 同上；§6.18 记为「无独立用户协议文档」 |

- `lang` 参数切换中 / 英（`lang=zh` / `lang=en`）；上架后以 **AGC 云侧托管配置**为准（提审包内已确认**不含** `appgallery_privacy_*` 预置 metadata；`appgallery_privacy_*` metadata 仅属未上架调试构建，§6.17 提审包 v4 / §6.18 第 4 节）。
- 应用内不再展示自建隐私弹窗与本地法务文本，首启同意由平台托管弹窗承担（§6.17 / §6.18）；设置页「关于」展示平台下发的隐私政策 / 用户协议链接。

**⚠️ 与仓库现存取值的差异（需同步，否则平台弹窗与包内回退不一致）**：

- `src/utils/ohosPrivacy.ts` 仍写着旧值 `PRIVACY_STATEMENT_AGREEMENT_ID = '2053968770484970432'`（连同 `PRIVACY_STATEMENT_BASE`），属应用内**兜底**常量；要跟随本次上架取值为 `2068880948149116608` 需改该常量。
- `docs/HARMONYOS_PORTING.md` §6.18（第 4 节配方与实测行）记录的也是旧 `agreementId=2053968770484970432`，属历史记录；若以新协议为准，应在其后追加一条更新说明。

---

## 8. 上架前置清单

- [ ] **名称 / 图标一致性**：store 应用名称 = **LightRaw**，与包内 `app_name` / `EntryAbility_label` 一致；图标使用 `LightRaw.png`（1024×1024，已直拷 `AppScope` + `entry` 的 `foreground.png`）；控制台不得残留 `RapidRAW`（§6.17）。
- [ ] **截图 ≥ 3 张**：仓库侧要求「截图 ≥3 张」（§6.15）；AGC 表单要求的 **1920×1080 / 16:9** 为平台规则（仓库文档未记录该分辨率，以 AGC 控制台实际提示为准）。
- [ ] **AI 功能声明**：已按第 5 节申报（AI 降噪 / AI 蒙版为**必须**申报项，§6.15）。
- [ ] **隐私声明托管**：AGC 侧配置标准化隐私声明（`agreementId=2068880948149116608`，2026-10-25 用户提供）；提审包内**不得**含 `appgallery_privacy_*` metadata；隐私权利 URL 复用同一链接。
- [ ] **支持设备只勾选 `2in1`（PC）**：包内 `deviceTypes = ["2in1"]`，控制台多勾其他形态会报 998（§6.15）。
- [ ] **权限一致性**：包内 7 项权限与隐私声明申报逐项一致；ACL 受限权限在 release Profile 的 `allowed-acls` 覆盖（§6.15 / §6.17）。
- [ ] **提审包必须 `-Release`**：禁止携带调试信息（dev `.so`、`.debug_*` / symtab、`ets/sourceMaps.map`），§6.17。
- [ ] **中国大陆合规**：APP 备案 + 电子版权证书（非国区首发可豁免后两项，§6.15）。
- [ ] **版本号确认**：`versionName 1.0.0 / versionCode 1`；同版本修订重提可用原号，若 AGC 要求新版本号需改 `AppScope/app.json5` 后重出（§6.17）。
- [ ] **上传场景**：选择「测试和正式上架」；上架后 AGC 会对应用重签名（官方流程，不影响升级，§6.15）。

<sub>脚注：§6.15 记录「应用名称须填 "RapidRAW"」已被 §6.17 品牌重塑取代；同理由，权限数 §6.15「8 项」→ §6.17 v3「7 项」。两份记录并存时以 §6.17 / §6.18（更新）为准。</sub>

---

## 未能在仓库中找到的事实（显式声明）

1. **隐私权利 URL** —— 仓库中无此字段（参见第 7 节）。
2. **用户协议 URL** —— 仓库中无独立用户协议文档（§6.18 明确省略）。
3. **提交截图的 1920×1080 / 16:9 分辨率要求** —— 仓库仅记录「截图 ≥3 张」，未记录分辨率；该约束来自 AGC 平台规则。
4. **鸿蒙版本是否会随包分发 NPU（`.om`）模型** —— §6.16 残留项④明确「`.om` 发布形态待定（HAP 内置 / AGC 通道分发 / 设备检测下载）」，故第 5 节只声明「支持设备端 NPU 加速」，未承诺具体供给方式。
5. **可选云端生成式服务的上线状态与订阅价格** —— README AI 路线图标为「Coming Soon」、`$TBD/month`；`zh-CN.json` 已存在云端登录 / 月用量文案。二者不一致，文案未按「已上线」表述。
6. **AGC 应用名称与 bundleName 不一致是否需要额外说明** —— 仓库未记录；本文件按 §6.17「bundleName 刻意保留」处理。
7. **应用商店展示版本号应为 1.0.0 还是 1.6.4** —— 仓库两处记录不一致（见第 4 节脚注），未找到裁决结论。
8. **鸿蒙版本是否包含相机联机拍摄 / 命令行批量导出** —— README 标注联机拍摄为 macOS & Linux、CLI 为独立导出通道；仓库未见鸿蒙端启用记录，故未列入功能文案。
