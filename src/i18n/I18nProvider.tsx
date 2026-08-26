import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";
import { de, en, type TranslationKey } from "./messages";
import type { LocaleSetting } from "../theme/types";

type ResolvedLocale = "de" | "en";
type Params = Record<string, string | number>;

interface I18nValue {
  locale: ResolvedLocale;
  t: (key: TranslationKey, params?: Params) => string;
  plural: (baseKey: string, count: number, params?: Params) => string;
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  formatDate: (value: Date | number, options?: Intl.DateTimeFormatOptions) => string;
}

const dictionaries = { de, en } as const;
const I18nContext = createContext<I18nValue | null>(null);

export function resolveLocale(setting: LocaleSetting, browserLanguage = navigator.language): ResolvedLocale {
  if (setting === "de" || setting === "en") return setting;
  return browserLanguage.toLowerCase().startsWith("de") ? "de" : "en";
}

export function interpolate(template: string, params: Params = {}): string {
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (match, key: string) =>
    Object.prototype.hasOwnProperty.call(params, key) ? String(params[key]) : match,
  );
}

export function I18nProvider({ localeSetting, children }: { localeSetting: LocaleSetting; children: ReactNode }) {
  const locale = resolveLocale(localeSetting);
  useEffect(() => { document.documentElement.lang = locale; }, [locale]);
  const value = useMemo<I18nValue>(() => ({
    locale,
    t: (key, params) => interpolate(dictionaries[locale][key] ?? `⟦${key}⟧`, params),
    plural: (baseKey, count, params) => {
      const category = new Intl.PluralRules(locale).select(count);
      const key = `${baseKey}_${category === "one" ? "one" : "other"}` as TranslationKey;
      return interpolate(dictionaries[locale][key] ?? `⟦${key}⟧`, { count, ...params });
    },
    formatNumber: (value, options) => new Intl.NumberFormat(locale, options).format(value),
    formatDate: (value, options) => new Intl.DateTimeFormat(locale, options).format(value),
  }), [locale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useOptionalI18n(): I18nValue | null {
  return useContext(I18nContext);
}

export function useI18n(): I18nValue {
  const value = useOptionalI18n();
  if (!value) throw new Error("I18nProvider missing");
  return value;
}
