<script lang="ts">
  import ProgressIndicator from '$lib/ProgressIndicator.svelte'
  import { taskStateStore } from '$lib/stores/TaskStore'
</script>

<div class="flex">
  {#if $taskStateStore === 'Receiving request'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 5%" />
    </div>
  {:else if $taskStateStore === 'Locating assets'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 7%" />
    </div>
  {:else if $taskStateStore === 'Provisioning asset files'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 15%" />
    </div>
  {:else if $taskStateStore === 'Parsing asset files'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 20%" />
    </div>
  {:else if $taskStateStore === 'Assembling content'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 50%" />
    </div>
  {:else if $taskStateStore === 'Provisioning USFM asset files for TW resource'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 55%" />
    </div>
  {:else if $taskStateStore === 'Parsing USFM asset files for TW resource'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 60%" />
    </div>
  {:else if $taskStateStore === 'Limiting TW words'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 65%" />
    </div>
  {:else if $taskStateStore === 'Converting to PDF' || $taskStateStore === 'Converting to Docx'}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 70%" />
    </div>
  {:else}
    <div class="h-1 w-1/2 bg-[#F2F3F5]">
      <div class="h-1 blue-gradient-bar" style="width: 3%" />
    </div>
  {/if}
</div>
{#if $taskStateStore}
  <p id="state" class="text-[#B3B9C2] text-xl">
    <ProgressIndicator labelString={$taskStateStore} />
  </p>
{:else}
  <p id="state" class="text-xl text-[#B3B9C2]">
    <ProgressIndicator labelString="Sending request" />
  </p>
{/if}

<style lang="postcss">
  * :global(.blue-gradient-bar) {
    background: linear-gradient(180deg, #1876fd 0%, #015ad9 100%);
  }
</style>
