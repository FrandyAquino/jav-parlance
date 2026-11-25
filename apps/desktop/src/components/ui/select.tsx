import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface SelectOption {
  value: string;
  label: string;
  group?: string;
}

interface SelectProps {
  value: string | null;
  options: SelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
  className?: string;
}

function groupOptions(options: SelectOption[]): Array<[string, SelectOption[]]> {
  const groups = new Map<string, SelectOption[]>();
  for (const option of options) {
    const key = option.group ?? "";
    groups.set(key, [...(groups.get(key) ?? []), option]);
  }
  return [...groups.entries()];
}

export function Select({ value, options, onChange, placeholder, label, icon, disabled, className }: SelectProps) {
  return (
    <SelectPrimitive.Root value={value ?? ""} onValueChange={onChange} disabled={disabled ?? false}>
      <SelectPrimitive.Trigger
        aria-label={label}
        className={cn(
          "inline-flex h-7 min-w-0 items-center gap-1.5 rounded-lg px-2 text-[12px] text-ink-muted hover:bg-surface-raised hover:text-ink disabled:opacity-40",
          className,
        )}
      >
        {icon}
        <span className="truncate">
          <SelectPrimitive.Value placeholder={placeholder} />
        </span>
        <ChevronDown className="size-3 shrink-0 opacity-60" aria-hidden />
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={4}
          className="z-50 max-h-80 min-w-56 overflow-hidden rounded-xl border border-line bg-[var(--surface)] text-[12px] text-ink shadow-xl backdrop-blur-xl"
        >
          <SelectPrimitive.Viewport className="p-1">
            {groupOptions(options).map(([group, items]) => (
              <SelectPrimitive.Group key={group}>
                {group && (
                  <SelectPrimitive.Label className="px-2 pb-1 pt-2 text-[10px] font-medium uppercase tracking-wide text-ink-faint">
                    {group}
                  </SelectPrimitive.Label>
                )}
                {items.map((option) => (
                  <SelectPrimitive.Item
                    key={option.value}
                    value={option.value}
                    className="relative flex cursor-default items-center rounded-md py-1.5 pl-6 pr-2 outline-none data-[highlighted]:bg-surface-raised"
                  >
                    <SelectPrimitive.ItemIndicator className="absolute left-1.5">
                      <Check className="size-3" aria-hidden />
                    </SelectPrimitive.ItemIndicator>
                    <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText>
                  </SelectPrimitive.Item>
                ))}
              </SelectPrimitive.Group>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
