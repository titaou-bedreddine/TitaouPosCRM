import { writable, derived, get } from 'svelte/store';
import ar from './ar.json';
import fr from './fr.json';
import en from './en.json';

export type Language = 'ar' | 'fr' | 'en';

const dictionaries: Record<Language, Record<string, string>> = {
  ar,
  fr,
  en,
};

const initialLang = (localStorage.getItem('pos_lang') as Language) || 'ar';
export const currentLocale = writable<Language>(initialLang);

// Synchronize HTML attributes on change
currentLocale.subscribe((lang) => {
  if (typeof document !== 'undefined') {
    document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
    document.documentElement.lang = lang;
    localStorage.setItem('pos_lang', lang);
  }
});

export function setLocale(lang: Language) {
  currentLocale.set(lang);
  // Persist so backend Telegram notifications match the UI language.
  import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('set_setting', { key: 'ui_language', value: lang }).catch(() => {}))
    .catch(() => {});
}

export function getLanguage(): Language {
  return get(currentLocale);
}

export function t(key: string, lang?: Language): string {
  const active = lang || get(currentLocale);
  return dictionaries[active]?.[key] || dictionaries['en']?.[key] || key;
}

/** t() with {placeholder} interpolation: tf('key', { n: '5' }). */
export function tf(key: string, params: Record<string, string>, lang?: Language): string {
  const raw = t(key, lang);
  return raw.replace(/\{(\w+)\}/g, (_, name: string) => params[name] ?? `{${name}}`);
}

export const translationStore = derived(currentLocale, ($lang) => (key: string) => {
  return dictionaries[$lang]?.[key] || dictionaries['en']?.[key] || key;
});