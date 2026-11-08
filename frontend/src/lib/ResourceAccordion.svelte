<script lang="ts">
  // Mobile presentation of the resources available: for one language at a
  // time, a card per resource type that expands to the books it is
  // available for.
  import {
    availabilityStore,
    selectedResourcesStore,
    applyToSelection
  } from '$lib/stores/ResourcesStore'
  import { isSelected, selectionState, typeCells } from '$lib/resources'
  import type { Cell } from '$lib/resources'

  export let lang: string
  export let rows: Array<{ code: string; name: string }> = []

  $: av = $availabilityStore[lang]
  $: visibleBookCodes = rows.map((row) => row.code)
  $: nameOf = new Map(rows.map((row) => [row.code, row.name]))
  $: columns = (av?.resource_types ?? []).filter((t) => t.scope === 'book')

  function toggle(cell: Cell, event: Event) {
    applyToSelection([cell], (<HTMLInputElement>event.target).checked)
  }

  function booksOf(type: string, testament?: 'OT' | 'NT'): Array<Cell> {
    if (!av) return []
    return typeCells(av, type, visibleBookCodes).filter(
      (cell) => !testament || av.books.find((b) => b.code === cell.book)?.testament === testament
    )
  }
</script>

{#each columns as column (column.code)}
  {@const cells = booksOf(column.code)}
  {@const chosen = cells.filter((cell) => isSelected($selectedResourcesStore, cell)).length}
  <details
    class="mb-2 rounded-lg border border-[#E5E8EB] bg-white"
    data-testid="type-{lang}-{column.code}"
  >
    <summary
      class="flex min-h-[56px] cursor-pointer items-center justify-between px-4 text-xl text-[#33445C]"
    >
      <span>{column.name} <span class="text-base text-[#66768B]">({column.code})</span></span>
      <span class="text-base text-[#66768B]">{chosen} of {cells.length}</span>
    </summary>
    <div class="px-4 pb-2">
      <div class="flex flex-wrap gap-2 py-2 text-base">
        <button
          class="rounded border border-[#015ad9] px-2 py-1 text-[#015ad9]"
          on:click={() => applyToSelection(cells, true)}>Select all</button
        >
        <button
          class="rounded border border-[#015ad9] px-2 py-1 text-[#015ad9]"
          on:click={() => applyToSelection(booksOf(column.code, 'OT'), true)}>Old Testament</button
        >
        <button
          class="rounded border border-[#015ad9] px-2 py-1 text-[#015ad9]"
          on:click={() => applyToSelection(booksOf(column.code, 'NT'), true)}>New Testament</button
        >
        <button
          class="rounded border border-[#B3B9C2] px-2 py-1 text-[#33445C]"
          disabled={selectionState($selectedResourcesStore, cells) === 'none'}
          on:click={() => applyToSelection(cells, false)}>Clear</button
        >
      </div>
      {#each cells as cell (cell.book)}
        <label class="flex h-[48px] items-center text-xl text-[#33445C]">
          <input
            type="checkbox"
            class="checkbox-target checkbox-style"
            aria-label={`${nameOf.get(cell.book) ?? cell.book} – ${column.name}`}
            data-testid="cell-{lang}-{column.code}-{cell.book}"
            checked={isSelected($selectedResourcesStore, cell)}
            on:change={(event) => toggle(cell, event)}
          />
          <span class="pl-2">{nameOf.get(cell.book) ?? cell.book}</span>
          <span class="ml-2 text-base text-[#66768B]">({cell.book})</span>
        </label>
      {/each}
    </div>
  </details>
{/each}
