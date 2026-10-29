import type { ContentConfigReport } from './bindings/ContentConfigReport';
export function contentChoices(report: ContentConfigReport, kind: string) {
  const choices = new Map<
    string,
    {
      id: string;
      name: string;
      paths: string[];
      dependencies: string[] | null;
      version: string | null;
    }
  >();
  for (const pack of report.discovery.packages.filter((p) => p.kind === kind)) {
    for (const definition of pack.definitions) {
      const existing = choices.get(definition.content_id);
      if (existing) existing.paths.push(pack.path);
      else
        choices.set(definition.content_id, {
          id: definition.content_id,
          name: definition.name,
          paths: [pack.path],
          dependencies: definition.dependencies,
          version: definition.version,
        });
    }
  }
  return [...choices.values()].sort((a, b) => a.name.localeCompare(b.name));
}
