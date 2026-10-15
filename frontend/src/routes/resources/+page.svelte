<script lang="ts">
  import { onMount } from 'svelte'
  import { PUBLIC_TAILWIND_SM_MIN_WIDTH } from '$env/static/public'
  import WizardBreadcrumb from '$lib/WizardBreadcrumb.svelte'
  import WizardBasket from '$lib/WizardBasket.svelte'
  import WizardBasketModal from '$lib/WizardBasketModal.svelte'
  import ResourceMatrix from '$lib/ResourceMatrix.svelte'
  import ResourceAccordion from '$lib/ResourceAccordion.svelte'
  import ProgressIndicator from '$lib/ProgressIndicator.svelte'
  import CheckIcon from '$lib/CheckIcon.svelte'
  import ErrorAlertIcon from '$lib/ErrorAlertIcon.svelte'
  import { langCodesStore, langNamesStore, langCountStore } from '$lib/stores/LanguagesStore'
  import {
    availabilityStore,
    selectedResourcesStore,
    syncLanguagesStore,
    resourcesCountStore,
    applyToSelection
  } from '$lib/stores/ResourcesStore'
  import { fetchResourceAvailability } from '$lib/resources-api'
  import {
    allCells,
    commonSelection,
    everythingCells,
    isSelected,
    languagesWithSelection,
    mismatches,
    removeUnavailable,
    rowBooks,
    typeInfo,
    parseKey
  } from '$lib/resources'
  import otBooks from '$lib/ot-books'

  let loading: Array<string> = []
  let failed: Array<string> = []

  async function load(lang: string) {
    loading = [...loading, lang]
    try {
      const availability = await fetchResourceAvailability(lang)
      $availabilityStore = { ...$availabilityStore, [lang]: availability }
    } catch (err) {
      console.error(err)
      failed = [...failed, lang]
    } finally {
      loading = loading.filter((l) => l !== lang)
    }
  }

  onMount(() => {
    // Each language is independent, ask for them at the same time.
    for (const lang of $langCodesStore) {
      if (!$availabilityStore[lang]) load(lang)
    }
  })

  $: langs = $langCodesStore
  $: loaded = langs.length > 0 && langs.every((lang) => $availabilityStore[lang])
  $: nothingAvailable =
    loaded &&
    langs.every(
      (lang) =>
        $availabilityStore[lang].books.length === 0 &&
        $availabilityStore[lang].language_resources.length === 0
    )

  // If the user previously chose Old Testament resources and no New
  // Testament ones then default to showing the Old Testament.
  const chosenBooks = [...$selectedResourcesStore].map((key) => parseKey(key).book).filter(Boolean)
  let showOldTestament =
    chosenBooks.some((book) => otBooks.includes(book)) &&
    !chosenBooks.some((book) => !otBooks.includes(book))

  let searchTerm = ''
  $: allRows = rowBooks($availabilityStore, langs)
  $: searchedRows = allRows.filter(
    (row) =>
      row.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      row.code.toLowerCase().includes(searchTerm.toLowerCase())
  )
  $: rows = searchedRows.filter((row) =>
    showOldTestament ? row.testament === 'OT' : row.testament === 'NT'
  )

  function selectEverything() {
    applyToSelection(
      langs.flatMap((lang) => (loadedAv(lang) ? everythingCells($availabilityStore[lang]) : [])),
      true
    )
  }

  function selectTestament(testament: 'OT' | 'NT') {
    applyToSelection(
      langs.flatMap((lang) =>
        loadedAv(lang) ? allCells($availabilityStore[lang], testament) : []
      ),
      true
    )
  }

  function loadedAv(lang: string): boolean {
    return Boolean($availabilityStore[lang])
  }

  function clearAll() {
    $selectedResourcesStore = new Set()
  }

  function selectOnlyWhatBothHave() {
    $selectedResourcesStore = commonSelection($availabilityStore, langs)
  }

  function removeUnavailableOnes() {
    $selectedResourcesStore = removeUnavailable($selectedResourcesStore, $availabilityStore, langs)
  }

  function languageWideChange(lang: string, type: string, event: Event) {
    applyToSelection([{ lang, type, book: '' }], (<HTMLInputElement>event.target).checked)
  }

  $: withBooks = languagesWithSelection($selectedResourcesStore)
  $: problems = mismatches($selectedResourcesStore, $availabilityStore, langs)
  $: unavailable = problems.filter((p) => p.reason === 'unavailable')
  $: unchosen = problems.filter((p) => p.reason === 'unselected')

  function describe(problem: { lang: string; type: string; book: string }): string {
    const av = $availabilityStore[problem.lang]
    const bookName = av?.books.find((b) => b.code === problem.book)?.name ?? problem.book
    const typeName = (av && typeInfo(av, problem.type)?.name) ?? problem.type
    return problem.book ? `${bookName} – ${typeName}` : typeName
  }

  function summarize(
    problemsOfLang: Array<{ lang: string; type: string; book: string }>,
    max: number = 6
  ): string {
    const described = problemsOfLang.map(describe)
    return described.length > max
      ? `${described.slice(0, max).join(', ')} and ${described.length - max} more`
      : described.join(', ')
  }

  function nameOfLang(lang: string): string {
    return $langNamesStore[langs.indexOf(lang)] ?? lang
  }

  let showWizardBasketModal = false
  let mobileLang = 0
  $: mobileLangCode = langs[Math.min(mobileLang, langs.length - 1)]

  let windowWidth: number = typeof window !== 'undefined' ? window.innerWidth : 0
  let TAILWIND_SM_MIN_WIDTH: number = PUBLIC_TAILWIND_SM_MIN_WIDTH as unknown as number
</script>

<svelte:window bind:innerWidth={windowWidth} />

<WizardBreadcrumb />

<div class="flex flex-grow flex-row overflow-y-auto overflow-x-hidden">
  <div class="mx-4 mb-6 flex flex-1 flex-col bg-white sm:w-2/3">
    <h3 class="mb-4 text-4xl font-normal leading-[48px] text-[#33445C]">Choose resources</h3>
    <!-- mobile basket modal launcher -->
    <div class="mr-4 text-right sm:hidden">
      <button on:click={() => (showWizardBasketModal = true)}>
        <div class="relative">
          <CheckIcon />
          {#if $langCountStore > 0 || $resourcesCountStore > 0}
            <div
              class="bg-neutral-focus absolute -right-0.5 -top-0.5 h-7 w-7 rounded-full text-center text-xl text-[#33445C]"
              style="background: linear-gradient(180deg, #1876FD 0%, #015AD9 100%);"
            >
              <span class="text-[8px] text-white">{$langCountStore + $resourcesCountStore}</span>
            </div>
          {/if}
        </div>
      </button>
    </div>

    {#if failed.length > 0}
      <div class="bg-white">
        <ErrorAlertIcon />
        <div class="m-auto"><h3 class="text-center text-[#B85659]">Uh Oh...</h3></div>
        <p class="text-xl text-[#B3B9C2]">
          Something went wrong getting the resources available for {failed
            .map(nameOfLang)
            .join(', ')}. Please try again or contact tech support for assistance.
        </p>
      </div>
    {:else if !loaded}
      <ProgressIndicator
        labelString="Acquiring and analyzing resources available for the languages chosen, please be patient as this can take a few minutes"
      />
    {:else if nothingAvailable}
      <p class="text-xl text-[#33445C]" data-testid="nothing-available">
        No resources are available for the language{langs.length > 1 ? 's' : ''} chosen.
      </p>
    {:else}
      <!-- search and testament -->
      <div class="mb-2 flex flex-wrap items-center gap-2">
        <label>
          <input
            id="filter-books"
            type="search"
            bind:value={searchTerm}
            placeholder="Search books"
            class="h-10 rounded-[7px] border border-gray-200 px-3 text-xl text-[#33445c]"
          />
        </label>
        <div class="hidden sm:flex" role="group">
          <button
            class="h-10 whitespace-nowrap rounded-l-md border-2 border-[#015ad9] px-4 text-xl font-medium {showOldTestament
              ? 'bg-[#015ad9] text-white'
              : 'bg-white text-[#33445c]'}"
            on:click={() => (showOldTestament = true)}>Old Testament</button
          >
          <button
            class="h-10 whitespace-nowrap rounded-r-md border-2 border-[#015ad9] px-4 text-xl font-medium {showOldTestament
              ? 'bg-white text-[#33445c]'
              : 'bg-[#015ad9] text-white'}"
            on:click={() => (showOldTestament = false)}>New Testament</button
          >
        </div>
      </div>
      <!-- quick picks -->
      <div class="mb-2 flex flex-wrap items-center gap-2 text-xl">
        <button class="quick-pick" on:click={selectEverything}>Select everything</button>
        <button class="quick-pick" on:click={() => selectTestament('OT')}
          >Select all Old Testament</button
        >
        <button class="quick-pick" on:click={() => selectTestament('NT')}
          >Select all New Testament</button
        >
        <button class="quick-pick" disabled={$resourcesCountStore === 0} on:click={clearAll}
          >Clear all</button
        >
        {#if langs.length === 2}
          <button class="quick-pick" on:click={selectOnlyWhatBothHave}
            >Select only what both languages have</button
          >
        {/if}
      </div>
      {#if langs.length === 2}
        <label class="mb-2 flex items-center text-xl text-[#33445C]">
          <input
            id="sync-languages"
            type="checkbox"
            class="checkbox-target checkbox-style"
            bind:checked={$syncLanguagesStore}
          />
          <span class="pl-2">Keep languages in sync</span>
          <span class="pl-2 text-base text-[#66768B]"
            >(choosing a resource for one language also chooses the equivalent for the other)</span
          >
        </label>
      {/if}
      <!-- language wide resources, e.g., Translation Words -->
      {#each langs as lang}
        {#each $availabilityStore[lang].language_resources as type}
          <label class="mb-2 flex items-center text-xl text-[#33445C]">
            <input
              type="checkbox"
              class="checkbox-target checkbox-style"
              data-testid="language-wide-{lang}-{type}"
              aria-label={langs.length > 1
                ? `${typeInfo($availabilityStore[lang], type)?.name} (${lang})`
                : typeInfo($availabilityStore[lang], type)?.name}
              disabled={!withBooks.has(lang)}
              checked={isSelected($selectedResourcesStore, { lang, type, book: '' })}
              on:change={(event) => languageWideChange(lang, type, event)}
            />
            <span class="pl-2"
              >{typeInfo($availabilityStore[lang], type)?.name}{langs.length > 1
                ? ` (${nameOfLang(lang)})`
                : ''}</span
            >
            <span class="pl-2 text-base text-[#66768B]"
              >all words of the language, for the books chosen</span
            >
          </label>
        {/each}
      {/each}
      <!-- mismatches between the two languages -->
      {#if langs.length === 2 && (unavailable.length > 0 || unchosen.length > 0)}
        <div
          class="mb-2 rounded-lg border border-[#B85659] bg-white p-3 text-base text-[#33445C]"
          data-testid="mismatch-banner"
        >
          {#each langs as lang}
            {@const missing = unavailable.filter((p) => p.lang === lang)}
            {#if missing.length > 0}
              <p>
                ⚠ Not available in {nameOfLang(missing[0].otherLang)}: {summarize(missing)}
              </p>
            {/if}
          {/each}
          {#each langs as lang}
            {@const notChosen = unchosen.filter((p) => p.lang === lang)}
            {#if notChosen.length > 0}
              <p>
                Chosen for {nameOfLang(lang)} but not for {nameOfLang(notChosen[0].otherLang)}: {summarize(
                  notChosen
                )}
              </p>
            {/if}
          {/each}
          {#if unavailable.length > 0}
            <button class="quick-pick mt-1" on:click={removeUnavailableOnes}
              >Remove resources not available in the other language</button
            >
          {/if}
        </div>
      {/if}
      {#if windowWidth < TAILWIND_SM_MIN_WIDTH}
        {#if langs.length > 1}
          <div class="mb-2 flex" role="tablist">
            {#each langs as lang, index}
              <button
                role="tab"
                aria-selected={mobileLang === index}
                class="h-10 flex-1 border-2 border-[#015ad9] text-xl {mobileLang === index
                  ? 'bg-[#015ad9] text-white'
                  : 'bg-white text-[#33445c]'}"
                on:click={() => (mobileLang = index)}>{nameOfLang(lang)}</button
              >
            {/each}
          </div>
        {:else}
          <h3 class="text-2xl text-[#33445C]">{nameOfLang(langs[0])}</h3>
        {/if}
        <ResourceAccordion lang={mobileLangCode} rows={searchedRows} />
      {:else}
        <ResourceMatrix {rows} langNames={$langNamesStore} />
      {/if}
    {/if}
  </div>

  {#if showWizardBasketModal}
    <WizardBasketModal title="Your selections" bind:showWizardBasketModal>
      <svelte:fragment slot="body">
        <WizardBasket />
      </svelte:fragment>
    </WizardBasketModal>
  {/if}
  <div class="hidden sm:flex sm:w-1/3">
    <WizardBasket />
  </div>
</div>

<style lang="postcss">
  .quick-pick {
    @apply rounded-md border border-[#015ad9] bg-white px-3 py-1 text-[#015ad9] hover:bg-[#e6eefb] disabled:cursor-not-allowed disabled:border-[#B3B9C2] disabled:text-[#B3B9C2];
  }
</style>
