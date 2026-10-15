<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { resourcesRegExp } from '$lib/utils'
  import { langCodesStore, langNamesStore } from '$lib/stores/LanguagesStore'
  import {
    availabilityStore,
    selectedResourcesStore,
    resourcesCountStore
  } from '$lib/stores/ResourcesStore'
  import { groupedByType, parseKey, setCells } from '$lib/resources'
  import FileIcon from './FileIcon.svelte'
  import EditIcon from './EditIcon.svelte'
  import CloseIcon from './CloseIcon.svelte'

  $: editable = resourcesRegExp.test($page.url.pathname)
  $: langs = $langCodesStore
  $: groups = langs.map((lang) => ({
    lang,
    name: $langNamesStore[langs.indexOf(lang)] ?? lang,
    types: groupedByType($selectedResourcesStore, lang, $availabilityStore[lang])
  }))

  function bookName(lang: string, code: string): string {
    return $availabilityStore[lang]?.books.find((b) => b.code === code)?.name ?? code
  }

  function removeType(lang: string, type: string) {
    const cells = [...$selectedResourcesStore]
      .map(parseKey)
      .filter((cell) => cell.lang === lang && cell.type === type)
    $selectedResourcesStore = setCells($selectedResourcesStore, cells, false)
  }
</script>

{#if !editable && $resourcesCountStore > 0}
  <div class="my-2 flex items-center justify-between">
    <div class="flex items-center justify-between">
      <FileIcon />
      <h2 class="ml-2 text-xl font-semibold text-[#33445C]">Resources</h2>
    </div>
    <button
      class="flex rounded bg-white px-4 py-2 text-xl text-[#33445c] hover:bg-[#efefef]"
      on:click={() => goto('/resources')}
    >
      <EditIcon />
      <span class="ml-2"> Edit </span>
    </button>
  </div>
{:else}
  <div class="my-2 flex items-center">
    <FileIcon />
    <h2 class="ml-2 text-xl font-semibold text-[#33445C]">Resources</h2>
  </div>
{/if}
{#if $resourcesCountStore > 0}
  {#each groups as group (group.lang)}
    {#if group.types.length > 0}
      {#if langs.length > 1}
        <h3 class="mt-2 text-xl font-semibold text-[#33445C]">{group.name}</h3>
      {/if}
      {#each group.types as type (type.type)}
        <div
          class="mt-2 w-full rounded-lg bg-white p-4 text-xl text-[#66768B]"
          data-testid="basket-{group.lang}-{type.type}"
        >
          <div class="flex items-center justify-between">
            <span>
              {type.name} ({type.type}){#if type.books.length > 0}<span class="ml-2"
                  >{type.books.length} {type.books.length === 1 ? 'book' : 'books'}</span
                >{/if}
            </span>
            {#if editable}
              <button
                aria-label="Remove {type.name} ({group.lang})"
                on:click={() => removeType(group.lang, type.type)}
              >
                <CloseIcon />
              </button>
            {/if}
          </div>
          {#if type.books.length > 0}
            <details class="text-base">
              <summary class="cursor-pointer">
                {type.books
                  .slice(0, 3)
                  .map((code) => bookName(group.lang, code))
                  .join(', ')}{type.books.length > 3 ? ` and ${type.books.length - 3} more` : ''}
              </summary>
              <ul class="ml-4 list-disc">
                {#each type.books as code}
                  <li>{bookName(group.lang, code)} ({code})</li>
                {/each}
              </ul>
            </details>
          {/if}
        </div>
      {/each}
    {/if}
  {/each}
{:else}
  <div class="rounded-lg bg-[#e5e8eb] p-6 text-[#66768b]">
    Selections will appear here once a resource is selected
  </div>
{/if}
