import { useRef } from "react";
import type { ProjectJson } from "../types/project";
import { readProjectFile, saveProject } from "../hooks/useProjectFile";

type Props = {
  project: ProjectJson;
  onNew: () => void;
  onOpen: (project: ProjectJson) => void;
  onGenerate: () => void;
};

export function Toolbar({ project, onNew, onOpen, onGenerate }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);

  return (
    <header className="flex h-12 items-center justify-between border-b border-slate-800 bg-slate-950 px-3">
      <div className="font-semibold text-slate-100">Digital Logic Editor</div>
      <div className="flex gap-2">
        <button className="btn" onClick={onNew}>New Project</button>
        <button className="btn" onClick={() => inputRef.current?.click()}>Open JSON</button>
        <button className="btn" onClick={() => saveProject(project)}>Save JSON</button>
        <button className="btn-primary" onClick={onGenerate}>Generate Verilog</button>
      </div>
      <input
        ref={inputRef}
        className="hidden"
        type="file"
        accept="application/json,.json"
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) readProjectFile(file).then(onOpen);
          event.currentTarget.value = "";
        }}
      />
    </header>
  );
}
