<script lang="ts">
  import LanguageBasket from './LanguageBasket.svelte'
  import ResourcesBasket from './ResourcesBasket.svelte'
</script>

<div class="w-full flex-shrink-0 overflow-y-auto bg-[#f2f3f5] p-4">
  <h1 class="hidden py-4 pl-0 text-xl font-semibold text-[#33445C] sm:block">Your Selections</h1>
  <LanguageBasket />
  <ResourcesBasket />
</div>
