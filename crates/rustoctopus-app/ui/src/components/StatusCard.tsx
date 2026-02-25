interface Props {
  label: string;
  value: string | number;
  sub?: string;
}

export default function StatusCard({ label, value, sub }: Props) {
  return (
    <div className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4">
      <div className="text-sm text-gray-500 dark:text-gray-400">{label}</div>
      <div className="mt-1 text-2xl font-semibold">{value}</div>
      {sub && <div className="mt-0.5 text-xs text-gray-400">{sub}</div>}
    </div>
  );
}
