<script lang="ts">
  import { PUBLIC_MAX_LANGUAGES } from '$env/static/public'
  import { page, navigating } from '$app/stores'
  import BackButton from '$lib/BackButton.svelte'
  import NextButton from '$lib/NextButton.svelte'
  import { langCountStore } from '$lib/stores/LanguagesStore'
  import { languagesWithResourcesCountStore } from '$lib/stores/ResourcesStore'
  import { langRegExp, resourcesRegExp, settingsRegExp } from '$lib/utils'
  import LeftArrowIcon from '$lib/LeftArrowIcon.svelte'
  import RightArrowIcon from '$lib/RightArrowIcon.svelte'

  export let title: string
  export let stepLabel: string
  export let turnLangStepOn: boolean
  export let turnResourcesStepOn: boolean
  export let turnSettingsStepOn: boolean
  export let submitLanguages: Function
  export let submitResources: Function

  let MAX_LANGUAGES = PUBLIC_MAX_LANGUAGES as unknown as number
</script>

<div
  class="flex items-center justify-between text-xl text-xl
         font-semibold leading-8 text-[#B3B9C2] sm:hidden"
>
  <!-- mobile only page title -->
  <div>
    <h3 class="text-xl text-[#015AD9]">{title}</h3>
    <h4 class="text-xl text-[#33445C]">Step {stepLabel}</h4>
  </div>
  <div class="flex items-center">
    <!-- back button logic -->
    {#if resourcesRegExp.test($page.url.pathname)}
      <BackButton url="/languages" />
    {:else if settingsRegExp.test($page.url.pathname)}
      <BackButton url="/resources" />
    {:else}
      <button
        class="flex cursor-not-allowed items-center rounded-md border
                 border-[#E5E8EB] bg-white px-4 py-2
                 text-xl text-[#33445c]"
        disabled
      >
        <LeftArrowIcon />
      </button>
    {/if}
    <!-- next button logic -->
    {#if langRegExp.test($page.url.pathname) && $langCountStore > 0 && $langCountStore <= MAX_LANGUAGES}
      <NextButton func={submitLanguages} />
    {:else if resourcesRegExp.test($page.url.pathname) && $langCountStore > 0 && $languagesWithResourcesCountStore === $langCountStore}
      <NextButton func={submitResources} />
    {:else}
      <button
        class="ml-2 flex cursor-not-allowed items-center rounded-md
                 border border-[#E5E8EB] bg-white px-4 py-2 text-xl text-[#33445c]"
        disabled
      >
        <RightArrowIcon />
      </button>
    {/if}
  </div>
</div>

{#if turnLangStepOn && !turnResourcesStepOn && !turnSettingsStepOn}
  <div class="w-1/3 border border-[#015ad9] sm:hidden" />
{:else if turnResourcesStepOn && !turnSettingsStepOn}
  <div class="w-2/3 border border-[#015ad9] sm:hidden" />
{:else if turnSettingsStepOn}
  <div class="w-full border border-[#015ad9] sm:hidden" />
{/if}
