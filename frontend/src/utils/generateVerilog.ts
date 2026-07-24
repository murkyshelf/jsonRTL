import type { ProjectJson } from "../types/project";

declare global {
  interface Window {
    generateVerilog?: (projectJson: ProjectJson) => string;
  }
}

export function generateVerilog(projectJson: ProjectJson) {
  if (typeof window.generateVerilog !== "function") {
    throw new Error("generateVerilog(projectJson) is not loaded.");
  }
  return window.generateVerilog(projectJson);
}
