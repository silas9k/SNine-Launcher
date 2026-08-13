export interface AccentPalette {
  input: string;
  accent: string;
  onAccent: "#000000" | "#ffffff";
  hover: string;
  pressed: string;
  focus: string;
  lightContrast: number;
  darkContrast: number;
  adjusted: boolean;
  valid: boolean;
}

type RGB = { r: number; g: number; b: number };

const LIGHT_SURFACE = "#ffffff";
const DARK_SURFACE = "#12151b";
const MIN_UI_CONTRAST = 3;
const MIN_TEXT_CONTRAST = 4.5;

export function isHexColor(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

function toRgb(hex: string): RGB {
  return {
    r: Number.parseInt(hex.slice(1, 3), 16),
    g: Number.parseInt(hex.slice(3, 5), 16),
    b: Number.parseInt(hex.slice(5, 7), 16),
  };
}

function toHex({ r, g, b }: RGB): string {
  return `#${[r, g, b].map((value) => Math.round(value).toString(16).padStart(2, "0")).join("")}`;
}

function mix(a: string, b: string, amount: number): string {
  const left = toRgb(a);
  const right = toRgb(b);
  return toHex({
    r: left.r + (right.r - left.r) * amount,
    g: left.g + (right.g - left.g) * amount,
    b: left.b + (right.b - left.b) * amount,
  });
}

function channel(value: number): number {
  const normalized = value / 255;
  return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
}

export function contrastRatio(first: string, second: string): number {
  const luminance = (hex: string) => {
    const rgb = toRgb(hex);
    return 0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b);
  };
  const a = luminance(first);
  const b = luminance(second);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

function chooseText(accent: string): "#000000" | "#ffffff" {
  return contrastRatio(accent, "#ffffff") >= contrastRatio(accent, "#000000") ? "#ffffff" : "#000000";
}

export function resolveAccentPalette(input: string): AccentPalette {
  if (!isHexColor(input)) {
    return {
      input,
      accent: "#8b5cf6",
      onAccent: "#ffffff",
      hover: "#9d78f8",
      pressed: "#7343df",
      focus: "#b9a0ff",
      lightContrast: contrastRatio("#8b5cf6", LIGHT_SURFACE),
      darkContrast: contrastRatio("#8b5cf6", DARK_SURFACE),
      adjusted: false,
      valid: false,
    };
  }

  const normalized = input.toLowerCase();
  let accent = normalized;
  let lightContrast = contrastRatio(accent, LIGHT_SURFACE);
  let darkContrast = contrastRatio(accent, DARK_SURFACE);
  let onAccent = chooseText(accent);

  for (let step = 0; step < 24; step += 1) {
    const textContrast = contrastRatio(accent, onAccent);
    if (lightContrast >= MIN_UI_CONTRAST && darkContrast >= MIN_UI_CONTRAST && textContrast >= MIN_TEXT_CONTRAST) break;
    if (lightContrast < MIN_UI_CONTRAST) accent = mix(accent, "#000000", 0.08);
    if (darkContrast < MIN_UI_CONTRAST) accent = mix(accent, "#ffffff", 0.08);
    onAccent = chooseText(accent);
    lightContrast = contrastRatio(accent, LIGHT_SURFACE);
    darkContrast = contrastRatio(accent, DARK_SURFACE);
  }

  return {
    input,
    accent,
    onAccent,
    hover: mix(accent, onAccent === "#ffffff" ? "#ffffff" : "#000000", 0.12),
    pressed: mix(accent, onAccent === "#ffffff" ? "#000000" : "#ffffff", 0.18),
    focus: mix(accent, "#ffffff", 0.38),
    lightContrast,
    darkContrast,
    adjusted: accent !== normalized,
    valid: true,
  };
}
