import type { ComponentKind } from "../types/project";

export const componentGroups: { title: string; items: ComponentKind[] }[] = [
  { title: "Inputs", items: ["INPUT"] },
  { title: "Outputs", items: ["OUTPUT"] },
  { title: "Logic Gates", items: ["AND", "OR", "XOR", "NOT", "NAND"] },
];

export function defaultInputs(kind: ComponentKind) {
  if (kind === "NOT") return 1;
  if (["AND", "OR", "XOR", "NAND"].includes(kind)) return 2;
  return 0;
}

export function canEditInputs(kind: ComponentKind) {
  return ["AND", "OR", "XOR", "NAND"].includes(kind);
}
