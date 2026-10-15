<script lang="ts">
  // Desktop presentation of the resources available: a grid of books (rows)
  // by resource types (columns), grouped by language when two languages are
  // chosen. Resources that don't exist for a book have no checkbox.
  import {
    availabilityStore,
    selectedResourcesStore,
    applyToSelection
  } from '$lib/stores/ResourcesStore'
  import { selectionState, isSelected, bookCells, typeCells, mismatches } from '$lib/resources'
  import type { Cell, Mismatch } from '$lib/resources'
  import { langCodesStore } from '$lib/stores/LanguagesStore'

  export let rows: Array<{ code: string; name: string }> = []
  export let langNames: Array<string> = []

  $: langs = $langCodesStore
  $: visibleBookCodes = rows.map((row) => row.code)
  $: mismatchReasons = new Map<string, Mismatch['reason']>(
    mismatches($selectedResourcesStore, $availabilityStore, langs).map((m) => [
      `${m.lang}|${m.type}|${m.book}`,
      m.reason
    ])
  )

  function columns(lang: string) {
    return ($availabilityStore[lang]?.resource_types ?? []).filter((t) => t.scope === 'book')
  }

  function label(lang: string, text: string): string {
    return langs.length > 1 ? `${text} (${lang})` : text
  }

  function toggle(cells: Array<Cell>, event: Event) {
    applyToSelection(cells, (<HTMLInputElement>event.target).checked)
  }

  function toggleGroup(cells: Array<Cell>, state: 'none' | 'some' | 'all') {
    applyToSelection(cells, state !== 'all')
  }

  function mismatchTitle(reason: Mismatch['reason'] | undefined, lang: string): string {
    if (!reason) return ''
    const other = langNames[langs.findIndex((l) => l !== lang)] ?? 'the other language'
    return reason === 'unavailable' ? `Not available in ${other}` : `Not chosen in ${other}`
  }
</script>

<ul class="mb-2 text-base text-[#66768B]" data-testid="resource-type-legend">
  {#each langs as lang, index}
    <li>
      {#if langs.length > 1}<span class="font-semibold">{langNames[index] ?? lang}:</span>{/if}
      {#each columns(lang) as column, columnIndex}
        {column.code} = {column.name}{columnIndex < columns(lang).length - 1 ? '; ' : ''}
      {/each}
    </li>
  {/each}
</ul>

<div class="overflow-x-auto">
  <table class="w-full border-collapse text-xl text-[#33445C]" data-testid="resource-matrix">
    <thead class="sticky top-0 bg-white">
      <tr>
        <th class="p-2 text-left" rowspan="2">Book</th>
        {#each langs as lang, index}
          <th
            class="border-l border-[#E5E8EB] p-2 text-center"
            colspan={columns(lang).length + 1}
            scope="colgroup"
          >
            {langNames[index] ?? lang}
          </th>
        {/each}
      </tr>
      <tr>
        {#each langs as lang}
          <th class="border-l border-[#E5E8EB] p-1 text-center text-base font-normal">All</th>
          {#each columns(lang) as column}
            {@const cells = typeCells($availabilityStore[lang], column.code, visibleBookCodes)}
            {@const state = selectionState($selectedResourcesStore, cells)}
            <th class="p-1 text-center text-base font-normal" scope="col">
              <label class="flex flex-col items-center" title={column.name}>
                <span>{column.code}</span>
                <input
                  type="checkbox"
                  class="checkbox-target checkbox-style"
                  aria-label={label(lang, `Select all ${column.name}`)}
                  data-testid="column-{lang}-{column.code}"
                  checked={state === 'all'}
                  indeterminate={state === 'some'}
                  disabled={cells.length === 0}
                  on:change={() => toggleGroup(cells, state)}
                />
              </label>
            </th>
          {/each}
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.code)}
        <tr class="border-t border-[#E5E8EB]" data-testid="row-{row.code}">
          <th class="p-2 text-left font-normal" scope="row">
            {row.name} <span class="text-base text-[#66768B]">({row.code})</span>
          </th>
          {#each langs as lang}
            {@const av = $availabilityStore[lang]}
            {@const rowCells = av ? bookCells(av, row.code) : []}
            {@const rowState = selectionState($selectedResourcesStore, rowCells)}
            <td class="border-l border-[#E5E8EB] p-1 text-center">
              {#if rowCells.length > 0}
                <input
                  type="checkbox"
                  class="checkbox-target checkbox-style"
                  aria-label={label(lang, `Select all for ${row.name}`)}
                  data-testid="row-{lang}-{row.code}"
                  checked={rowState === 'all'}
                  indeterminate={rowState === 'some'}
                  on:change={() => toggleGroup(rowCells, rowState)}
                />
              {:else}
                <span class="text-[#B3B9C2]" title="Nothing available">–</span>
              {/if}
            </td>
            {#each columns(lang) as column}
              {@const cell = { lang, type: column.code, book: row.code }}
              <td class="p-1 text-center">
                {#if av && (av.books.find((b) => b.code === row.code)?.resources ?? []).includes(column.code)}
                  {@const reason = mismatchReasons.get(`${lang}|${column.code}|${row.code}`)}
                  <label class="inline-flex items-center">
                    <input
                      type="checkbox"
                      class="checkbox-target checkbox-style"
                      aria-label={label(lang, `${row.name} – ${column.name}`)}
                      data-testid="cell-{lang}-{column.code}-{row.code}"
                      checked={isSelected($selectedResourcesStore, cell)}
                      on:change={(event) => toggle([cell], event)}
                    />
                    {#if reason === 'unavailable'}
                      <span
                        class="ml-1 text-base text-[#B85659]"
                        title={mismatchTitle(reason, lang)}
                        data-testid="mismatch-{lang}-{column.code}-{row.code}">⚠</span
                      >
                    {/if}
                  </label>
                {:else}
                  <span class="text-[#B3B9C2]" title="Not available">–</span>
                {/if}
              </td>
            {/each}
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>
