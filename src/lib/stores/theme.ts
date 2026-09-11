import { writable, get } from 'svelte/store';

export type ThemeSkin = 'soft' | 'glass' | 'contrast' | 'coral';

export const THEME_SKINS: Array<{ id: ThemeSkin; name: string; colors: string[] }> = [
  { id: 'soft', name: 'Soft Neumorphic', colors: ['#F5F7FA', '#FFFFFF', '#1E75FF'] },
  { id: 'glass', name: 'Sleek Glassmorphism', colors: ['#7C3AED', '#F28C28', '#E8DED8'] },
  { id: 'contrast', name: 'Bold High-Contrast', colors: ['#FFCC00', '#FFFFFF', '#0B0B0B'] },
  { id: 'coral', name: 'Warm Coral', colors: ['#FFF3F2', '#FFFFFF', '#FF3B30'] },
];

const STORAGE_KEY = 'pos_theme';

/** Applies the skin: data-theme on <html> so app.css token sets activate. */
function applySkin(skin: ThemeSkin) {
  if (typeof document === 'undefined') return;
  document.documentElement.setAttribute('data-theme', skin);
}

function loadInitial(): ThemeSkin {
  if (typeof localStorage === 'undefined') return 'soft';
  const saved = localStorage.getItem(STORAGE_KEY) as ThemeSkin | null;
  return saved && THEME_SKINS.some((t) => t.id === saved) ? saved : 'soft';
}

export const themeSkin = writable<ThemeSkin>(loadInitial());

// Apply immediately on boot (also mirrored by an inline script in index.html
// to avoid a first-paint flash).
applySkin(get(themeSkin));

export function setThemeSkin(skin: ThemeSkin) {
  themeSkin.set(skin);
  applySkin(skin);
  localStorage.setItem(STORAGE_KEY, skin);
}
