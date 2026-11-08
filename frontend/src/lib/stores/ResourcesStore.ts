import { derived, get, writable } from 'svelte/store'
import type { Readable, Writable } from 'svelte/store'
import { langCodesStore } from '$lib/stores/LanguagesStore'
import {
  applyCells,
  chosenResourceTypes,
  languagesWithSelection,
  pruneToLanguages,
  selectedCount
} from '$lib/resources'
import type { Availabilities, Cell, Selection } from '$lib/resources'

// What is available, by language code, as reported by the backend.
export const availabilityStore: Writable<Availabilities> = writable<Availabilities>({})

// What the user chose: a set of `lang|resource type|book` keys (see $lib/resources).
export const selectedResourcesStore: Writable<Selection> = writable<Selection>(new Set<string>())

// When two languages are chosen, choosing a resource in one also chooses
// the equivalent resource in the other.
export const syncLanguagesStore: Writable<boolean> = writable<boolean>(true)

// Number of chosen resources of books.
export const resourcesCountStore: Readable<number> = derived(selectedResourcesStore, (selected) =>
  selectedCount(selected)
)

// Number of chosen languages that have at least one resource chosen.
export const languagesWithResourcesCountStore: Readable<number> = derived(
  [selectedResourcesStore, langCodesStore],
  ([selected, langCodes]) => {
    const withResources = languagesWithSelection(selected)
    return langCodes.filter((lang) => withResources.has(lang)).length
  }
)

// "<lang>, <resource type code>, <resource type name>" for each
// resource type chosen in each language.
export const chosenResourceTypesStore: Readable<Array<string>> = derived(
  [selectedResourcesStore, langCodesStore, availabilityStore],
  ([selected, langCodes, availability]) => chosenResourceTypes(selected, langCodes, availability)
)

// Choose or unchoose cells, keeping the languages in sync if so desired.
export function applyToSelection(cells: Array<Cell>, on: boolean): void {
  const availability = get(availabilityStore)
  const langs = get(langCodesStore)
  const sync = get(syncLanguagesStore)
  selectedResourcesStore.update((selected) =>
    applyCells(selected, cells, on, availability, langs, sync)
  )
}

// Choices of languages that are not chosen any longer don't linger.
langCodesStore.subscribe((langs) => {
  selectedResourcesStore.update((selected) => {
    const pruned = pruneToLanguages(selected, langs)
    return pruned.size === selected.size ? selected : pruned
  })
})
