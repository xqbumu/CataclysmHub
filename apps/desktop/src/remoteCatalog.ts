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
export interface RemoteExtraction {
  source: string;
  name: string;
  tag: string;
  id: string;
  download_id: string;
  archive_sha256: string;
  path: string;
  entries: number;
  bytes: number;
  extracted_at: string;
}
export interface RemoteView {
  host_os?: string;
  host_arch?: string;
  sources: RemoteSource[];
  catalog: {
    listings: Record<string, RemoteListing>;
    downloads: RemoteDownload[];
    extractions?: RemoteExtraction[];
  };
}
export interface RemoteTransfer {
  name: string;
  received: number;
  total: number;
}
export interface RemoteApi {
  lookup(id: string, tag: string): Promise<RemoteView>;
  openExtraction(id: string): Promise<void>;
  pickExtraction(
    id: string,
    kind: 'directory' | 'executable',
  ): Promise<string | null>;
  extract(id: string): Promise<RemoteExtraction>;
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
  lookup: (id, tag) => invoke('lookup_remote_release', { id, tag }),
  openExtraction: (id) => invoke('open_remote_extraction', { id }),
  pickExtraction: (extractionId, kind) =>
    invoke('pick_path', { kind, extractionId }),
  extract: (id) => invoke('extract_remote_download', { id }),
  progress: () =>
    isTauri() ? invoke('remote_download_progress') : Promise.resolve(null),
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

/** Filename hints only; never a compatibility certificate or an automatic asset choice. */
export function assetHostHint(name: string, os?: string, arch?: string) {
  const n = name.toLowerCase();
  const platform = /(?:macos|osx|darwin|mac-)/.test(n)
    ? 'macos'
    : /(?:windows|win32|win64|msvc|mingw)/.test(n)
      ? 'windows'
      : /(?:linux|appimage)/.test(n)
        ? 'linux'
        : undefined;
  const cpu = /(?:aarch64|arm64)/.test(n)
    ? 'aarch64'
    : /(?:x86_64|x64|amd64)/.test(n)
      ? 'x86_64'
      : /(?:i686|i386|x86-)/.test(n)
        ? 'x86'
        : undefined;
  if ((platform && os && platform !== os) || (cpu && arch && cpu !== arch))
    return '文件名提示：其他系统或架构';
  if (platform && cpu && os && arch) return '文件名提示：匹配本机';
  return '系统或架构未完全声明，请核对';
}
export function matchesDownload(
  d: RemoteDownload,
  source: RemoteSource,
  release: RemoteRelease,
  asset: RemoteAsset,
) {
  return (
    d.source === source.id &&
    d.repo === source.repo &&
    d.tag === release.tag &&
    d.asset_id === asset.id &&
    d.url === asset.url &&
    d.name === asset.name
  );
}
