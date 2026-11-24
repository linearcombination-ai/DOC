import { browser } from '$app/environment'
import { goto } from '$app/navigation'
import { env } from '$env/dynamic/public'
import { PUBLIC_LANGUAGE_BOOK_ORDER } from '$env/static/public'
import {
  gatewayCodeAndNamesStore,
  heartCodeAndNamesStore,
  lang0NameAndCodeStore,
  lang1NameAndCodeStore,
  langCodesStore,
  langNamesStore,
  langCountStore
} from '$lib/stores/LanguagesStore'
import { otBookStore, ntBookStore, bookCountStore } from '$lib/stores/BooksStore'
import {
  lang0ResourceTypesStore,
  lang1ResourceTypesStore,
  resourceTypesCountStore,
  twResourceRequestedStore
} from '$lib/stores/ResourceTypesStore'
import { documentReadyStore, errorStore } from '$lib/stores/NotificationStore'
import {
  layoutForPrintStore,
  assemblyStrategyKindStore,
  generatePdfStore,
  generateDocxStore,
  documentRequestKeyStore
} from '$lib/stores/SettingsStore'
import { PUBLIC_LANG_CODES_NAMES_URL, PUBLIC_TAILWIND_SM_MIN_WIDTH } from '$env/static/public'

const languageBookOrder: string = <string>PUBLIC_LANGUAGE_BOOK_ORDER

type StoreGroup = 'languages' | 'books' | 'resource_types' | 'settings' | 'notifications'

export let langRegExp = new RegExp('.*languages.*')
export let bookRegExp = new RegExp('.*books.*')
export let resourceTypeRegExp = new RegExp('.*resource_types.*')
export let settingsRegExp = new RegExp('.*settings.*')

export function resetStores(storeGroup: StoreGroup) {
  if (storeGroup === 'languages') {
    gatewayCodeAndNamesStore.set([])
    heartCodeAndNamesStore.set([])
    lang0NameAndCodeStore.set('')
    lang1NameAndCodeStore.set('')
    langCodesStore.set([])
    langNamesStore.set([])
    langCountStore.set(0)
  }

  if (storeGroup === 'books') {
    otBookStore.set([])
    ntBookStore.set([])
    bookCountStore.set(0)
  }

  if (storeGroup === 'resource_types') {
    lang0ResourceTypesStore.set([])
    lang1ResourceTypesStore.set([])
    resourceTypesCountStore.set(0)
  }

  if (storeGroup === 'settings') {
    layoutForPrintStore.set(false)
    assemblyStrategyKindStore.set(languageBookOrder)
    generatePdfStore.set(true)
    generateDocxStore.set(false)
    documentRequestKeyStore.set('')
    twResourceRequestedStore.set(false)
  }

  if (storeGroup === 'notifications') {
    documentReadyStore.set(false)
    errorStore.set(null)
  }
}

export function getName(codeAndName: string): string {
  return codeAndName?.split(/, (.*)/s)[1]
}
export function getCode(codeAndName: string): string {
  return codeAndName?.split(/, (.*)/s)[0]
}

export function getResourceTypeName(codeAndName: string): string {
  return codeAndName?.split(', ')[2]
}
export function getResourceTypeCode(codeAndName: string): string {
  return codeAndName?.split(', ')[1]
}
export function getResourceTypeLangCode(codeAndName: string): string {
  return codeAndName?.split(', ')[0]
}

export function routeToPage(url: string): void {
  if (browser) {
    goto(url)
  }
}

export function handleError(err: unknown): string | null {
  if (typeof err === 'string') {
    return err
  }
  if (err instanceof Error) {
    return err.message
  }
  return null
}

export async function getLangCodesNames(
  apiRootUrl: string = env.PUBLIC_BACKEND_API_URL,
  langCodesAndNamesUrl: string = <string>PUBLIC_LANG_CODES_NAMES_URL
): Promise<Array<[string, string, boolean]>> {
  const response = await fetch(`${apiRootUrl}${langCodesAndNamesUrl}`)
  if (!response.ok) {
    console.log(`Error: ${response.statusText}`)
    throw new Error(response.statusText)
  }
  return await response.json()
}

export async function loadLangCodesNames(): Promise<
  [Array<[string, string, boolean]>, Array<string>, Array<string>]
> {
  const langCodeNameAndTypes = await getLangCodesNames()
  const gatewayCodesAndNames = langCodeNameAndTypes
    .filter(([, , isGateway]) => isGateway)
    .map(([code, name]) => `${code}, ${name}`)
  const heartCodesAndNames = langCodeNameAndTypes
    .filter(([, , isGateway]) => !isGateway)
    .map(([code, name]) => `${code}, ${name}`)
  return [langCodeNameAndTypes, gatewayCodesAndNames, heartCodesAndNames]
}
