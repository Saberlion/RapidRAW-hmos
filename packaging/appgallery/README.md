# AppGallery 上架素材（LightRaw · PC/2in1）

本目录是 HarmonyOS 应用 **LightRaw**（bundle `io.github.CyberTimon.RapidRAW`）在 AGC 上架时「应用详情页」所需的展示素材与文字材料，**全部由真机截图加工而来**，不是设计稿。

## 1. 文件清单

| 路径 | 内容 | 对应 AGC 字段 |
| --- | --- | --- |
| `screenshots/01..05-*.png` | 应用介绍截图，5 张，1920×1080 16:9 PNG | 应用介绍截图（3~5 张） |
| `screenshots/_alt-16x9-crop/` | 同一批截图的**满幅裁切**备选版（见 §4） | 备选，通常不用 |
| `icon/icon-1024.png`、`icon/icon-216.png` | 应用图标（1024 / 216，正方形，不透明） | 应用图标（216×216 或 1024×1024） |
| `listing-copy.md` | 应用简介 / 详细描述 / 更新说明 / AI 功能声明 / 权限使用理由 / 隐私 URL / 上架前置清单 | 应用信息各文字字段 |
| `_preview/*.jpeg` | 两版截图的拼版预览（给人工过目用） | — |
| `tools/` | 生成脚本（见 §5），可复现本次全部产物 | — |

## 2. 规格核对（AGC「PC / 2-in-1」）

| 项目 | AGC 要求 | 本次产物 | 结论 |
| --- | --- | --- | --- |
| 应用图标 | 1 张，216×216 或 1024×1024，正方形，PNG ≤3MB | `icon-216.png` 47KB / `icon-1024.png` 791KB，1024×1024 不透明 | ✅ |
| 应用介绍截图 | 3~5 张，16:9，1920×1080，PNG/JPG/JPEG ≤5MB | 5 张，1920×1080，PNG 207~573KB | ✅ |
| 名称/图标与包内一致 | 必须一致 | 名称 **LightRaw**（包内 `app_name` / `EntryAbility_label`）、图标取包内 `foreground.png`（= `LightRaw.png`） | ✅ |
| 支持设备 | 包内 `deviceTypes = ["2in1"]` → 控制台只勾 2in1 | — | ⚠️ 需在控制台勾选 |

## 3. 截图内容与建议顺序

设备为 **2in1 / 3120×2080**，app 以最大化的深色主题窗口截图；系统装饰栏（顶部 70px）与系统任务栏（底部 125px）已裁掉。

| 文件名 | 界面 | 说明 |
| --- | --- | --- |
| `01-editor-adjust.png` | 编辑器 + 「调整」面板 | 主视觉：哈苏 X2D RAW（`2022.6.14_Swiss_Linksphotograph.fff`，23s f/5.6 ISO64 43mm）预览 + 右侧调整面板 |
| `02-library.png` | 图库 | 文件夹「来源」树 + 缩略图网格 + 评级/EXIF 卡片 |
| `03-masks.png` | 「蒙版」面板 | 蒙版栈与蒙版工具入口 |
| `04-ai-retouch.png` | 「图像修复」面板 | AI 编辑（该 RAW 的 `.rrdata` 侧车自带一个「AI 编辑 1 / 前景」主体蒙版） |
| `05-presets.png` | 「预设」面板 | 预设库 |

建议按上表顺序上传（第 1 张即详情页首图）。若要替换：编辑器的「裁剪与变换」面板截图、以及「欢迎回来」欢迎页截图这次也已采集过（见 §5 复现配方），可随时补做。

## 4. 16:9 处理方式（为什么有两条命）

设备是 3:2（3120×2080），AGC 要 16:9 1920×1080，二者不可能同时满足，因此有两种做法：

- **主版本（`screenshots/`）：等比缩放到高 1080，两侧各补 66px 同色边。**
  补边色取自画面四角的实测色（`RGB(27,27,27)`，四角色差仅 **1/255**），边框在视觉上不可见；代价是内容宽度为 1787px（约 93%）。**不损失任何界面元素。**
- **备选版（`screenshots/_alt-16x9-crop/`）：满幅 1920 宽裁切，往下裁掉 app 窗口底部 130px。**
  更锐利、无边框，但会切掉底部胶片条缩略带的下沿约 73px（元素边界实测 `[645,1599][817,1828]`），故不作为主版本。

## 5. 复现配方（真机 → 素材）

前置：`hdc` 在 PATH、设备已装 debug 签名的 LightRaw 包（本机设备 `3XB0124617000013`）。

```powershell
# 1) 让 app 用深色主题（AGC 素材观感更好；app 默认 theme=system，本机系统是浅色）
hdc shell "aa force-stop io.github.CyberTimon.RapidRAW"
# 拉出 settings.json → 把 "theme" 改成 "dark" → 推回
hdc file recv /data/app/el2/100/base/io.github.CyberTimon.RapidRAW/files/appdata/settings.json .
hdc file send settings.json /data/app/el2/100/base/io.github.CyberTimon.RapidRAW/files/appdata/settings.json
hdc shell "aa start -a EntryAbility -b io.github.CyberTimon.RapidRAW"

# 2) 驱动 UI：可访问性树能读到 ArkWeb 页面内的文本与坐标（无需看图即可定位/点击）
hdc shell "uitest dumpLayout -p /data/local/tmp/layout.json"   # 默认只合并本应用窗口
hdc file recv /data/local/tmp/layout.json .
#   加 -i 才是「全窗口」模式（返回窗口数组）；系统 picker 属独立窗口，须 -i
#   单点 click = 选中，doubleClick = 打开图片进编辑器
hdc shell "uitest uiInput doubleClick 1190 436"

# 3) 截图（整屏 3120×2080）
hdc shell "snapshot_display -f /data/local/tmp/s.jpeg"
hdc file recv /data/local/tmp/s.jpeg .
```

`tools/` 三个脚本：

- `ui.py` —— 设备驱动（`dump` / `dumpall` / `treefile` / `images` / `find` / `click` / `click-xy` / `dclick-xy` / `key` / `shot`）。
- `ascii.py` —— 把截图降采样成字符画（本机模型无法读图时用来核对版面）。
- `build_assets.py` —— 把 `tools/raw/*.jpeg` 加工成 `screenshots/`、`_alt-16x9-crop/`、`icon/`、`_preview/`。**把原始整屏截图放进 `tools/raw/` 后运行即可**（文件名见脚本里的 `SHOTS` 表）。原始截图未入库（体积大且含系统任务栏），需要重做时按上面的 §5 重新采集。

关键坐标（3120×2080、窗口最大化时）：右侧面板切换器为竖排 5 个 69px 图标，x≈3060，y 依次 **97=调整、173=裁剪、249=蒙版、325=图像修复、401=预设**；窗口控制按钮在 `y≈316`：`x2388` 最大化 / `2464` 最小化 / `2540` 关闭。

## 6. 已知事项 / 待确认

- **隐私协议链接已更新为** `https://agreement-drcn.hispace.dbankcloud.cn/index.html?lang=zh&agreementId=2068880948149116608`（2026-10-25 提供）：
  - `listing-copy.md` §7 与清单已同步；
  - `src/utils/ohosPrivacy.ts` 的兜底常量 `PRIVACY_STATEMENT_AGREEMENT_ID` 已改为 `2068880948149116608`（⚠️ 应用内生效需重新出包；平台下发链接时该常量本就只作兜底）；
  - `docs/HARMONYOS_PORTING.md` §6.18 仍保留旧值 `2053968770484970432` 作为历史记录，尚未追加更新说明。
- **设备状态**：截图期间把 app 主题临时改成 `dark` 以出片；**现已还原为 `system`** 并重启 app（`settings.json` 的 `theme` 字段）。
- `tools/`（驱动/降采样/加工脚本）与 `screenshots/_alt-16x9-crop/`（备选裁切版）按约定保留，便于下个版本重拍。
- **图库里是测试样片**：`rapidraw_test.png` / `fresh_cann_test.jpg` 文件名带测试色彩（截图里只体现为卡片文字）。若上架前想要更干净的画面，可换样片后按 §5 重拍。
- **截图未包含**：系统文件选择器、隐私声明弹窗、欢迎页；如审核要求佐证某个流程，欢迎页那张可补做。
- **AI 声明口径**：`listing-copy.md` §5 只声明到「支持本机 NPU 加速」，因为仓库对 `.om`（NPU 模型）的发版形态尚无定论（§6.16 残留 ④）。
