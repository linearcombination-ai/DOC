import { derived, writable } from 'svelte/store'
import type { Readable, Writable } from 'svelte/store'
import { chosenResourceTypesStore } from '$lib/stores/ResourcesStore'

// The resource types chosen, e.g., "en, ulb, Unlocked Literal Bible", one
// for each language and type. Derived from what the user chose on the
// resources page.
export const resourceTypesStore: Readable<Array<string>> = chosenResourceTypesStore
export const resourceTypesCountStore: Readable<number> = derived(
  chosenResourceTypesStore,
  (chosen) => chosen.length
)
export let usfmAvailableStore: Writable<boolean> = writable<boolean>(false)
export let limitTwStore: Writable<boolean> = writable<boolean>(true)
export let twResourceRequestedStore: Writable<boolean> = writable<boolean>(false)
