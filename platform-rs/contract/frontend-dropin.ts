// Drop-in check: the frontend's hand-written types for the seven endpoints (copied from the call
// sites named below) are assignable both ways to the types generated from the OpenAPI document.
// Compile: tsc --strict --noEmit frontend-dropin.ts
import type { components, paths } from './api'

type Ok<P extends keyof paths> = paths[P] extends { get: { responses: { 200: { content: { 'application/json': infer R } } } } } ? R : never

// frontend/src/routes/books/+page.svelte:28  Promise<Array<[string, string]>>
type FrontendBooks = Array<[string, string]>
// frontend/src/lib/utils.ts:113  Promise<Array<[string, string, boolean]>>
type FrontendLanguages = Array<[string, string, boolean]>
// frontend/src/routes/passages/passages/BibleReferenceSelector.svelte:16  Promise<Record<string, number[]>>
type FrontendChapters = Record<string, number[]>
// frontend/src/routes/resource_types/+page.svelte:36  const resourceTypesAndNames: Array<[string, string]>
type FrontendResourceTypes = Array<[string, string]>

type Both<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never

export const books: Both<Ok<'/book_codes_for_lang/{lang_code}'>, FrontendBooks> = true
export const booksUsfm: Both<Ok<'/book_codes_for_lang_from_usfm_only/{lang_code}'>, FrontendBooks> = true
export const shared: Both<Ok<'/shared_book_codes/{lang0_code}/{lang1_code}'>, FrontendBooks> = true
export const languages: Both<Ok<'/language_codes_and_names'>, FrontendLanguages> = true
export const chapters: Both<Ok<'/chapters_in_books'>, FrontendChapters> = true
export const resourceTypes: Both<Ok<'/resource_types/{lang_code}/{book_codes}'>, FrontendResourceTypes> = true
// the health response is still a tuple whose first element has `status: string` (P6 adds fields to it); the
// UI does not read it today, a client that reads index 0 .status or index 1 keeps working:
type Health = Ok<'/health/status'>
export const healthStatus: Health[0]['status'] extends string ? true : never = true
export const healthCode: Health[1] extends number ? true : never = true

// a consumer of the findings routes gets typed findings
export function codes(r: components['schemas']['WithFindings_Vec_BookEntry']): string[] {
  return r.findings.map((f) => f.code)
}

// ---------------------------------------------------------------------------------------------------------
// P6: the document endpoints, written the way frontend/src/routes/settings/GenerateDocument.svelte calls them
// (lines 78-108): the store types are copied from frontend/src/lib/stores/SettingsStore.ts:8-27.
type Req = components['schemas']['DocumentRequestBody']
type Created = components['schemas']['TaskCreated']
type Status = components['schemas']['TaskStatusBody']

declare const $layoutForPrintStore: boolean
declare const $assemblyStrategyKindStore: Req['assembly_strategy_kind'] // the store is typed `string`: see frontend-mismatch.ts
declare const $generatePdfStore: boolean
declare const $generateEpubStore: boolean
declare const $generateDocxStore: boolean
declare const $emailStore: string | null
declare const $limitTwStore: boolean
declare const $useChapterLabelsStore: boolean
declare const $useSectionVisualSeparatorStore: boolean
declare const $usePrinceStore: boolean
declare const $useTwoColumnLayoutForTnNotesStore: boolean
declare const $useTwoColumnLayoutForTqNotesStore: boolean
declare const $showTnBookIntroStore: boolean
declare const $showTnChapterIntroStore: boolean
declare const $showBcBookIntroStore: boolean
declare const $showBcChapterCommentaryStore: boolean

const resourceRequests: Req['resource_requests'] = []
resourceRequests.push({ lang_code: 'en', resource_type: 'ulb', book_code: 'jud' })

// the request exactly as GenerateDocument.svelte builds it (minus the one field the backend does not model)
export const documentRequest: Req = {
  email_address: $emailStore,
  assembly_strategy_kind: $assemblyStrategyKindStore,
  layout_for_print: $layoutForPrintStore,
  generate_pdf: $generatePdfStore,
  generate_epub: $generateEpubStore,
  generate_docx: $generateDocxStore,
  resource_requests: resourceRequests,
  document_request_source: 'ui',
  limit_words: $limitTwStore,
  use_chapter_labels: $useChapterLabelsStore,
  use_section_visual_separator: $useSectionVisualSeparatorStore,
  use_prince: $usePrinceStore,
  use_two_column_layout_for_tn_notes: $useTwoColumnLayoutForTnNotesStore,
  use_two_column_layout_for_tq_notes: $useTwoColumnLayoutForTqNotesStore,
  show_tn_book_intro: $showTnBookIntroStore,
  show_tn_chapter_intro: $showTnChapterIntroStore,
  show_bc_book_intro: $showBcBookIntroStore,
  show_bc_chapter_commentary: $showBcChapterCommentaryStore
}

// POST /documents_docx or /documents: the 200 body is { task_id: string }, read as `data.task_id`
export function taskIdOf(data: Created): string {
  return data.task_id
}
// poll(): `json?.state` and, when state is SUCCESS, `json?.result` as the document key (string)
export function pollResult(json: Status): string | [string, string] {
  const state = json.state
  if (state === 'SUCCESS' && typeof json.result === 'string') return [state, json.result]
  return state
}
// the error path: `$errorStore = data.detail` after a non-ok response (422 detail list, 503/501 detail string)
export function errorOf(data: components['schemas']['RequestValidationBody'] | components['schemas']['ApiProblem']): unknown {
  return data.detail
}
// the UI's progress strings are matched literally by TaskStatus.svelte:7-40; the status body keeps `state` a plain string
export const stateIsString: Status['state'] extends string ? true : never = true
