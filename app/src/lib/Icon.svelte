<script lang="ts">
  // The one icon-usage convention. Renders a Material Symbols
  // ligature from the self-hosted variable font (see app.css). `name` is the
  // symbol ligature (e.g. "close", "add", "splitscreen_right").
  //
  // Decorative by default (aria-hidden) since icons usually sit inside a
  // labelled button; pass `label` to expose it as an image to assistive tech.
  interface Props {
    name: string;
    size?: number | string;
    fill?: boolean;
    weight?: number;
    label?: string;
  }

  let { name, size = 20, fill = false, weight = 400, label }: Props = $props();

  const fontSize = $derived(typeof size === "number" ? `${size}px` : size);
</script>

<span
  class="material-symbols-outlined"
  style="font-size: {fontSize}; font-variation-settings: 'FILL' {fill
    ? 1
    : 0}, 'wght' {weight};"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
  translate="no">{name}</span
>
