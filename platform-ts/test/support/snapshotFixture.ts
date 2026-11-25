/** A tiny synthetic catalog in the data API's shape (the real recorded snapshot is 5 MB). */
const row = (url: string, type: string | null, lang: { english_name: string | null; ietf_code: string; national_name: string; direction: string } | null) => ({
  repo_url: url,
  content: { resource_type: type, language: lang },
});
const L = (english: string, code: string, national: string, direction = "ltr") => ({ english_name: english, ietf_code: code, national_name: national, direction });
const H = "https://content.example.test/";

export const SW = L("Swahili", "sw", "Kiswahili");
export const LN = L("Lingala", "ln", "Lingala");
export const RWANDA = L("Kinyarwanda", "rw", "Ikinyarwanda");

export function syntheticRows() {
  return [
    row(`${H}WA-Catalog/sw_ulb`, "ulb", SW),
    row(`${H}WA-Catalog/sw_tn`, "tn", SW),
    row(`${H}WA-Catalog/sw_tw`, "tw", SW),
    row(`${H}x/ln_mat_text_reg`, "reg", LN),
    row(`${H}x/ln_mrk_text_reg`, "reg", LN),
    row(`${H}DCS-Mirror/faustin-azaza_rw_php_text_reg`, "reg", RWANDA),
    row(`${H}DCS-Mirror/rw_jhn_text_reg`, "reg", RWANDA),
    row(`${H}x/null_language`, "reg", null),
    row(`${H}x/null_type`, null, SW),
  ];
}

export function envelope(rows: unknown[]): string {
  return JSON.stringify({ data: { git_repo: rows } });
}
