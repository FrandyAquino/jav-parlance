/** Languages offered as translation targets; Whisper detects ~100 as sources. */
export const LANGUAGES: ReadonlyArray<{ code: string; name: string }> = [
  { code: "en", name: "English" },
  { code: "es", name: "Español" },
  { code: "fr", name: "Français" },
  { code: "de", name: "Deutsch" },
  { code: "it", name: "Italiano" },
  { code: "pt", name: "Português" },
  { code: "nl", name: "Nederlands" },
  { code: "pl", name: "Polski" },
  { code: "uk", name: "Українська" },
  { code: "ru", name: "Русский" },
  { code: "tr", name: "Türkçe" },
  { code: "ar", name: "العربية" },
  { code: "hi", name: "हिन्दी" },
  { code: "zh", name: "中文" },
  { code: "ja", name: "日本語" },
  { code: "ko", name: "한국어" },
];

export function languageName(code: string | null | undefined): string {
  if (!code) return "Auto";
  return LANGUAGES.find((language) => language.code === code)?.name ?? code.toUpperCase();
}
