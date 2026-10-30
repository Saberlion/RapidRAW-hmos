import { invoke } from '@tauri-apps/api/core';
import { Invokes } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';

/**
 * Withdraw OpenHarmony folder grants.
 *
 * A folder picked through FileKit is persisted (`fileShare.persistPermission`)
 * and recorded in the ability's `ohos_grants.json`; every launch re-activates
 * every recorded URI before the library is read. Removing a library root in the
 * UI only dropped the path from the frontend store, so the platform permission
 * survived and the next launch brought it back — "removing the folder revoked
 * nothing" (docs 6.20). This command asks the ArkTS side to deactivate the
 * permission *and* drop the record, so the user has to re-pick the folder.
 *
 * @param identifiers picker URIs or resolved paths to withdraw; `null` (the
 * default) withdraws every recorded grant — used by the privacy revoke, whose
 * confirmation promises that file access has to be re-authorized.
 *
 * No-op on other platforms (which have no such grants) and never throws: the
 * failure is logged and forwarded to app.log by the log bridge.
 */
export async function revokeOhosFolderGrants(identifiers: string[] | null = null): Promise<void> {
  if (!useSettingsStore.getState().isOhos) {
    return;
  }

  try {
    await invoke(Invokes.RevokeOhosFolderGrants, { identifiers });
  } catch (err) {
    console.error('Failed to revoke OHOS folder grants', identifiers, err);
  }
}
