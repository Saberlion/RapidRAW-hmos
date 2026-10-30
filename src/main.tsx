import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { installExternalLinkHandler } from './utils/externalLinks';
import { installFrontendLogBridge } from './utils/frontendLogBridge';
import { installWebStorageShim } from './utils/webStorageShim';
import './styles.css';

// Must run before anything can touch web storage: ArkWeb (the OHOS webview)
// exposes localStorage as null, which throws on any access.
installWebStorageShim();
installFrontendLogBridge();
// Claims external link clicks before tauri-plugin-shell's injected script can
// route them into an open() that cannot work on OHOS (docs 6.19).
installExternalLinkHandler();

const root = createRoot(document.getElementById('root')!);
root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
