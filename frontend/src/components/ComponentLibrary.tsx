import type { ComponentKind } from "../types/project";
import { componentGroups } from "../utils/components";

export function ComponentLibrary() {
  return (
    <aside className="panel w-48">
      <h2 className="panel-title">Component Library</h2>
      {componentGroups.map((group) => (
        <section key={group.title} className="space-y-2">
          <h3 className="text-xs uppercase tracking-wide text-slate-500">{group.title}</h3>
          {group.items.map((kind) => (
            <button
              key={kind}
              draggable
              onDragStart={(event) => event.dataTransfer.setData("application/logic-kind", kind)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-left text-sm text-slate-100 hover:border-emerald-500"
            >
              {kind}
            </button>
          ))}
        </section>
      ))}
    </aside>
  );
}
