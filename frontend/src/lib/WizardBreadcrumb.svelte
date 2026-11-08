<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { resetStores, langRegExp, resourcesRegExp, settingsRegExp } from '$lib/utils'
  import MobileBreadcrumb from '$lib/MobileBreadcrumb.svelte'
  import DesktopBreadcrumb from '$lib/DesktopBreadcrumb.svelte'

  function submitLanguages() {
    resetStores('notifications')
    goto('/resources')
  }

  function submitResources() {
    resetStores('notifications')
    goto('/settings')
  }

  // Turn off and on breadcrumb number circles
  let turnLangStepOn: boolean = false
  let turnResourcesStepOn: boolean = false
  let turnSettingsStepOn: boolean = false
  // Title and label for breadcrumb for mobile (mobile = anything
  // under sm size according to our use of tailwindcss)
  let title: string = 'Languages'
  let stepLabel: string = '1 of 3'
  $: {
    if (langRegExp.test($page.url.pathname)) {
      turnLangStepOn = true
      title = 'Languages'
      stepLabel = '1 of 3'
    } else if (resourcesRegExp.test($page.url.pathname)) {
      turnLangStepOn = true
      turnResourcesStepOn = true
      turnSettingsStepOn = false
      title = 'Resources'
      stepLabel = '2 of 3'
    } else if (settingsRegExp.test($page.url.pathname)) {
      turnLangStepOn = true
      turnResourcesStepOn = true
      turnSettingsStepOn = true
      title = 'Review'
      stepLabel = '3 of 3'
    }
  }
</script>

<!-- wizard breadcrumb -->
<div class="border border-[#E5E8EB] p-4">
  <!-- if isMobile -->
  <MobileBreadcrumb
    {title}
    {stepLabel}
    {turnLangStepOn}
    {turnResourcesStepOn}
    {turnSettingsStepOn}
    {submitLanguages}
    {submitResources}
  />
  <!-- else -->
  <DesktopBreadcrumb
    {turnLangStepOn}
    {turnResourcesStepOn}
    {turnSettingsStepOn}
    {submitLanguages}
    {submitResources}
  />
  <!-- end if -->
</div>

<style lang="postcss">
  * :global(.next-button) {
    background:
      linear-gradient(180deg, #1876fd 0%, #015ad9 100%), linear-gradient(0deg, #33445c, #33445c);
  }
  * :global(.next-button:hover) {
    background:
      linear-gradient(180deg, #0765ec 0%, #0149c8 100%), linear-gradient(0deg, #33445c, #33445c);
  }
</style>
