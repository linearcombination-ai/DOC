<script lang="ts">
  // This page exists to service the use case where BIEL wants to hand off
  // to DOC by providing only a repo URL and have DOC populate state
  // as if the user had started on DOC. Here is an example URL that
  // would work:
  // http(s)://<DOC base url here>/transfer/repo_url=https%3A%2F%2Fcontent.bibletranslationtools.org%2Fbahasatech.indotengah%2Fbne_gal_text_reg&book_name=Galatians
  // or
  // http(s)://<DOC base url here>/transfer/repo_url=https:%2F%2Fcontent.bibletranslationtools.org%2FWycliffeAssociates%2Fen_ulb

  import type { PageData } from './$types'
  import {
    languagesClickedOrderStore,
    langCodesStore,
    langNamesStore,
    langCountStore
  } from '$lib/stores/LanguagesStore'
  import { availabilityStore, selectedResourcesStore } from '$lib/stores/ResourcesStore'
  import ProgressIndicator from '$lib/ProgressIndicator.svelte'
  import { routeToPage, getLangCodesNames } from '$lib/utils'
  import { fetchResourceAvailability } from '$lib/resources-api'
  import { LANGUAGE_WIDE, setCells } from '$lib/resources'
  import type { Cell } from '$lib/resources'

  export let data: PageData

  let message: string = ''
  let errorMessage: string = ''

  // Choose the language and the resource (of a book, or of all the books
  // the language has the resource for) that BIEL asked for. Then go to the
  // review step, or, if nothing that was asked for is available, to the
  // resources step so that the user can choose.
  async function transfer(langCode: string, resource: string, bookCode?: string) {
    const langCodeNameAndTypes = await getLangCodesNames()
    const language = langCodeNameAndTypes.find(([code]) => code === langCode)
    if (!language) {
      message = `Unfortunately, you've requested a language code, ${langCode}, which is not available.`
      return
    }
    const [code, name] = language
    $languagesClickedOrderStore = [`${code}, ${name}`]
    $langCodesStore = [code]
    $langNamesStore = [name]
    $langCountStore = 1
    const availability = await fetchResourceAvailability(code)
    $availabilityStore = { ...$availabilityStore, [code]: availability }
    const cells: Array<Cell> = availability.books
      .filter(
        (book) =>
          book.resources.includes(resource) && (bookCode === undefined || book.code === bookCode)
      )
      .map((book) => ({ lang: code, type: resource, book: book.code }))
    if (availability.language_resources.includes(resource)) {
      cells.push({ lang: code, type: resource, book: LANGUAGE_WIDE })
    }
    $selectedResourcesStore = setCells(new Set<string>(), cells, true)
    routeToPage($selectedResourcesStore.size > 0 ? '/settings' : '/resources')
  }

  const components = data.repo?.split('_') || []
  if (components.length === 4) {
    // e.g., repo_url=https%3A%2F%2Fcontent.bibletranslationtools.org%2Fbahasatech.indotengah%2Fbne_gal_text_reg&book_name=Galatians
    const [langCode, bookCode, , resource] = components
    console.log(
      `Transferred values from BIEL, langCode: ${langCode}, bookCode: ${bookCode}, resource: ${resource}`
    )
    message = `You've requested the following values, langCode: ${langCode}, bookCode: ${bookCode}, resource: ${resource}`
    transfer(langCode, resource, bookCode).catch((err) => console.error(err))
  } else if (components.length === 2) {
    // e.g., repo_url=https:%2F%2Fcontent.bibletranslationtools.org%2FWycliffeAssociates%2Fen_ulb
    const [langCode, resource] = components
    console.log(
      `Transferred values from BIEL, langCode: ${langCode}, all books, resource: ${resource}`
    )
    message = `You've requested the following values, langCode: ${langCode}, all books, resource: ${resource}`
    transfer(langCode, resource).catch((err) => console.error(err))
  } else {
    errorMessage =
      "We're sorry the information transferred from bibleineverylanguage.org is not available in this system. Please contact support."
    console.log(`repo url: ${data.repo}`)
    console.log(`components of transfer url: ${components}`)
  }
</script>

<div class="ml-8 mt-8 text-[#33445C]">
  <div class="p-4">
    <p>{message}</p>
  </div>
  {#if !errorMessage}
    <div class="mt-4 p-4">
      <p>
        We are initializing and validating your request so far, this will take a few moments and
        then you will be transferred to the review page...
      </p>
    </div>
    <div class="mt-4 p-4">
      <ProgressIndicator />
    </div>
  {:else}
    <div class="mt-4 p-4">
      <p>{errorMessage}</p>
    </div>
  {/if}
</div>
