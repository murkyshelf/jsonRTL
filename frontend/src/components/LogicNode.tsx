import { Handle, Position, type NodeProps } from "reactflow";
import type { ComponentKind } from "../types/project";

type Data = { kind: ComponentKind; name: string; inputs: number };

export function LogicNode({ data, selected }: NodeProps<Data>) {
  const isInput = data.kind === "INPUT";
  const isOutput = data.kind === "OUTPUT";

  return (
    <div className={`min-w-28 rounded-md border bg-slate-900 px-3 py-2 text-center shadow ${selected ? "border-emerald-400" : "border-slate-700"}`}>
      {!isInput &&
        Array.from({ length: Math.max(1, data.inputs) }).map((_, index) => (
          <Handle key={index} id={`in-${index}`} type="target" position={Position.Left} style={{ top: 18 + index * 16 }} />
        ))}
      {!isOutput && <Handle id="out" type="source" position={Position.Right} />}
      <div className="text-xs text-slate-400">{data.kind}</div>
      <div className="text-sm font-medium text-slate-100">{data.name}</div>
    </div>
  );
}
