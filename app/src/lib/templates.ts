// Starter templates for "New from template": minimal, clean-compiling scaffolds
// per instrument so a new document opens with the right tuning/time/default
// boilerplate already in place. Sources live in examples/templates/*.ctab so they
// are a single source of truth, compile-checked by cadtab-core.
import banjo from "../../../examples/templates/banjo.ctab?raw";
import guitar from "../../../examples/templates/guitar.ctab?raw";
import blank from "../../../examples/templates/blank.ctab?raw";

export interface Template {
  id: string;
  label: string;
  source: string;
}

export const TEMPLATES: readonly Template[] = [
  { id: "banjo", label: "Banjo (open G)", source: banjo },
  { id: "guitar", label: "Guitar (standard)", source: guitar },
  { id: "blank", label: "Blank", source: blank },
];

export function templateById(id: string): Template | undefined {
  return TEMPLATES.find((t) => t.id === id);
}
