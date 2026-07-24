import type { ProjectNode } from "../types/project";
import { canEditInputs } from "../utils/components";

type Props = {
  node?: ProjectNode;
  onChange: (id: string, patch: Partial<{ id: string; name: string; inputs: number }>) => void;
};

export function PropertiesPanel({ node, onChange }: Props) {
  return (
    <aside className="panel w-64">
      <h2 className="panel-title">Properties</h2>
      {!node ? (
        <p className="text-sm text-slate-500">Select a component.</p>
      ) : (
        <div className="space-y-3">
          <label className="field">
            <span>Name</span>
            <input value={node.name} onChange={(event) => onChange(node.id, { name: event.target.value })} />
          </label>
          <label className="field">
            <span>Unique ID</span>
            <input value={node.id} onChange={(event) => onChange(node.id, { id: event.target.value })} />
          </label>
          {canEditInputs(node.kind) && (
            <label className="field">
              <span>Number of inputs</span>
              <input
                type="number"
                min={2}
                max={8}
                value={node.inputs}
                onChange={(event) => onChange(node.id, { inputs: Number(event.target.value) })}
              />
            </label>
          )}
        </div>
      )}
    </aside>
  );
}
