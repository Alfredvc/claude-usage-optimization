import { useEffect, useMemo, useRef, useState } from "react";

export interface PickerOption {
  value: string;
  label: string;
  hint?: string;
}

interface Props {
  label: string;
  options: PickerOption[];
  selected: string[];
  onChange: (next: string[]) => void;
  pillClassName?: string;
  placeholder?: string;
}

export function MultiSelectPicker({
  label,
  options,
  selected,
  onChange,
  pillClassName = "",
  placeholder = "Search…",
}: Props) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onDoc(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);

  const byValue = useMemo(() => {
    const m = new Map<string, PickerOption>();
    for (const o of options) m.set(o.value, o);
    return m;
  }, [options]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const selSet = new Set(selected);
    return options
      .filter((o) => !selSet.has(o.value))
      .filter((o) =>
        needle === ""
          ? true
          : o.label.toLowerCase().includes(needle) ||
            o.value.toLowerCase().includes(needle),
      )
      .slice(0, 50);
  }, [options, q, selected]);

  const add = (v: string) => {
    if (!selected.includes(v)) onChange([...selected, v]);
    setQ("");
  };
  const remove = (v: string) => onChange(selected.filter((x) => x !== v));

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && filtered.length > 0) {
      add(filtered[0].value);
      e.preventDefault();
    } else if (e.key === "Backspace" && q === "" && selected.length > 0) {
      remove(selected[selected.length - 1]);
    }
  };

  return (
    <div className="msp" ref={wrapRef}>
      <label className="msp-label">{label}</label>
      <div
        className={`msp-control ${open ? "open" : ""}`}
        onClick={() => setOpen(true)}
      >
        {selected.map((v) => {
          const opt = byValue.get(v);
          return (
            <span key={v} className={`msp-chip ${pillClassName}`}>
              {opt?.label ?? v}
              <button
                className="msp-chip-x"
                onClick={(e) => {
                  e.stopPropagation();
                  remove(v);
                }}
                aria-label={`remove ${opt?.label ?? v}`}
              >
                ×
              </button>
            </span>
          );
        })}
        <input
          className="msp-input"
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={onKey}
          placeholder={selected.length === 0 ? placeholder : ""}
        />
      </div>
      {open && filtered.length > 0 && (
        <div className="msp-dropdown">
          {filtered.map((o) => (
            <button
              key={o.value}
              className="msp-option"
              onClick={() => add(o.value)}
            >
              <span className="msp-option-label">{o.label}</span>
              {o.hint && <span className="msp-option-hint">{o.hint}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
