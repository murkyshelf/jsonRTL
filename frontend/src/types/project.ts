import type { XYPosition } from "reactflow";

export type ComponentKind = "INPUT" | "OUTPUT" | "AND" | "OR" | "XOR" | "NOT" | "NAND";

export type ProjectNode = {
  id: string;
  name: string;
  kind: ComponentKind;
  inputs: number;
  position: XYPosition;
};

export type ProjectWire = {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string | null;
  targetHandle?: string | null;
};

export type ProjectJson = {
  nodes: ProjectNode[];
  wires: ProjectWire[];
};
