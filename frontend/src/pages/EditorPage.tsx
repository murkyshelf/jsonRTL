import { useCallback, useMemo, useState, type DragEvent } from "react";
import ReactFlow, { Background, Controls, MiniMap, ReactFlowProvider, useReactFlow } from "reactflow";
import { ComponentLibrary } from "../components/ComponentLibrary";
import { LogicNode } from "../components/LogicNode";
import { PropertiesPanel } from "../components/PropertiesPanel";
import { Toolbar } from "../components/Toolbar";
import { ValidationPanel } from "../components/ValidationPanel";
import { VerilogModal } from "../components/VerilogModal";
import { useProjectStore, toFlow } from "../store/projectStore";
import type { ComponentKind } from "../types/project";
import { generateVerilog } from "../utils/generateVerilog";
import { validateProject } from "../utils/validation";

const nodeTypes = { logic: LogicNode };
const snapGrid: [number, number] = [16, 16];
const deleteKeys = ["Backspace", "Delete"];
const multiSelectionKeys = ["Meta", "Control"];

function EditorCanvas() {
  const project = useProjectStore((state) => state.project);
  const selectedId = useProjectStore((state) => state.selectedId);
  const setSelectedId = useProjectStore((state) => state.setSelectedId);
  const addComponent = useProjectStore((state) => state.addComponent);
  const onNodesChange = useProjectStore((state) => state.onNodesChange);
  const onEdgesChange = useProjectStore((state) => state.onEdgesChange);
  const onConnect = useProjectStore((state) => state.onConnect);
  const { screenToFlowPosition } = useReactFlow();
  const flow = useMemo(() => toFlow(project), [project]);
  const onDragOver = useCallback((event: DragEvent) => event.preventDefault(), []);
  const onDrop = useCallback(
    (event: DragEvent) => {
      event.preventDefault();
      const kind = event.dataTransfer.getData("application/logic-kind") as ComponentKind;
      if (kind) addComponent(kind, screenToFlowPosition({ x: event.clientX, y: event.clientY }));
    },
    [addComponent, screenToFlowPosition],
  );
  const onSelectionChange = useCallback(
    ({ nodes }: { nodes: { id: string }[] }) => setSelectedId(nodes[0]?.id ?? null),
    [setSelectedId],
  );

  return (
    <ReactFlow
      nodeTypes={nodeTypes}
      nodes={flow.nodes}
      edges={flow.edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onConnect={onConnect}
      onSelectionChange={onSelectionChange}
      onDragOver={onDragOver}
      onDrop={onDrop}
      fitView
      snapToGrid
      snapGrid={snapGrid}
      deleteKeyCode={deleteKeys}
      multiSelectionKeyCode={multiSelectionKeys}
      className="bg-slate-950"
    >
      <Background gap={16} color="#334155" />
      <Controls />
      <MiniMap nodeColor={(node) => (node.id === selectedId ? "#34d399" : "#475569")} />
    </ReactFlow>
  );
}

export function EditorPage() {
  const [verilog, setVerilog] = useState<string | null>(null);
  const project = useProjectStore((state) => state.project);
  const selectedId = useProjectStore((state) => state.selectedId);
  const selectedNode = project.nodes.find((node) => node.id === selectedId);
  const messages = validateProject(project);
  const store = useProjectStore();

  return (
    <ReactFlowProvider>
      <div className="grid h-screen grid-rows-[auto_1fr_auto] bg-slate-950 text-slate-100">
        <Toolbar
          project={project}
          onNew={store.newProject}
          onOpen={store.setProject}
          onGenerate={() => {
            try {
              setVerilog(generateVerilog(project));
            } catch (error) {
              setVerilog(error instanceof Error ? error.message : String(error));
            }
          }}
        />
        <main className="grid min-h-0 grid-cols-[auto_1fr_auto]">
          <ComponentLibrary />
          <EditorCanvas />
          <PropertiesPanel node={selectedNode} onChange={store.updateNode} />
        </main>
        <ValidationPanel messages={messages} />
        {verilog !== null && <VerilogModal code={verilog} onClose={() => setVerilog(null)} />}
      </div>
    </ReactFlowProvider>
  );
}
