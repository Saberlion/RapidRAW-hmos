import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import clsx from 'clsx';

import Button from './ui/Button';
import Text from './ui/Text';
import { TextColors, TextVariants, TextWeights } from '../types/typography';
import { AppSettings, Invokes } from './ui/AppProperties';

/**
 * Bump to force a fresh consent prompt for all users (e.g. after a material
 * policy change). The accepted value is persisted in app settings as
 * `agreementAcceptedVersion`.
 */
export const AGREEMENT_VERSION = '1.0';

export type AgreementDocKind = 'privacy' | 'terms';

// The legal documents are deliberately NOT i18n JSON entries: they are legal
// texts authored once per script (zh-Hans / zh-Hant / en) and selected by
// language prefix. The 15-locale sync check only governs UI chrome strings.
const PRIVACY_DOCS: Record<string, string> = {
  'zh-CN': `RapidRAW 隐私政策
更新日期:2026 年 10 月 6 日

一、总则
RapidRAW 是一款以本地处理为核心的开源照片编辑器。我们高度重视您的隐私:本应用没有用户账号系统,不包含任何遥测或使用统计代码。

二、我们收集哪些信息
RapidRAW 不收集、不上传、不分享任何个人身份信息。

三、本地数据
您的照片、编辑参数(.rrdata 边车文件)、缩略图缓存与应用设置均仅保存在您的设备本地。卸载应用即删除上述应用数据;您的照片文件不受影响。

四、权限使用说明
· 文件与媒体访问:仅在您主动选择文件夹或导入照片时,用于读取您指定的内容;
· 保存到相册:仅在您主动导出时,用于写入您指定的图片;
· 网络访问:仅用于按需功能——首次使用 AI 功能时从开源模型镜像站下载模型文件到本地,以及打开您主动点击的外部链接。应用启动时不发起任何网络请求。

五、可选云服务
云端 AI 扩展为可选功能,仅在您明确配置后才会连接对应第三方服务,相关数据处理受该服务条款约束。

六、开源许可
本应用基于 AGPL-3.0 许可证开源,源代码链接见设置页"关于"部分。

七、政策变更
本政策如有重大变更,将在应用内重新弹窗征求您的同意。

八、联系我们
请通过 GitHub 仓库的 Issues 页面与我们联系。`,
  'zh-TW': `RapidRAW 隱私政策
更新日期:2026 年 10 月 6 日

一、總則
RapidRAW 是一款以本機處理為核心的開源照片編輯器。我們高度重視您的隱私:本應用沒有使用者帳號系統,不包含任何遙測或使用統計程式碼。

二、我們收集哪些資訊
RapidRAW 不收集、不上傳、不分享任何個人身分資訊。

三、本機資料
您的照片、編輯參數(.rrdata 邊車檔案)、縮圖快取與應用程式設定均僅儲存在您的裝置本機。解除安裝應用程式即刪除上述資料;您的照片檔案不受影響。

四、權限使用說明
· 檔案與媒體存取:僅在您主動選擇資料夾或匯入照片時,用於讀取您指定的內容;
· 儲存至相簿:僅在您主動匯出時,用於寫入您指定的圖片;
· 網路存取:僅用於按需功能——首次使用 AI 功能時從開源模型鏡像站下載模型檔案到本機,以及開啟您主動點擊的外部連結。應用程式啟動時不發起任何網路請求。

五、可選雲端服務
雲端 AI 擴充為可選功能,僅在您明確設定後才會連線對應第三方服務,相關資料處理受該服務條款約束。

六、開源許可
本應用基於 AGPL-3.0 許可證開源,原始碼連結見設定頁「關於」部分。

七、政策變更
本政策如有重大變更,將在應用內重新彈窗徵求您的同意。

八、聯絡我們
請透過 GitHub 儲存庫的 Issues 頁面與我們聯絡。`,
  en: `RapidRAW Privacy Policy
Last updated: October 6, 2026

1. Overview
RapidRAW is an open-source, local-first photo editor. We take your privacy seriously: the app has no account system and contains no telemetry or usage tracking.

2. Information we collect
RapidRAW does not collect, upload, or share any personal information.

3. Local data
Your photos, edit parameters (.rrdata sidecar files), thumbnail caches, and app settings are stored only on your device. Uninstalling the app removes this app data; your photo files are untouched.

4. Permissions
- Files & media: read only for the folders and photos you explicitly select.
- Save to gallery: writes only the images you explicitly export.
- Network: used solely by on-demand features - downloading AI model files from an open-source model mirror the first time an AI feature is used, and opening links you click. No network request is made at app startup.

5. Optional cloud services
Cloud AI extensions are optional and connect to third-party services only after you explicitly configure them; their handling of your data is governed by those services' terms.

6. Open source
This app is licensed under AGPL-3.0. See the About section in Settings for the source code link.

7. Changes to this policy
Material changes will be presented in-app for your renewed consent.

8. Contact
Reach us via the GitHub repository's Issues page.`,
};

const TERMS_DOCS: Record<string, string> = {
  'zh-CN': `RapidRAW 用户协议
更新日期:2026 年 10 月 6 日

一、协议的接受
点击"同意并继续"或继续使用 RapidRAW,即表示您已阅读并同意本协议与《隐私政策》。

二、开源许可
本应用以 GNU Affero 通用公共许可证第 3.0 版(AGPL-3.0)分发。您可以在遵守该许可证的前提下自由使用、研究、修改和再分发本软件。完整许可证文本:https://www.gnu.org/licenses/agpl-3.0.html

三、免责声明
本软件按"现状"提供,不作任何明示或默示的担保。对因使用本软件导致的任何直接或间接损失(包括数据丢失),作者不承担责任。请在对重要照片进行操作前自行做好备份。

四、用户内容
您的照片与编辑数据完全归您所有,本应用不主张任何权利。

五、第三方组件
本应用包含多个开源组件,其版权归各自作者所有并遵循其各自许可证,清单见源代码仓库。

六、协议变更
本协议如有重大变更,将在应用内重新弹窗征求您的同意。继续使用即视为接受变更后的协议。`,
  'zh-TW': `RapidRAW 使用者協議
更新日期:2026 年 10 月 6 日

一、協議的接受
點擊「同意並繼續」或繼續使用 RapidRAW,即表示您已閱讀並同意本協議與《隱私政策》。

二、開源許可
本應用以 GNU Affero 通用公共授權條款第 3.0 版(AGPL-3.0)散布。您可以在遵守該授權條款的前提下自由使用、研究、修改和再散布本軟體。完整授權條款文本:https://www.gnu.org/licenses/agpl-3.0.html

三、免責聲明
本軟體按「現狀」提供,不作任何明示或默示的擔保。對因使用本軟體導致的任何直接或間接損失(包括資料遺失),作者不承擔責任。請在對重要照片進行操作前自行做好備份。

四、使用者內容
您的照片與編輯資料完全歸您所有,本應用不主張任何權利。

五、第三方元件
本應用包含多個開源元件,其版權歸各自作者所有並遵循其各自授權條款,清單見原始碼儲存庫。

六、協議變更
本協議如有重大變更,將在應用內重新彈窗徵求您的同意。繼續使用即視為接受變更後的協議。`,
  en: `RapidRAW User Agreement
Last updated: October 6, 2026

1. Acceptance
By tapping "Agree & Continue" or using RapidRAW, you accept this agreement and the Privacy Policy.

2. Open-source license
This app is distributed under the GNU Affero General Public License v3.0 (AGPL-3.0). You may use, study, modify, and redistribute it under the terms of that license. Full text: https://www.gnu.org/licenses/agpl-3.0.html

3. Disclaimer of warranty
The software is provided "as is", without warranty of any kind. The authors are not liable for any direct or indirect damages, including data loss. Back up important photos before editing.

4. Your content
Your photos and edit data remain entirely yours; the app claims no rights over them.

5. Third-party components
The app bundles several open-source components, each owned by its authors and governed by its own license; see the source repository for the list.

6. Changes to this agreement
Material changes will be presented in-app for your renewed consent. Continued use constitutes acceptance.`,
};

export function getAgreementDocument(kind: AgreementDocKind, language: string): string {
  const docs = kind === 'privacy' ? PRIVACY_DOCS : TERMS_DOCS;
  if (language === 'zh-TW' || language === 'zh-HK') return docs['zh-TW'];
  if (language.startsWith('zh')) return docs['zh-CN'];
  return docs.en;
}

interface AgreementGateProps {
  appSettings: AppSettings;
  onAgree: (settings: AppSettings) => void;
}

/**
 * First-launch consent gate (app-store compliance). Rendered by App instead
 * of the whole main UI until the user accepts the current AGREEMENT_VERSION;
 * the unmounted main tree guarantees no network or file activity happens
 * before consent.
 */
export default function AgreementGate({ appSettings, onAgree }: AgreementGateProps) {
  const { t, i18n } = useTranslation();
  const [activeDoc, setActiveDoc] = useState<AgreementDocKind>('privacy');
  const [showExitConfirm, setShowExitConfirm] = useState(false);

  const docText = getAgreementDocument(activeDoc, i18n.language);

  const handleAgree = () => {
    onAgree({ ...appSettings, agreementAcceptedVersion: AGREEMENT_VERSION });
  };

  const handleExit = () => {
    invoke(Invokes.ExitApp).catch(() => window.close());
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-bg-primary text-text-primary font-sans select-none">
      <div className="pt-10 pb-3 px-6 text-center shrink-0">
        <Text variant={TextVariants.title} color={TextColors.accent} weight={TextWeights.bold}>
          RapidRAW
        </Text>
        <div className="mt-2">
          <Text variant={TextVariants.heading}>{t('ui.agreement.title')}</Text>
        </div>
        <div className="mt-1">
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {t('ui.agreement.subtitle')}
          </Text>
        </div>
      </div>

      <div className="flex justify-center gap-2 shrink-0 px-6">
        {(['privacy', 'terms'] as const).map((kind) => (
          <button
            key={kind}
            type="button"
            onClick={() => setActiveDoc(kind)}
            className={clsx(
              'px-4 py-1.5 rounded-md text-sm font-medium transition-colors cursor-pointer',
              activeDoc === kind
                ? 'bg-accent text-button-text'
                : 'bg-bg-tertiary text-text-secondary hover:text-text-primary',
            )}
          >
            {t(kind === 'privacy' ? 'ui.agreement.privacyTab' : 'ui.agreement.termsTab')}
          </button>
        ))}
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar w-full max-w-3xl mx-auto px-6 py-4 my-3">
        <Text className="whitespace-pre-wrap leading-relaxed">{docText}</Text>
      </div>

      <div className="shrink-0 px-6 py-5 border-t border-border-color flex justify-center">
        <div className="flex items-center gap-3">
          <Button className="bg-surface" onClick={() => setShowExitConfirm(true)}>
            {t('ui.agreement.decline')}
          </Button>
          <Button onClick={handleAgree}>{t('ui.agreement.agree')}</Button>
        </div>
      </div>

      {showExitConfirm && (
        <div className="absolute inset-0 z-[60] flex items-center justify-center bg-black/60 p-6">
          <div className="bg-surface rounded-xl p-6 w-full max-w-sm shadow-lg">
            <Text variant={TextVariants.heading} className="mb-2">
              {t('ui.agreement.exitTitle')}
            </Text>
            <Text variant={TextVariants.small} color={TextColors.secondary} className="mb-6">
              {t('ui.agreement.exitMessage')}
            </Text>
            <div className="flex justify-end gap-3">
              <Button className="bg-surface" onClick={() => setShowExitConfirm(false)}>
                {t('ui.agreement.back')}
              </Button>
              <Button onClick={handleExit}>{t('ui.agreement.confirmExit')}</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
