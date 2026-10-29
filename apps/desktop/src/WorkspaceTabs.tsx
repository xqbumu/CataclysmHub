import { useEffect, useId, useState, type ReactNode } from 'react';

/** Visited panels stay mounted so switching views cannot discard an in-flight task. */
export default function WorkspaceTabs({
  label,
  items,
  value,
  onChange,
}: {
  label: string;
  items: { id: string; label: string; content: ReactNode }[];
  value?: string;
  onChange?: (id: string) => void;
}) {
  const prefix = useId();
  const [local, setLocal] = useState(items[0].id);
  const selected = value ?? local;
  const [visited, setVisited] = useState<Set<string>>(
    () => new Set([selected]),
  );
  useEffect(() => {
    setVisited((old) =>
      old.has(selected) ? old : new Set([...old, selected]),
    );
  }, [selected]);
  function select(id: string) {
    setVisited((old) => new Set([...old, id]));
    setLocal(id);
    onChange?.(id);
  }
  return (
    <div className="workspace-tabs">
      <div className="view-tabs" role="tablist" aria-label={label}>
        {items.map((item, index) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            id={`${prefix}-${item.id}-tab`}
            aria-controls={`${prefix}-${item.id}-panel`}
            aria-selected={selected === item.id}
            tabIndex={selected === item.id ? 0 : -1}
            onClick={() => select(item.id)}
            onKeyDown={(event) => {
              let next: number;
              if (event.key === 'ArrowRight') next = (index + 1) % items.length;
              else if (event.key === 'ArrowLeft')
                next = (index + items.length - 1) % items.length;
              else if (event.key === 'Home') next = 0;
              else if (event.key === 'End') next = items.length - 1;
              else return;
              event.preventDefault();
              select(items[next].id);
              document
                .getElementById(`${prefix}-${items[next].id}-tab`)
                ?.focus();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
      {items.map((item) => (
        <section
          key={item.id}
          id={`${prefix}-${item.id}-panel`}
          role="tabpanel"
          aria-labelledby={`${prefix}-${item.id}-tab`}
          hidden={selected !== item.id}
          tabIndex={0}
        >
          {(selected === item.id || visited.has(item.id)) && item.content}
        </section>
      ))}
    </div>
  );
}
