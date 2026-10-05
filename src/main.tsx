import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { installFrontendLogBridge } from './utils/frontendLogBridge';
import { installWebStorageShim } from './utils/webStorageShim';
import './styles.css';

// Must run before anything can touch web storage: ArkWeb (the OHOS webview)
// exposes localStorage as null, which throws on any access.
installWebStorageShim();
installFrontendLogBridge();

const root = createRoot(document.getElementById('root')!);
root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
