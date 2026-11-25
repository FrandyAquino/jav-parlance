import * as SwitchPrimitive from "@radix-ui/react-switch";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
}

export function SwitchRow({ checked, onChange, label, description }: SwitchProps) {
  return (
    <label className="flex items-start justify-between gap-4 py-2">
      <span className="min-w-0">
        <span className="block text-[13px] text-ink">{label}</span>
        {description && <span className="block text-[12px] text-ink-muted">{description}</span>}
      </span>
      <SwitchPrimitive.Root
        checked={checked}
        onCheckedChange={onChange}
        className="relative mt-0.5 h-5 w-9 shrink-0 rounded-full bg-line transition-colors data-[state=checked]:bg-accent"
      >
        <SwitchPrimitive.Thumb className="block size-4 translate-x-0.5 rounded-full bg-white shadow transition-transform data-[state=checked]:translate-x-[18px]" />
      </SwitchPrimitive.Root>
    </label>
  );
}
