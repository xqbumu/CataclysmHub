import { invoke, isTauri } from '@tauri-apps/api/core';
export interface RemoteSource {
  id: string;
  name: string;
  kind: string;
  repo: string;
}
export interface RemoteAsset {
  id: number;
  name: string;
  size: number;
  url: string;
  digest: string | null;
}
export interface RemoteRelease {
  tag: string;
  prerelease: boolean;
  published: string;
  assets: RemoteAsset[];
}
export interface RemoteListing {
  attempted: boolean;
  updated: string | null;
  error: string | null;
  releases: RemoteRelease[];
}
export interface RemoteDownload {
  repo?: string;
  url?: string;
  asset_id?: number;
  downloaded_at?: string;
  id: string;
  source: string;
  tag: string;
  name: string;
  path: string;
  size: number;
  sha256: string;
  upstream_verified: boolean;
}
export interface RemoteView {
  sources: RemoteSource[];
  catalog: {
    listings: Record<string, RemoteListing>;
    downloads: RemoteDownload[];
  };
}
export interface RemoteTransfer {
  name: string;
  received: number;
  total: number;
}
export interface RemoteApi {
  progress(): Promise<RemoteTransfer | null>;
  cancel(): Promise<void>;
  view(): Promise<RemoteView>;
  remove(id: string): Promise<RemoteView>;
  sync(id: string, manual: boolean): Promise<RemoteView>;
  download(
    source: string,
    tag: string,
    assetId: number,
  ): Promise<RemoteDownload>;
}
export const remoteApi: RemoteApi = {
  progress: () => invoke('remote_download_progress'),
  cancel: () => invoke('cancel_remote_download'),
  remove: (id) => invoke('remove_remote_download', { id }),
  view: () =>
    isTauri()
      ? invoke('remote_catalog')
      : Promise.resolve({
          sources: [],
          catalog: { listings: {}, downloads: [] },
        }),
  sync: (id, manual) => invoke('sync_remote_source', { id, manual }),
  download: (source, tag, assetId) =>
    invoke('download_remote_asset', { source, tag, assetId }),
};
let initialSync: Promise<void> | undefined;
/** Once per app session; the backend also persists each first attempt across restarts. */
export function syncInitialGameCatalog() {
  if (!isTauri()) return Promise.resolve();
  initialSync ??= (async () => {
    const view = await remoteApi.view();
    for (const source of view.sources.filter((s) => s.kind === 'game')) {
      if (!view.catalog.listings[source.id]?.attempted)
        await remoteApi.sync(source.id, false);
    }
  })().catch(() => {
    /* The page exposes cached failures and explicit retry. */
  });
  return initialSync;
}
