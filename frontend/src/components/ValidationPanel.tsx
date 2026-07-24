type Props = {
  messages: string[];
};

export function ValidationPanel({ messages }: Props) {
  return (
    <footer className="h-20 border-t border-slate-800 bg-slate-950 px-3 py-2 text-sm">
      {messages.length === 0 ? (
        <div className="text-emerald-400">No validation messages.</div>
      ) : (
        <ul className="space-y-1 text-amber-300">
          {messages.map((message) => <li key={message}>{message}</li>)}
        </ul>
      )}
    </footer>
  );
}
