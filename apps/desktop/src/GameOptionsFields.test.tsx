import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { expect, it, vi } from 'vitest';
import GameOptionsFields from './GameOptionsFields';
import type { SoundConfiguration } from './bindings/SoundConfiguration';
import ContentConfigPreviewPanel from './ContentConfigPreviewPanel';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
const configuration: SoundConfiguration = {
  path: 'config/options.json',
  status: 'read',
  sha256: 'before',
  soundpack: null,
  sound_enabled: null,
  game_options: {
    USE_LANG: '',
    AUTOSAVE: 'true',
    AUTOSAVE_TURNS: '50',
    AUTOSAVE_MINUTES: '5',
  },
};
function Fields() {
  const [draft, setDraft] = useState<Record<string, string>>({});
  return (
    <>
      <GameOptionsFields
        configuration={configuration}
        draft={draft}
        onChange={setDraft}
      />
      <output aria-label="changed">{JSON.stringify(draft)}</output>
    </>
  );
}
it('changes only explicitly selected options and preserves empty system language', async () => {
  render(<Fields />);
  const u = userEvent.setup();
  expect(screen.getByLabelText('changed')).toHaveTextContent('{}');
  await u.click(screen.getByLabelText('修改游戏语言'));
  expect(screen.getByRole('combobox', { name: '游戏语言' })).toHaveValue('');
  await u.selectOptions(
    screen.getByRole('combobox', { name: '游戏语言' }),
    'zh_CN',
  );
  expect(screen.getByLabelText('changed')).toHaveTextContent(
    '{"USE_LANG":"zh_CN"}',
  );
  await u.click(screen.getByLabelText('修改游戏语言'));
  expect(screen.getByLabelText('changed')).toHaveTextContent('{}');
});
it('does not offer undeclared options as editable defaults', () => {
  render(
    <GameOptionsFields
      configuration={{ ...configuration, game_options: { AUTOSAVE: 'true' } }}
      draft={{}}
      onChange={vi.fn()}
    />,
  );
  expect(screen.getByLabelText('修改游戏语言')).toBeDisabled();
  expect(screen.getByLabelText('修改游戏自动保存')).toBeEnabled();
});
it('sends common options through the fresh protected preview and invalidates edited drafts', async () => {
  const report: ContentConfigReport = {
    instance_id: 'p',
    data_root: '/p',
    installation_version: '0.I',
    checked_at: 'now',
    sound: configuration,
    default_mods: {
      path: 'config/user-default-mods.json',
      status: 'missing',
      sha256: null,
      mod_ids: [],
    },
    worlds: [],
    references: [],
    issues: [],
    binding_verified: false,
    discovery: { packages: [], scopes: [], findings: [], truncated: false },
  };
  const previewContentConfig = vi.fn().mockResolvedValue({
    report,
    files: [],
    proposed_discovery: report.discovery,
    write_supported: false,
  });
  render(
    <ContentConfigPreviewPanel
      report={report}
      api={{ previewContentConfig }}
    />,
  );
  const u = userEvent.setup();
  await u.click(screen.getByLabelText('修改游戏语言'));
  await u.selectOptions(
    screen.getByRole('combobox', { name: '游戏语言' }),
    'zh_CN',
  );
  await u.click(screen.getByRole('button', { name: '比较配置草稿' }));
  expect(screen.getByLabelText('配置草稿结果')).toHaveTextContent('USE_LANG');
  await u.click(screen.getByRole('button', { name: '重新读取并生成后端预览' }));
  expect(previewContentConfig).toHaveBeenCalledWith('p', {
    default_mods: null,
    soundpack: null,
    sound_enabled: null,
    game_options: { USE_LANG: 'zh_CN' },
  });
  await screen.findByLabelText('后端配置预览');
  await u.selectOptions(
    screen.getByRole('combobox', { name: '游戏语言' }),
    'en',
  );
  expect(screen.queryByLabelText('后端配置预览')).not.toBeInTheDocument();
});
