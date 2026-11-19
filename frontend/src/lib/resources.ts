// Pure logic for the "Choose resources" step. Nothing in here touches the
// DOM, stores or the network so it is straightforward to unit test.
//
// The user's selection is a set of keys, one per chosen resource:
//   `${langCode}|${resourceType}|${bookCode}`   a resource of a book
//   `${langCode}|${resourceType}|`              a language wide resource, e.g., tw
// The selection maps one to one onto the backend's resource_requests.
import { booksMap } from '$lib/bible-books'

export type ResourceScope = 'book' | 'language'

export type ResourceTypeInfo = {
  code: string
  name: string
  category: string
  scope: ResourceScope
}

export type BookAvailability = {
  code: string
  name: string
  testament: 'OT' | 'NT'
  resources: Array<string>
}

export type ResourceAvailability = {
  lang_code: string
  resource_types: Array<ResourceTypeInfo>
  books: Array<BookAvailability>
  language_resources: Array<string>
}

export type Availabilities = Record<string, ResourceAvailability>
export type Selection = ReadonlySet<string>
export type Cell = { lang: string; type: string; book: string }

export type ResourceRequest = {
  lang_code: string
  resource_type: string
  book_code: string
}

export type Mismatch = {
  lang: string
  otherLang: string
  type: string
  book: string
  // unavailable: the other language has no equivalent resource
  // unselected: the other language has an equivalent but it is not selected
  reason: 'unavailable' | 'unselected'
}

export const LANGUAGE_WIDE = ''
export const TN = 'tn'
export const TN_CONDENSED = 'tn-condensed'

const BOOK_ORDER: Array<string> = Object.keys(booksMap)
const bookIndex = (code: string): number => {
  const index = BOOK_ORDER.indexOf(code)
  return index === -1 ? BOOK_ORDER.length : index
}

export function cellKey(lang: string, type: string, book: string = LANGUAGE_WIDE): string {
  return `${lang}|${type}|${book}`
}

export function parseKey(key: string): Cell {
  const [lang, type, book] = key.split('|')
  return { lang, type, book: book ?? LANGUAGE_WIDE }
}

const keyOf = (cell: Cell): string => cellKey(cell.lang, cell.type, cell.book)

export function typeInfo(av: ResourceAvailability, code: string): ResourceTypeInfo | undefined {
  return av.resource_types.find((type) => type.code === code)
}

export function isLanguageWide(av: ResourceAvailability, code: string): boolean {
  return av.language_resources.includes(code)
}

export function isAvailable(av: ResourceAvailability, type: string, book: string): boolean {
  if (isLanguageWide(av, type)) return book === LANGUAGE_WIDE
  return av.books.find((b) => b.code === book)?.resources.includes(type) ?? false
}

// The order of a language's resource types, which is the order the document
// generator uses for the resources of a book (the backend sorts by name).
function typeOrder(av: ResourceAvailability, type: string): number {
  return av.resource_types.findIndex((t) => t.code === type)
}

// Resources for a book that "Select all" style actions choose. Condensed
// notes are only chosen when regular notes are not an option, they exclude each other.
function defaultResourcesOf(resources: Array<string>): Array<string> {
  return resources.includes(TN) ? resources.filter((r) => r !== TN_CONDENSED) : resources
}

// All the book scoped cells available for a book.
export function bookCells(av: ResourceAvailability, bookCode: string): Array<Cell> {
  const book = av.books.find((b) => b.code === bookCode)
  if (!book) return []
  return defaultResourcesOf(book.resources).map((type) => ({
    lang: av.lang_code,
    type,
    book: bookCode
  }))
}

// All the cells of a type available among the given books.
export function typeCells(
  av: ResourceAvailability,
  type: string,
  bookCodes: Array<string>
): Array<Cell> {
  if (isLanguageWide(av, type)) return [{ lang: av.lang_code, type, book: LANGUAGE_WIDE }]
  return av.books
    .filter((b) => bookCodes.includes(b.code) && b.resources.includes(type))
    .map((b) => ({ lang: av.lang_code, type, book: b.code }))
}

// Every book scoped cell available in a language, optionally one testament only.
export function allCells(av: ResourceAvailability, testament?: 'OT' | 'NT'): Array<Cell> {
  return av.books
    .filter((b) => !testament || b.testament === testament)
    .flatMap((b) => bookCells(av, b.code))
}

// Everything a language has, language wide resources included.
export function everythingCells(av: ResourceAvailability): Array<Cell> {
  return [
    ...allCells(av),
    ...av.language_resources.map((type) => ({ lang: av.lang_code, type, book: LANGUAGE_WIDE }))
  ]
}

// Rows of the grid: the union of the books of the languages chosen in canonical order.
export function rowBooks(
  avs: Availabilities,
  langs: Array<string>
): Array<{ code: string; name: string; testament: 'OT' | 'NT' }> {
  const rows = new Map<string, { code: string; name: string; testament: 'OT' | 'NT' }>()
  for (const lang of langs) {
    for (const book of avs[lang]?.books ?? []) {
      if (!rows.has(book.code)) {
        rows.set(book.code, { code: book.code, name: book.name, testament: book.testament })
      }
    }
  }
  return [...rows.values()].sort((a, b) => bookIndex(a.code) - bookIndex(b.code))
}

// Drop keys of languages that are no longer chosen.
export function pruneToLanguages(sel: Selection, langs: Array<string>): Selection {
  return new Set([...sel].filter((key) => langs.includes(parseKey(key).lang)))
}

// Language wide resources only make sense together with at least one book
// scoped resource of the language (a request needs a book).
export function normalizeSelection(sel: Selection): Selection {
  const langsWithBooks = new Set<string>()
  for (const key of sel) {
    const cell = parseKey(key)
    if (cell.book !== LANGUAGE_WIDE) langsWithBooks.add(cell.lang)
  }
  return new Set(
    [...sel].filter((key) => {
      const cell = parseKey(key)
      return cell.book !== LANGUAGE_WIDE || langsWithBooks.has(cell.lang)
    })
  )
}

// Add or remove cells. Regular and condensed translation notes exclude each
// other for a book: choosing one unchooses the other.
export function setCells(sel: Selection, cells: Array<Cell>, on: boolean): Selection {
  const next = new Set(sel)
  for (const cell of cells) {
    if (on) {
      next.add(keyOf(cell))
      if (cell.type === TN) next.delete(cellKey(cell.lang, TN_CONDENSED, cell.book))
      if (cell.type === TN_CONDENSED) next.delete(cellKey(cell.lang, TN, cell.book))
    } else {
      next.delete(keyOf(cell))
    }
  }
  return normalizeSelection(next)
}

export function isSelected(sel: Selection, cell: Cell): boolean {
  return sel.has(keyOf(cell))
}

// How selected a group of cells is: none, some or all of them.
export function selectionState(sel: Selection, cells: Array<Cell>): 'none' | 'some' | 'all' {
  if (cells.length === 0) return 'none'
  const selected = cells.filter((cell) => sel.has(keyOf(cell))).length
  if (selected === 0) return 'none'
  return selected === cells.length ? 'all' : 'some'
}

// The cell in another language that stands in for a cell, matched by
// category (scripture, notes, ...) rather than by resource type code, e.g., ulb in one
// language stands in for reg in another. A same code resource is preferred.
export function counterpartCell(
  source: ResourceAvailability,
  target: ResourceAvailability,
  cell: Cell
): Cell | null {
  const base = { lang: target.lang_code }
  if (isLanguageWide(source, cell.type)) {
    return isLanguageWide(target, cell.type)
      ? { ...base, type: cell.type, book: LANGUAGE_WIDE }
      : null
  }
  const category = typeInfo(source, cell.type)?.category
  const candidates = defaultResourcesOf(
    target.books.find((b) => b.code === cell.book)?.resources ?? []
  ).filter((type) => !isLanguageWide(target, type) && typeInfo(target, type)?.category === category)
  if (candidates.length === 0) return null
  const type = candidates.includes(cell.type) ? cell.type : candidates[0]
  return { ...base, type, book: cell.book }
}

// Change cells and, when the languages are kept in sync, the equivalent
// cells of the other language.
export function applyCells(
  sel: Selection,
  cells: Array<Cell>,
  on: boolean,
  avs: Availabilities,
  langs: Array<string>,
  sync: boolean
): Selection {
  let next = setCells(sel, cells, on)
  if (sync && langs.length === 2) {
    const counterparts: Array<Cell> = []
    for (const cell of cells) {
      const sourceAv = avs[cell.lang]
      const otherLang = langs.find((lang) => lang !== cell.lang)
      const targetAv = otherLang ? avs[otherLang] : undefined
      if (!sourceAv || !targetAv) continue
      const counterpart = counterpartCell(sourceAv, targetAv, cell)
      if (counterpart) counterparts.push(counterpart)
    }
    next = setCells(next, counterparts, on)
  }
  return next
}

// Choose what both languages have: for every book and category (scripture,
// notes, ...) that both languages provide, choose each language's resource
// of that category. Categories are matched, type codes need not be the same.
export function commonSelection(avs: Availabilities, langs: Array<string>): Selection {
  if (langs.length !== 2) {
    return setCells(
      new Set(),
      langs.flatMap((lang) => (avs[lang] ? everythingCells(avs[lang]) : [])),
      true
    )
  }
  const [a, b] = langs.map((lang) => avs[lang])
  if (!a || !b) return new Set()
  const cells: Array<Cell> = []
  for (const row of rowBooks(avs, langs)) {
    for (const cell of bookCells(a, row.code)) {
      const other = counterpartCell(a, b, cell)
      if (other) cells.push(cell, other)
    }
  }
  for (const type of a.language_resources) {
    if (isLanguageWide(b, type)) {
      cells.push(
        { lang: a.lang_code, type, book: LANGUAGE_WIDE },
        { lang: b.lang_code, type, book: LANGUAGE_WIDE }
      )
    }
  }
  return setCells(new Set(), cells, true)
}

// Selected resources that the other language does not have, or has but
// that are not chosen for it. Only meaningful with two languages.
export function mismatches(
  sel: Selection,
  avs: Availabilities,
  langs: Array<string>
): Array<Mismatch> {
  if (langs.length !== 2) return []
  const found: Array<Mismatch> = []
  for (const key of sel) {
    const cell = parseKey(key)
    const otherLang = langs.find((lang) => lang !== cell.lang)
    const source = avs[cell.lang]
    const target = otherLang ? avs[otherLang] : undefined
    if (!otherLang || !source || !target || !langs.includes(cell.lang)) continue
    const counterpart = counterpartCell(source, target, cell)
    if (!counterpart) {
      found.push({
        lang: cell.lang,
        otherLang,
        type: cell.type,
        book: cell.book,
        reason: 'unavailable'
      })
    } else if (!sel.has(keyOf(counterpart)) && !hasSameCategoryChosen(sel, target, counterpart)) {
      found.push({
        lang: cell.lang,
        otherLang,
        type: cell.type,
        book: cell.book,
        reason: 'unselected'
      })
    }
  }
  return found
}

function hasSameCategoryChosen(sel: Selection, av: ResourceAvailability, cell: Cell): boolean {
  const category = typeInfo(av, cell.type)?.category
  return [...sel].some((key) => {
    const other = parseKey(key)
    return (
      other.lang === av.lang_code &&
      other.book === cell.book &&
      typeInfo(av, other.type)?.category === category
    )
  })
}

// Remove the selected resources the other language does not have.
export function removeUnavailable(
  sel: Selection,
  avs: Availabilities,
  langs: Array<string>
): Selection {
  const cells = mismatches(sel, avs, langs)
    .filter((m) => m.reason === 'unavailable')
    .map((m) => ({ lang: m.lang, type: m.type, book: m.book }))
  return setCells(sel, cells, false)
}

// The resource_requests of a document request. Order is language, then
// canonical book order, then the language's resource type order, the same
// order the document generator has always received so that document
// request keys stay stable. A language wide resource, e.g., tw, is requested
// once for each book the language has chosen, as it always has been.
export function resourceRequests(
  sel: Selection,
  langs: Array<string>,
  avs: Availabilities
): Array<ResourceRequest> {
  const requests: Array<ResourceRequest> = []
  for (const lang of langs) {
    const av = avs[lang]
    if (!av) continue
    const cells = [...sel].map(parseKey).filter((cell) => cell.lang === lang)
    const books = [
      ...new Set(cells.filter((c) => c.book !== LANGUAGE_WIDE).map((c) => c.book))
    ].sort((a, b) => bookIndex(a) - bookIndex(b))
    const chosenTypes = new Set(cells.map((c) => c.type))
    const types = av.resource_types.map((t) => t.code).filter((code) => chosenTypes.has(code))
    for (const book of books) {
      for (const type of types) {
        if (isLanguageWide(av, type) || sel.has(cellKey(lang, type, book))) {
          requests.push({ lang_code: lang, resource_type: type, book_code: book })
        }
      }
    }
  }
  return requests
}

// "<lang>, <type code>, <type name>" strings, one per language and resource
// type chosen, the shape the review step has always worked with.
export function chosenResourceTypes(
  sel: Selection,
  langs: Array<string>,
  avs: Availabilities
): Array<string> {
  const chosen: Array<string> = []
  for (const lang of langs) {
    const av = avs[lang]
    if (!av) continue
    const types = new Set(
      [...sel]
        .map(parseKey)
        .filter((c) => c.lang === lang)
        .map((c) => c.type)
    )
    for (const type of av.resource_types) {
      if (types.has(type.code)) chosen.push(`${lang}, ${type.code}, ${type.name}`)
    }
  }
  return chosen
}

// Number of selected resources of books, i.e., excluding language wide ones.
export function selectedCount(sel: Selection): number {
  return [...sel].filter((key) => parseKey(key).book !== LANGUAGE_WIDE).length
}

// Number of languages with at least one book scoped resource selected.
export function languagesWithSelection(sel: Selection): Set<string> {
  return new Set(
    [...sel]
      .map(parseKey)
      .filter((cell) => cell.book !== LANGUAGE_WIDE)
      .map((cell) => cell.lang)
  )
}

// The book scoped selection of a language grouped by resource type, in the
// type order of the language and canonical book order.
export function groupedByType(
  sel: Selection,
  lang: string,
  av: ResourceAvailability | undefined
): Array<{ type: string; name: string; books: Array<string> }> {
  const cells = [...sel].map(parseKey).filter((c) => c.lang === lang)
  const codes = [...new Set(cells.map((c) => c.type))]
  const order = (code: string) => (av ? typeOrder(av, code) : 0)
  return codes
    .sort((a, b) => order(a) - order(b))
    .map((type) => ({
      type,
      name: (av && typeInfo(av, type)?.name) || type,
      books: cells
        .filter((c) => c.type === type && c.book !== LANGUAGE_WIDE)
        .map((c) => c.book)
        .sort((a, b) => bookIndex(a) - bookIndex(b))
    }))
}
