import ContentConfigDialog from './ContentConfigDialog';
import type { ContentConfigPlan } from './bindings/ContentConfigPlan';
import { act, render, screen } from '@testing-library/react';
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

it('omits an unchanged language outside the picker list while changing autosave', async () => {
  const report: ContentConfigReport = {
    instance_id: 'p',
    data_root: '/p',
    installation_version: '0.I',
    checked_at: 'now',
    sound: {
      ...configuration,
      game_options: { USE_LANG: 'pl', AUTOSAVE: 'true' },
    },
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
  await u.click(screen.getByLabelText('修改游戏自动保存'));
  await u.selectOptions(
    screen.getByRole('combobox', { name: '游戏自动保存' }),
    'false',
  );
  await u.click(screen.getByRole('button', { name: '比较配置草稿' }));
  expect(screen.getByLabelText('配置草稿结果')).not.toHaveTextContent(
    'USE_LANG',
  );
  await u.click(screen.getByRole('button', { name: '重新读取并生成后端预览' }));
  expect(previewContentConfig).toHaveBeenCalledWith('p', {
    default_mods: null,
    soundpack: null,
    sound_enabled: null,
    game_options: { AUTOSAVE: 'false' },
  });
});

it.each(['instance', 'baseline'])(
  'discards pending drafts and previews when the %s changes',
  async (change) => {
    const a: ContentConfigReport = {
      instance_id: 'a',
      data_root: '/a',
      installation_version: '0.I',
      checked_at: 'first',
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
    const b: ContentConfigReport = {
      ...a,
      instance_id: change === 'instance' ? 'b' : 'a',
      data_root: change === 'instance' ? '/b' : '/a',
      checked_at: 'second',
      sound: {
        ...configuration,
        sha256: 'new',
        game_options: { USE_LANG: 'ja' },
      },
    };
    let complete: (plan: ContentConfigPlan) => void = () => {};
    const pending = new Promise<ContentConfigPlan>((resolve) => {
      complete = resolve;
    });
    const previewContentConfig = vi
      .fn()
      .mockReturnValueOnce(pending)
      .mockResolvedValue({
        report: b,
        files: [],
        proposed_discovery: b.discovery,
        write_supported: false,
      });
    const ui = render(
      <ContentConfigDialog
        report={a}
        api={{ previewContentConfig }}
        onClose={vi.fn()}
      />,
    );
    const u = userEvent.setup();
    await u.click(screen.getByRole('tab', { name: '修改配置' }));
    await u.click(screen.getByLabelText('修改游戏语言'));
    await u.selectOptions(
      screen.getByRole('combobox', { name: '游戏语言' }),
      'zh_CN',
    );
    await u.click(
      screen.getByRole('button', { name: '重新读取并生成后端预览' }),
    );
    ui.rerender(
      <ContentConfigDialog
        report={b}
        api={{ previewContentConfig }}
        onClose={vi.fn()}
      />,
    );
    await u.click(screen.getByRole('tab', { name: '修改配置' }));
    expect(screen.getByLabelText('修改游戏语言')).not.toBeChecked();
    await act(async () => {
      complete({
        report: a,
        files: [],
        proposed_discovery: a.discovery,
        write_supported: false,
      });
      await pending;
    });
    expect(screen.queryByLabelText('后端配置预览')).not.toBeInTheDocument();
    await u.click(screen.getByLabelText('修改游戏语言'));
    expect(screen.getByRole('combobox', { name: '游戏语言' })).toHaveValue(
      'ja',
    );
    await u.selectOptions(
      screen.getByRole('combobox', { name: '游戏语言' }),
      'en',
    );
    await u.click(
      screen.getByRole('button', { name: '重新读取并生成后端预览' }),
    );
    expect(previewContentConfig).toHaveBeenLastCalledWith(b.instance_id, {
      default_mods: null,
      soundpack: null,
      sound_enabled: null,
      game_options: { USE_LANG: 'en' },
    });
  },
);
