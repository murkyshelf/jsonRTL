import type { ProjectJson } from "../types/project";

export function saveProject(project: ProjectJson) {
  const blob = new Blob([JSON.stringify(project, null, 2)], { type: "application/json" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = "logic-project.json";
  link.click();
  URL.revokeObjectURL(link.href);
}

export function readProjectFile(file: File) {
  return file.text().then((text) => JSON.parse(text) as ProjectJson);
}
