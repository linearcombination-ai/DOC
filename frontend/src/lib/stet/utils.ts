import { browser } from '$app/environment'
import { goto } from '$app/navigation'
import { env } from '$env/dynamic/public'
import { PUBLIC_LANGUAGE_BOOK_ORDER } from '$env/static/public'
import {
  gatewayCodeAndNamesStore,
  heartCodeAndNamesStore,
  lang0CodeAndNameStore,
  lang1CodeAndNameStore,
  langCodesStore,
  langCountStore
} from '$lib/stet/stores/LanguagesStore'
// import { otBookStore, ntBookStore, bookCountStore } from '$lib/stores/BooksStore'
// import {
//   lang0ResourceTypesStore,
//   lang1ResourceTypesStore,
//   resourceTypesCountStore
// } from '$lib/stores/ResourceTypesStore'
import { documentReadyStore, errorStore } from '$lib/stet/stores/NotificationStore'
import {
    documentRequestKeyStore
} from '$lib/stet/stores/SettingsStore'

// const languageBookOrder: string = <string>PUBLIC_LANGUAGE_BOOK_ORDER

type StoreGroup = 'source_languages' | 'target_languages' | 'settings' | 'notifications'

export let sourceLangRegExp = new RegExp('.*source_languages.*')
export let targetLangRegExp = new RegExp('.*target_languages.*')
export let settingsRegExp = new RegExp('.*settings.*')

export function resetStores(storeGroup: StoreGroup) {
  if (storeGroup === 'source_languages' || storeGroup === 'target_languages') {
    gatewayCodeAndNamesStore.set([])
    heartCodeAndNamesStore.set([])
    lang0CodeAndNameStore.set('')
    lang1CodeAndNameStore.set('')
    langCodesStore.set([])
    langCountStore.set(0)
  }

  // if (storeGroup === 'books') {
  //   otBookStore.set([])
  //   ntBookStore.set([])
  //   bookCountStore.set(0)
  // }

  // if (storeGroup === 'resource_types') {
  //   lang0ResourceTypesStore.set([])
  //   lang1ResourceTypesStore.set([])
  //   resourceTypesCountStore.set(0)
  // }

  if (storeGroup === 'settings') {
    // layoutForPrintStore.set(false)
    // assemblyStrategyKindStore.set(languageBookOrder)
    // generatePdfStore.set(true)
    // generateDocxStore.set(false)
    documentRequestKeyStore.set('')
    // twResourceRequestedStore.set(false)
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

export function routeToPage(url: string): void {
  if (browser) {
    goto(url)
  }
}
