# darktable-ai 算法分析

> 分析日期:2026-10-30 · 依据:github.com/darktable-org/darktable-ai(master,9 个模型卡逐一核实)+ darktable 5.6.0 发布说明。
> 定位:darktable 官方 AI 模型仓库(GPL-3.0),配合 darktable 5.6 的 `-DUSE_AI=ON` AI 子系统,ONNX Runtime 后端、全本地推理。与本仓(RapidRAW/LightRaw)是同类应用,其选型对本仓有直接参照价值。

---

## 1. 模型清单(9 个模型卡,5 类任务)

| 任务 | 模型 | 算法/架构 | IO(全部静态烘焙 + 滑窗 tile) |
|---|---|---|---|
| 降噪 | `denoise-nind` | 标准 U-Net(3→64→128→256→512→512,sigmoid 输出),NIND 数据集(Wikimedia 真实噪声,arXiv:1906.00270) | `[1,3,768,768]` f32 → 同 |
| 降噪 | `denoise-nafnet` | NAFNet width-32(无非线性激活网络,ECCV 2022),SIDD | `[1,3,768,768]` |
| RAW 降噪 | `rawdenoise-nind` | UtNet2(4-pool U-Net,funit=32,LeakyReLU),双变体按传感器自动分派:**Bayer** 4ch→3ch@2×(Conv1×1+PixelShuffle,联合去噪+去马赛克,输出 camRGB 需后接色彩矩阵与增益匹配);**Linear** 3ch→3ch(线性 Rec.2020) | Bayer `[1,4,512,512]→[1,3,1024,1024]`;Linear `[1,3,512,512]` |
| 交互遮罩 | `mask-object-sam21-{tiny,small,base-plus}` | SAM 2.1(Hiera 分层编码器 + mask decoder,高分辨率特征投影烘进编码器),多掩码输出(3 候选按 IoU 选) | encoder `[1,3,1024,1024]` → 3 路特征;decoder 点提示+上次掩码 → `[1,3,1024,1024]` |
| 交互遮罩 | `mask-object-segnext-b2hq` | SegNext(CVPR 2024):MAE 预训练 ViT-B + 稀疏窗口注意力 + SAx2 自注意力提示融合 + SimpleFPN;HQSeg-44K 微调(高质量边界) | encoder `[1,3,1024,1024]→[1,768,64,64]`;decoder 点+prev_mask → 1024² 掩码 |
| 超分 | `upscale-realplksr` | RealPLKSR(~7M 参数,部分大核 17×17 CNN + DySample 上采样;MSSIM 预训练,**无 GAN 微调**,避免纹理幻觉) | x2: 512²→1024²;x4: 256²→1024² |
| 超分 | `upscale-bsrgan` | RRDBNet(23 个残差套残差密集块,64base/32growth),实用退化模型(ICCV 2021) | 同上 |
| 嵌入 | `embedding-openclip-rn101-yfcc15m` | CLIP 变体 ResNet-101(抗混叠池化 + attention pool),YFCC15M(唯一全授权 CLIP 训练集);仅图像编码器,FP16 ~60MB,mean/std 与 L2 归一化烘进图 | 224² → 512d |

darktable 5.6 集成:**neural restore 模块**(raw denoise / denoise / upscale 三任务;默认 NIND UNet、RawNIND UtNet2、RealPLKSR,NAFNet/BSRGAN 手动安装)+ Lua `darktable.ai` API(张量/模型加载/GPU provider,可跑社区自定义模型)。

## 2. 选型哲学

1. **修复类全用纯 CNN**(U-Net/NAFNet/RRDB/PLKSR),不上 SCUNet 类 transformer;RealPLKSR 特意停在 MSSIM 预训练阶段,宁保守不幻觉。
2. **遮罩类才用 transformer**(SAM 2.1、SegNext ViT-B)——交互式分割质量优先,且 encoder/decoder 拆分导出控制内存。
3. **许可与隐私一等公民**:GPL-3.0 兼容 + OSAID v1.0/MOF 分类 + 训练数据逐项审计;CLIP 为此牺牲精度选 YFCC15M(zero-shot 31% vs LAION 系 67%),因其为唯一"每张图都经作者授权"的 CLIP 训练集;全部本地推理。

## 3. 与本仓交叉对照

| 维度 | darktable-ai | 本仓 v1.6.7 | 备注 |
|---|---|---|---|
| 降噪 | NIND UNet 同源(trougnouf/nind-denoise,他们 tile 768) | 同一模型族 `nind_denoise_utnet_684`,tile 504 | 本仓已上 NPU(`nind_v2.om`,PSNR 69.5 dB) |
| NAFNet | 可选手动安装 | 评估后拒绝(SIDD 手机噪声,DSLR 每档 ISO 垫底) | 两边结论一致:非主力 |
| RawNIND UtNet2 | 正式发布 | 文档 §8.2 记录 Bayer 变体 fp16 溢出 | **互相印证**:其模型卡明确 `cpu_only: model_bayer: [coreml]` |
| 交互遮罩 | SAM 2.1(Hiera)+ SegNext | SAM ViT-b 一代(95.6MB)+ BiRefNet/ESAM3 | darktable 更新一代且更轻 |
| 自动打标 | 60MB FP16 RN101 + 嵌入质心(用户标签均值→建议 + 相似搜索,无文本编码器) | 577MB 完整 CLIP(图文双塔) | 内存受限的 OHOS 设备可借鉴"仅图像塔" |
| AI 超分 | RealPLKSR / BSRGAN | **没有** | 功能位缺口 |
| 独有 | — | BiRefNet 前景、ESAM3 天空/文本/人脸、MediaPipe 人脸、DepthAnything、MoGe-2 法线、LaMa 修补 | 本仓面更宽 |

## 4. 工程模式对照(与本仓高度一致)

- 静态 shape 烘焙 + manifest 声明 tile 尺寸 + 滑窗——与本仓 `.om` 504 tile 同构(他们为 JIT EP 一次性编译成本,本仓为 CANN 静态维度约束)。
- encoder/decoder 拆分导出(SAM 2.1、SegNext)——与本仓 SAM/ESAM3 相同。
- 多变体单包分派(Bayer/Linear 按 `raw_pattern` 自动选)——对应本仓未来 RAW 域按需分发。

## 5. 对本仓的启示

1. **AI 超分是最明确的可借鉴项**:RealPLKSR 纯 CNN、静态 shape、无 GatherND/动态 shape/int64——落在 CANN 可行域(`docs/CANN_MODEL_MIGRATION.md` §6.4),是 NPU 落地的下一个低风险候选(定位类似 face_landmarks)。
2. **SAM 2.1-small** 值得评估替换 SAM ViT-b:更新一代、更轻,decoder 拆分模式与本仓管线兼容(SAM3 移植状态另见 `docs/SAM3_PORTING_STATUS.md`)。
3. **CLIP 瘦身方向**:仅图像编码器 + 用户标签质心,可把 OHOS 打标内存从 578MB 降至 ~60MB 量级。
4. **fp16 溢出教训共享**:其 Bayer 变体 `cpu_only` 与本仓 omg `--weight_data_type FP16(默认)` 的发现互相印证;若做 RAW 域降噪 `.om`,Bayer 变体需 `--weight_data_type FP32`(SAM3 decoder 战役同样踩过 fp16 GemmD dtype 坑)。
