/** A synthetic catalog the size of the real one (about 21,000 rows, 5 MB of JSON), in the data API's shape. */
export function bigCatalogRows(n = 21_000): unknown[] {
  const types = ["ulb", "tn", "tw", "tq", "reg", "udb", "bc"];
  const rows: unknown[] = [];
  for (let i = 0; i < n; i += 1) {
    const lang = i % 420;
    const code = `l${lang.toString(36)}${lang % 7 === 0 ? `-${(lang * 3).toString(36)}` : ""}`;
    rows.push({
      repo_url: `https://content.example.test/WA-Catalog/${code}_${types[i % types.length]}_${i}`,
      content: { resource_type: types[i % types.length], language: { english_name: `Language ${lang}`, ietf_code: code, national_name: `Lugha ${lang}`, direction: "ltr" } },
    });
  }
  return rows;
}
