import type { ProjectJson } from "../types/project";

export function validateProject(project: ProjectJson) {
  const messages: string[] = [];
  const seen = new Set<string>();
  const nodes = new Set(project.nodes.map((node) => node.id));

  for (const node of project.nodes) {
    if (seen.has(node.id)) messages.push(`Duplicate component ID: ${node.id}`);
    seen.add(node.id);
  }

  for (const wire of project.wires) {
    if (!wire.source && !wire.target) messages.push(`Floating wire: ${wire.id}`);
    if (!wire.source || !nodes.has(wire.source)) messages.push(`Missing wire source: ${wire.id}`);
    if (!wire.target || !nodes.has(wire.target)) messages.push(`Missing wire destination: ${wire.id}`);
  }

  return messages;
}
