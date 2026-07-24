import { create } from "zustand";
import type { Connection, Edge, EdgeChange, Node, NodeChange } from "reactflow";
import { addEdge, applyEdgeChanges, applyNodeChanges } from "reactflow";
import type { ComponentKind, ProjectJson } from "../types/project";
import { defaultInputs } from "../utils/components";

type NodeData = { kind: ComponentKind; name: string; inputs: number };

type ProjectState = {
  project: ProjectJson;
  selectedId: string | null;
  setProject: (project: ProjectJson) => void;
  setSelectedId: (id: string | null) => void;
  addComponent: (kind: ComponentKind, position: { x: number; y: number }) => void;
  updateNode: (id: string, patch: Partial<{ id: string; name: string; inputs: number }>) => void;
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  newProject: () => void;
};

function nodeToFlow(node: ProjectJson["nodes"][number]): Node<NodeData> {
  return {
    id: node.id,
    type: "logic",
    position: node.position,
    data: { kind: node.kind, name: node.name, inputs: node.inputs },
  };
}

function wireToFlow(wire: ProjectJson["wires"][number]): Edge {
  return { ...wire, animated: false };
}

function flowToProject(nodes: Node<NodeData>[], edges: Edge[]): ProjectJson {
  return {
    nodes: nodes.map((node) => ({
      id: node.id,
      name: node.data.name,
      kind: node.data.kind,
      inputs: node.data.inputs,
      position: node.position,
    })),
    wires: edges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: edge.sourceHandle,
      targetHandle: edge.targetHandle,
    })),
  };
}

function nextNodeId(kind: ComponentKind, project: ProjectJson) {
  const prefix = kind.toLowerCase();
  const used = new Set(project.nodes.map((node) => node.id));
  let index = 1;
  while (used.has(`${prefix}-${index}`)) index += 1;
  return `${prefix}-${index}`;
}

export function toFlow(project: ProjectJson) {
  return {
    nodes: project.nodes.map(nodeToFlow),
    edges: project.wires.map(wireToFlow),
  };
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  project: { nodes: [], wires: [] },
  selectedId: null,
  setProject: (project) => set({ project, selectedId: null }),
  setSelectedId: (selectedId) => set({ selectedId }),
  newProject: () => set({ project: { nodes: [], wires: [] }, selectedId: null }),
  addComponent: (kind, position) =>
    set((state) => {
      const id = nextNodeId(kind, state.project);
      return {
        project: {
          ...state.project,
          nodes: [...state.project.nodes, { id, name: id, kind, inputs: defaultInputs(kind), position }],
        },
        selectedId: id,
      };
    }),
  updateNode: (id, patch) =>
    set((state) => ({
      selectedId: patch.id ?? state.selectedId,
      project: {
        ...state.project,
        nodes: state.project.nodes.map((node) =>
          node.id === id ? { ...node, ...patch, id: patch.id ?? node.id } : node,
        ),
        wires: state.project.wires.map((wire) => ({
          ...wire,
          source: wire.source === id ? (patch.id ?? id) : wire.source,
          target: wire.target === id ? (patch.id ?? id) : wire.target,
        })),
      },
    })),
  onNodesChange: (changes) => {
    const flow = toFlow(get().project);
    set({ project: flowToProject(applyNodeChanges(changes, flow.nodes), flow.edges) });
  },
  onEdgesChange: (changes) => {
    const flow = toFlow(get().project);
    set({ project: flowToProject(flow.nodes, applyEdgeChanges(changes, flow.edges)) });
  },
  onConnect: (connection) => {
    const flow = toFlow(get().project);
    const edge = { ...connection, id: `wire-${Date.now()}` };
    set({ project: flowToProject(flow.nodes, addEdge(edge, flow.edges)) });
  },
}));
