import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import WorldsPage from './WorldsPage';
import { emptyDashboard, type HubApi } from './api';
import type { InstanceSaveReport } from './bindings/InstanceSaveReport';

const a = {
  id: 'a',
  name: 'Alpha',
  installation_id: 'game',
  installation_sha256: 'x',
  data_root: '/alpha',
  binding_status: 'unverified',
  created_at: 'now',
};
const b = { ...a, id: 'b', name: 'Beta', data_root: '/beta' };
const result = (id: string): InstanceSaveReport => ({
  instance_id: id,
  data_root: id === 'a' ? '/alpha' : '/beta',
  save_dir: '/save',
  report: {
    task_id: 'instance',
    scanner_version: 'opaque-directory-v1',
    created_at: 'now',
    enumeration_complete: true,
    visited_entries: 1,
    total_bytes: '1',
    entries: [],
    issues: [],
    candidates: [
      {
        id: 'candidate',
        relative_root: `${id}-world`,
        file_count: 1,
        entry_count: 1,
        total_bytes: '1',
        identity_confidence: 'unknown',
        boundary_verified: false,
      },
    ],
  },
});

describe('instance saves', () => {
  it('discards an old instance response after switching sources', async () => {
    let resolve!: (value: InstanceSaveReport) => void;
    const api = {
      inspectInstanceSaves: vi.fn((id: string) =>
        id === 'a'
          ? new Promise<InstanceSaveReport>((r) => {
              resolve = r;
            })
          : Promise.resolve(result(id)),
      ),
    } as unknown as HubApi;
    render(
      <WorldsPage
        api={api}
        data={{ ...emptyDashboard, play_instances: [a, b] }}
        busy={false}
        run={async (f) => f()}
        onConfigure={() => {}}
        initialInstanceId="a"
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: '读取此实例存档' }));
    await user.selectOptions(screen.getByLabelText('存档来源'), 'b');
    await user.click(screen.getByRole('button', { name: '读取此实例存档' }));
    expect(await screen.findByText('b-world')).toBeInTheDocument();
    resolve(result('a'));
    expect(screen.queryByText('a-world')).not.toBeInTheDocument();
    expect(screen.getByText('/beta/save')).toBeInTheDocument();
  });
  it('rejects a mismatched report and disables scanning with claimed resources', async () => {
    const api = {
      inspectInstanceSaves: vi.fn().mockResolvedValue(result('b')),
    } as unknown as HubApi;
    const props = {
      api,
      data: { ...emptyDashboard, play_instances: [a] },
      busy: false,
      run: async (f: () => Promise<void>) => f(),
      onConfigure: () => {},
      initialInstanceId: 'a',
    };
    const view = render(<WorldsPage {...props} />);
    await userEvent.click(
      screen.getByRole('button', { name: '读取此实例存档' }),
    );
    expect(await screen.findByRole('alert')).toHaveTextContent(
      '扫描来源不匹配',
    );
    view.rerender(<WorldsPage {...props} busy />);
    expect(
      screen.getByRole('button', { name: '读取此实例存档' }),
    ).toBeDisabled();
  });
});
