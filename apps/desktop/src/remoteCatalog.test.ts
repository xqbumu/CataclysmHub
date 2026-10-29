import { expect, it } from 'vitest';
import { assetHostHint } from './remoteCatalog';
it('uses conservative host hints and keeps generic assets unknown', () => {
  expect(assetHostHint('cdda-osx-arm64.zip', 'macos', 'aarch64')).toContain(
    '匹配本机',
  );
  expect(assetHostHint('cdda-windows-x64.zip', 'macos', 'aarch64')).toContain(
    '其他系统或架构',
  );
  expect(assetHostHint('CC-Sounds.zip', 'macos', 'aarch64')).toContain(
    '未完全声明',
  );
  expect(assetHostHint('cdda-linux-x64.tar.gz', 'linux', 'aarch64')).toContain(
    '其他系统或架构',
  );
});
