import { describe, expect, it } from 'vitest'
import {
  allCells,
  applyCells,
  bookCells,
  cellKey,
  chosenResourceTypes,
  commonSelection,
  counterpartCell,
  mismatches,
  removeUnavailable,
  resourceRequests,
  rowBooks,
  selectionState,
  setCells,
  typeCells,
  type Availabilities,
  type ResourceAvailability
} from './resources'

const type = (
  code: string,
  name: string,
  category: string,
  scope: 'book' | 'language' = 'book'
) => ({
  code,
  name,
  category,
  scope
})
const book = (code: string, name: string, testament: 'OT' | 'NT', resources: Array<string>) => ({
  code,
  name,
  testament,
  resources
})

// English: ulb for gen and mat, tn and condensed tn for mat, commentary for mat, tw
const en: ResourceAvailability = {
  lang_code: 'en',
  resource_types: [
    type('bc', 'Bible Commentary', 'commentary'),
    type('tn-condensed', 'Condensed Translation Notes', 'notes'),
    type('tn', 'Translation Notes', 'notes'),
    type('tw', 'Translation Words', 'words', 'language'),
    type('ulb', 'Unlocked Literal Bible', 'scripture')
  ],
  books: [
    book('gen', 'Genesis', 'OT', ['ulb']),
    book('mat', 'Matthew', 'NT', ['bc', 'tn-condensed', 'tn', 'ulb'])
  ],
  language_resources: ['tw']
}
// Russian style: scripture only for OT books, notes and questions for all
const ru: ResourceAvailability = {
  lang_code: 'ru',
  resource_types: [
    type('tn', 'Translation Notes', 'notes'),
    type('tq', 'Translation Questions', 'questions'),
    type('tw', 'Translation Words', 'words', 'language'),
    type('ulb', 'Unlocked Literal Bible', 'scripture')
  ],
  books: [
    book('gen', 'Бытие', 'OT', ['tn', 'tq', 'ulb']),
    book('mat', 'От Матфея', 'NT', ['tn', 'tq'])
  ],
  language_resources: ['tw']
}
// A heart language with regular scripture only
const hl: ResourceAvailability = {
  lang_code: 'hl',
  resource_types: [type('reg', 'Regular', 'scripture')],
  books: [book('gen', 'Gen', 'OT', ['reg']), book('mat', 'Mat', 'NT', ['reg'])],
  language_resources: []
}
const avs: Availabilities = { en, ru, hl }
const empty = new Set<string>()

describe('cells', () => {
  it('bookCells excludes condensed notes when regular notes exist', () => {
    expect(bookCells(en, 'mat').map((c) => c.type)).toEqual(['bc', 'tn', 'ulb'])
  })
  it('typeCells only covers books that have the type', () => {
    expect(typeCells(ru, 'ulb', ['gen', 'mat']).map((c) => c.book)).toEqual(['gen'])
  })
  it('allCells can be limited to a testament', () => {
    expect(allCells(ru, 'NT').map((c) => `${c.type}:${c.book}`)).toEqual(['tn:mat', 'tq:mat'])
  })
  it('rowBooks merges books of the languages in canonical order', () => {
    const merged: Availabilities = {
      a: { ...en, lang_code: 'a', books: [book('mat', 'Matthew', 'NT', ['ulb'])] },
      b: {
        ...ru,
        lang_code: 'b',
        books: [book('gen', 'G', 'OT', ['tn']), book('mat', 'M', 'NT', ['tn'])]
      }
    }
    expect(rowBooks(merged, ['a', 'b']).map((r) => r.code)).toEqual(['gen', 'mat'])
  })
})

describe('setCells', () => {
  it('selects and unselects', () => {
    const on = setCells(empty, bookCells(ru, 'gen'), true)
    expect([...on].sort()).toEqual(['ru|tn|gen', 'ru|tq|gen', 'ru|ulb|gen'])
    expect(setCells(on, bookCells(ru, 'gen'), false).size).toBe(0)
  })
  it('regular and condensed notes exclude each other, last one wins', () => {
    let sel = setCells(empty, [{ lang: 'en', type: 'tn', book: 'mat' }], true)
    sel = setCells(sel, [{ lang: 'en', type: 'tn-condensed', book: 'mat' }], true)
    expect([...sel]).toEqual(['en|tn-condensed|mat'])
    sel = setCells(sel, [{ lang: 'en', type: 'tn', book: 'mat' }], true)
    expect([...sel]).toEqual(['en|tn|mat'])
  })
  it('a language wide resource needs a book scoped one', () => {
    const tw = { lang: 'en', type: 'tw', book: '' }
    expect(setCells(empty, [tw], true).size).toBe(0)
    const both = setCells(empty, [tw, { lang: 'en', type: 'ulb', book: 'gen' }], true)
    expect(both.size).toBe(2)
    // removing the last book scoped resource drops tw too
    expect(setCells(both, [{ lang: 'en', type: 'ulb', book: 'gen' }], false).size).toBe(0)
  })
  it('selectionState', () => {
    const cells = bookCells(ru, 'gen')
    expect(selectionState(empty, cells)).toBe('none')
    expect(selectionState(setCells(empty, [cells[0]], true), cells)).toBe('some')
    expect(selectionState(setCells(empty, cells, true), cells)).toBe('all')
  })
})

describe('counterparts', () => {
  it('matches scripture by category not by type code', () => {
    expect(counterpartCell(en, hl, { lang: 'en', type: 'ulb', book: 'gen' })).toEqual({
      lang: 'hl',
      type: 'reg',
      book: 'gen'
    })
  })
  it('is null when the other language lacks the resource for the book', () => {
    expect(counterpartCell(en, ru, { lang: 'en', type: 'ulb', book: 'mat' })).toBeNull()
    expect(counterpartCell(en, ru, { lang: 'en', type: 'bc', book: 'mat' })).toBeNull()
  })
  it('condensed notes stand in for regular notes and vice versa', () => {
    expect(counterpartCell(en, ru, { lang: 'en', type: 'tn-condensed', book: 'mat' })?.type).toBe(
      'tn'
    )
    expect(counterpartCell(ru, en, { lang: 'ru', type: 'tn', book: 'mat' })?.type).toBe('tn')
  })
  it('language wide resources match by code', () => {
    expect(counterpartCell(en, ru, { lang: 'en', type: 'tw', book: '' })).toEqual({
      lang: 'ru',
      type: 'tw',
      book: ''
    })
    expect(counterpartCell(en, hl, { lang: 'en', type: 'tw', book: '' })).toBeNull()
  })
})

describe('applyCells with the languages kept in sync', () => {
  const langs = ['en', 'hl']
  it('selects the equivalent resource in the other language', () => {
    const sel = applyCells(
      empty,
      [{ lang: 'en', type: 'ulb', book: 'gen' }],
      true,
      avs,
      langs,
      true
    )
    expect([...sel].sort()).toEqual(['en|ulb|gen', 'hl|reg|gen'])
  })
  it('unselects it too', () => {
    const on = applyCells(empty, bookCells(en, 'mat'), true, avs, langs, true)
    const off = applyCells(on, [{ lang: 'en', type: 'ulb', book: 'mat' }], false, avs, langs, true)
    expect(off.has(cellKey('hl', 'reg', 'mat'))).toBe(false)
    expect(off.has(cellKey('en', 'tn', 'mat'))).toBe(true)
  })
  it('does nothing to the other language when not in sync', () => {
    const sel = applyCells(
      empty,
      [{ lang: 'en', type: 'ulb', book: 'gen' }],
      true,
      avs,
      langs,
      false
    )
    expect([...sel]).toEqual(['en|ulb|gen'])
  })
  it('a resource the other language lacks is still selectable', () => {
    const sel = applyCells(empty, [{ lang: 'en', type: 'bc', book: 'mat' }], true, avs, langs, true)
    expect([...sel]).toEqual(['en|bc|mat'])
  })
})

describe('commonSelection', () => {
  it('chooses only what both languages have, matching scripture by category', () => {
    const sel = commonSelection(avs, ['en', 'ru'])
    expect([...sel].sort()).toEqual([
      'en|tn|mat',
      'en|tw|',
      'en|ulb|gen',
      'ru|tn|mat',
      'ru|tw|',
      'ru|ulb|gen'
    ])
  })
  it('pairs different scripture types', () => {
    const sel = commonSelection(avs, ['en', 'hl'])
    expect([...sel].sort()).toEqual(['en|ulb|gen', 'en|ulb|mat', 'hl|reg|gen', 'hl|reg|mat'])
  })
})

describe('mismatches', () => {
  it('reports resources the other language does not have', () => {
    const sel = setCells(empty, [{ lang: 'en', type: 'ulb', book: 'mat' }], true)
    expect(mismatches(sel, avs, ['en', 'ru'])).toEqual([
      { lang: 'en', otherLang: 'ru', type: 'ulb', book: 'mat', reason: 'unavailable' }
    ])
  })
  it('reports resources selected in only one language', () => {
    const sel = setCells(empty, [{ lang: 'en', type: 'ulb', book: 'gen' }], true)
    expect(mismatches(sel, avs, ['en', 'hl'])).toEqual([
      { lang: 'en', otherLang: 'hl', type: 'ulb', book: 'gen', reason: 'unselected' }
    ])
  })
  it('reports nothing when equivalents are chosen in both', () => {
    const sel = commonSelection(avs, ['en', 'hl'])
    expect(mismatches(sel, avs, ['en', 'hl'])).toEqual([])
  })
  it('removeUnavailable drops only the unavailable ones', () => {
    const sel = setCells(
      empty,
      [
        { lang: 'en', type: 'ulb', book: 'mat' },
        { lang: 'en', type: 'ulb', book: 'gen' }
      ],
      true
    )
    expect([...removeUnavailable(sel, avs, ['en', 'ru'])]).toEqual(['en|ulb|gen'])
  })
  it('is empty for one language', () => {
    expect(mismatches(commonSelection(avs, ['en']), avs, ['en'])).toEqual([])
  })
})

describe('resourceRequests', () => {
  it('orders by language, canonical book order and resource type order', () => {
    // selected in a scrambled order
    const sel = setCells(
      empty,
      [
        { lang: 'ru', type: 'tq', book: 'mat' },
        { lang: 'en', type: 'ulb', book: 'mat' },
        { lang: 'en', type: 'tn', book: 'mat' },
        { lang: 'en', type: 'ulb', book: 'gen' },
        { lang: 'ru', type: 'ulb', book: 'gen' },
        { lang: 'ru', type: 'tn', book: 'gen' }
      ],
      true
    )
    expect(
      resourceRequests(sel, ['en', 'ru'], avs).map(
        (r) => `${r.lang_code}-${r.resource_type}-${r.book_code}`
      )
    ).toEqual(['en-ulb-gen', 'en-tn-mat', 'en-ulb-mat', 'ru-tn-gen', 'ru-ulb-gen', 'ru-tq-mat'])
  })
  it('requests a language wide resource for each book of the language, in name order', () => {
    const sel = setCells(
      empty,
      [
        { lang: 'en', type: 'tw', book: '' },
        { lang: 'en', type: 'ulb', book: 'mat' },
        { lang: 'en', type: 'tn', book: 'mat' },
        { lang: 'en', type: 'ulb', book: 'gen' }
      ],
      true
    )
    expect(
      resourceRequests(sel, ['en'], avs).map((r) => `${r.resource_type}-${r.book_code}`)
    ).toEqual(['tw-gen', 'ulb-gen', 'tn-mat', 'tw-mat', 'ulb-mat'])
  })
  it('ignores languages that are no longer chosen', () => {
    const sel = commonSelection(avs, ['en', 'hl'])
    expect(resourceRequests(sel, ['en'], avs).every((r) => r.lang_code === 'en')).toBe(true)
  })
})

describe('chosenResourceTypes', () => {
  it('lists each chosen type once per language', () => {
    const sel = commonSelection(avs, ['en', 'ru'])
    expect(chosenResourceTypes(sel, ['en', 'ru'], avs)).toEqual([
      'en, tn, Translation Notes',
      'en, tw, Translation Words',
      'en, ulb, Unlocked Literal Bible',
      'ru, tn, Translation Notes',
      'ru, tw, Translation Words',
      'ru, ulb, Unlocked Literal Bible'
    ])
  })
})
