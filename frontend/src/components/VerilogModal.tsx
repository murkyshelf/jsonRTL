type Props = {
  code: string;
  onClose: () => void;
};

export function VerilogModal({ code, onClose }: Props) {
  return (
    <div className="fixed inset-0 z-20 grid place-items-center bg-black/60 p-4">
      <section className="grid max-h-[85vh] w-full max-w-4xl grid-rows-[auto_1fr] overflow-hidden rounded-lg border border-slate-700 bg-slate-950">
        <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
          <h2 className="font-semibold text-slate-100">Generated Verilog</h2>
          <div className="flex gap-2">
            <button className="btn" onClick={() => navigator.clipboard.writeText(code)}>Copy</button>
            <button className="btn" onClick={onClose}>Close</button>
          </div>
        </div>
        <pre className="overflow-auto p-4 text-sm text-emerald-100">{code}</pre>
      </section>
    </div>
  );
}
