import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import WorkspaceTabs from './WorkspaceTabs';

const items = [
  {
    id: 'a',
    label: '当前',
    content: <input aria-label="草稿 A" defaultValue="" />,
  },
  {
    id: 'b',
    label: '修改',
    content: <input aria-label="草稿 B" defaultValue="" />,
  },
];
it('loads panels on demand and preserves drafts across keyboard navigation', async () => {
  const user = userEvent.setup();
  render(<WorkspaceTabs label="配置视图" items={items} />);
  expect(screen.queryByLabelText('草稿 B')).not.toBeInTheDocument();
  await user.type(screen.getByLabelText('草稿 A'), 'pending draft');
  await user.click(screen.getByRole('tab', { name: '当前' }));
  await user.keyboard('{ArrowRight}');
  expect(screen.getByRole('tab', { name: '修改' })).toHaveFocus();
  expect(screen.getByRole('tabpanel')).toHaveAccessibleName('修改');
  await user.keyboard('{Home}');
  expect(screen.getByLabelText('草稿 A')).toHaveValue('pending draft');
});
it('preserves a panel first opened through a controlled navigation change', async () => {
  const user = userEvent.setup();
  const { rerender } = render(
    <WorkspaceTabs label="视图" items={items} value="a" />,
  );
  rerender(<WorkspaceTabs label="视图" items={items} value="b" />);
  await user.type(screen.getByLabelText('草稿 B'), 'external navigation');
  rerender(<WorkspaceTabs label="视图" items={items} value="a" />);
  rerender(<WorkspaceTabs label="视图" items={items} value="b" />);
  expect(screen.getByLabelText('草稿 B')).toHaveValue('external navigation');
});
