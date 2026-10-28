import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import PlayInstancesPanel from './PlayInstancesPanel';
import { api, emptyDashboard } from './api';

it('requires a native selection and keeps binding uncertainty explicit', async () => {
  const user = userEvent.setup();
  const registerPlayInstance = vi.fn().mockResolvedValue({});
  const onRefresh = vi.fn().mockResolvedValue(undefined);
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        pickPath: vi.fn().mockResolvedValue('/isolated'),
        registerPlayInstance,
      }}
      data={{
        ...emptyDashboard,
        installations: [
          {
            id: 'game',
            name: 'Game A',
            executable: '/game/exe',
            working_dir: '/game',
            sha256: 'abc',
            version: null,
            branch: null,
            platform: 'test',
            arch: 'test',
            imported_at: '',
          },
        ],
      }}
      onRefresh={onRefresh}
    />,
  );
  expect(screen.getByText(/并行启动尚未开放/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '登记实例' })).toBeDisabled();
  await user.selectOptions(screen.getByLabelText('关联安装'), 'game');
  await user.type(screen.getByLabelText('实例名称'), '独立世界');
  await user.click(screen.getByRole('button', { name: '选择实例空目录' }));
  await user.click(screen.getByRole('button', { name: '登记实例' }));
  expect(registerPlayInstance).toHaveBeenCalledWith({
    installation_id: 'game',
    name: '独立世界',
    data_root: '/isolated',
  });
  expect(onRefresh).toHaveBeenCalled();
});
