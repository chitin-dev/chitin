import { defineConfig, presetWind3 } from "unocss";

export default defineConfig({
  // Wind3 provides the utility vocabulary; the project-specific palette and
  // shortcuts below keep repeated molecular-viewer patterns readable.
  presets: [presetWind3()],
  // These classes are selected from runtime state and therefore cannot all be
  // discovered statically by UnoCSS's extractor.
  safelist: ["status-working", "status-ready", "status-error", "drop-visible"],
  theme: {
    colors: {
      acid: "#b9ff58",
      ink: "#071018",
      line: "rgba(186, 212, 205, 0.16)",
      muted: "#82948f",
      panel: "#0b151d",
    },
    fontFamily: {
      mono: '"DM Mono", monospace',
      sans: '"Manrope", sans-serif',
      serif: '"Newsreader", serif',
    },
  },
  shortcuts: {
    shell:
      "grid h-full w-full grid-rows-[64px_minmax(0,1fr)] bg-[radial-gradient(circle_at_70%_25%,rgba(55,104,96,0.10),transparent_34%)] bg-ink max-md:h-auto max-md:min-h-full",
    topbar:
      "grid grid-cols-[1fr_auto_1fr] items-center border-b border-line px-7 max-md:grid-cols-[1fr_auto] max-md:px-4.5",
    brand: "inline-flex items-center gap-3 font-mono text-[13px] text-[#eef5f3] no-underline tracking-[0.18em]",
    "brand-mark": "h-4 w-4 rounded-full border border-acid shadow-[inset_5px_0_0_#b9ff58]",
    status: "flex items-center gap-2.25 font-mono text-[11px] text-muted tracking-[0.06em] max-md:hidden",
    "status-dot": "h-1.5 w-1.5 rounded-full",
    "status-working": "bg-[#e4ad49]",
    "status-ready": "bg-acid shadow-[0_0_12px_rgba(185,255,88,0.7)]",
    "status-error": "bg-[#ff6957]",
    "source-link": "justify-self-end font-mono text-[11px] uppercase text-muted no-underline",
    workspace:
      "grid min-h-0 grid-cols-[minmax(280px,360px)_minmax(0,1fr)] max-md:grid-cols-1 max-md:grid-rows-[auto_62vh]",
    panel:
      "flex flex-col justify-between border-r border-line bg-[linear-gradient(145deg,rgba(13,27,35,0.95),rgba(7,16,24,0.86))] px-8.5 pb-7.5 pt-[clamp(28px,4vw,58px)] max-md:border-b max-md:border-r-0 max-md:px-5.5 max-md:py-5.5 max-md:pt-8",
    eyebrow: "mb-5.5 mt-0 font-mono text-[10px] font-medium text-acid tracking-[0.18em]",
    headline: "m-0 text-[clamp(43px,5vw,68px)] font-medium leading-[0.91] text-[#f0f5f4] tracking-[-0.055em]",
    "headline-accent": "font-serif font-normal text-[#9caaa7]",
    lede: "mb-0 mt-7 max-w-[275px] text-[13px] leading-[1.75] text-muted max-md:mt-5",
    controls: "my-8 grid gap-2.5 max-md:my-6.5",
    "control-surface": "min-h-16 border border-line bg-white/[0.018] text-[#dfe8e6]",
    "file-control":
      "control-surface flex cursor-pointer flex-col justify-center px-4 py-3 transition-colors hover:border-acid/50 hover:bg-acid/[0.035]",
    "control-label": "font-mono text-[9px] uppercase text-muted tracking-[0.12em]",
    "select-control": "control-surface grid grid-cols-[1fr_auto] items-center pl-4",
    "select-input":
      "h-full cursor-pointer border-0 bg-[#0d1921] py-0 pl-3.5 pr-9.5 text-xs text-[#e7eeec] outline-none",
    "reset-button": "control-surface flex cursor-pointer items-center justify-between px-4 text-xs",
    "key-cap": "border border-line px-1.75 py-1 font-mono text-[10px] text-muted",
    "structure-meta": "flex justify-between border-t border-line pt-5 font-mono text-[10px] text-muted",
    "structure-name": "max-w-[52%] overflow-hidden text-ellipsis whitespace-nowrap text-[#c6d1ce]",
    viewport: "relative min-h-0 min-w-0 overflow-hidden",
    "viewport-canvas": "block h-full w-full cursor-grab touch-none active:cursor-grabbing",
    "viewport-label":
      "pointer-events-none absolute right-7 top-6.25 z-2 font-mono text-[9px] text-muted tracking-[0.12em]",
    "interaction-hint":
      "pointer-events-none absolute bottom-6.25 right-7 z-2 font-mono text-[9px] text-muted tracking-[0.03em] max-md:bottom-4.5 max-md:right-4.5 max-md:text-[8px]",
    "drop-overlay":
      "pointer-events-none absolute inset-4.5 z-4 grid place-items-center border border-dashed border-transparent bg-transparent font-mono text-[13px] font-medium text-transparent transition-all duration-180",
    "drop-visible": "border-acid bg-ink/88 text-acid",
  },
});
