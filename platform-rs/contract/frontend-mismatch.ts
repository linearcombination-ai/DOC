// Expected to FAIL: the two places where the frontend as written does not fit the generated contract.
// Compile: tsc --strict --noEmit frontend-mismatch.ts   (the log is kept in results/frontend-types.log)
import type { components } from './api'

type Req = components['schemas']['DocumentRequestBody']

// frontend/src/lib/stores/SettingsStore.ts:9  assemblyStrategyKindStore: Writable<string>
declare const $assemblyStrategyKindStore: string
// frontend/src/routes/settings/GenerateDocument.svelte:99  show_rg_chapter_commentary: $showRgChapterCommentaryStore
declare const $showRgChapterCommentaryStore: boolean

// mismatch 1: string is wider than 'lbo' | 'blo' | 'lvo' | 'bvo' | 'stet'
export const m1: Req = { assembly_strategy_kind: $assemblyStrategyKindStore, resource_requests: [] }

// mismatch 2: a field DOC's DocumentRequest does not have (pydantic ignores it; an object literal typed to the contract does not)
export const m2: Req = { assembly_strategy_kind: 'lbo', resource_requests: [], show_rg_chapter_commentary: $showRgChapterCommentaryStore }
