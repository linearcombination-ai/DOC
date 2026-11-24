import { writable } from 'svelte/store'
import type { Writable } from 'svelte/store'
// import { PUBLIC_LANGUAGE_BOOK_ORDER, PUBLIC_CHUNK_SIZE_CHAPTER } from '$env/static/public'

// const groupingOrderDefault: string = <string>PUBLIC_LANGUAGE_BOOK_ORDER
// const chunkSizeDefault: string = <string>PUBLIC_CHUNK_SIZE_CHAPTER

// export let layoutForPrintStore: Writable<boolean> = writable<boolean>(false)
// export let limitTwStore: Writable<boolean> = writable<boolean>(true)
// export let assemblyStrategyKindStore: Writable<string> = writable<string>(groupingOrderDefault)
// export let assemblyStrategyChunkSizeStore: Writable<string> = writable<string>(chunkSizeDefault)
export let docTypeStore: Writable<string> = writable<string>('docx')
// export let generatePdfStore: Writable<boolean> = writable<boolean>(true)
// export let generateDocxStore: Writable<boolean> = writable<boolean>(false)
export let emailStore: Writable<string | null> = writable<string | null>(null)
export let documentRequestKeyStore: Writable<string> = writable<string>('')
export let settingsUpdatedStore: Writable<boolean> = writable<boolean>(false)
// export let twResourceRequestedStore: Writable<boolean> = writable<boolean>(false)
export let useIncreasedLineSpacingStore: Writable<boolean> = writable<boolean>(true)
