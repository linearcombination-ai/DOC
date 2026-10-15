<script lang="ts">
  import { env } from '$env/dynamic/public'
  import DownloadButton from './DownloadButton.svelte'
  import { documentReadyStore, errorStore } from '$lib/stores/NotificationStore'
  import { langCodesStore, langCountStore } from '$lib/stores/LanguagesStore'
  import {
    availabilityStore,
    resourcesCountStore,
    selectedResourcesStore
  } from '$lib/stores/ResourcesStore'
  import { resourceRequests as buildResourceRequests } from '$lib/resources'
  import { limitTwStore } from '$lib/stores/ResourceTypesStore'
  import {
    layoutForPrintStore,
    assemblyStrategyKindStore,
    docTypeStore,
    generatePdfStore,
    generateEpubStore,
    generateDocxStore,
    emailStore,
    documentRequestKeyStore,
    settingsUpdatedStore,
    showTnBookIntroStore,
    showTnChapterIntroStore,
    showBcBookIntroStore,
    showBcChapterCommentaryStore,
    showRgChapterCommentaryStore,
    useChapterLabelsStore,
    useSectionVisualSeparatorStore,
    usePrinceStore,
    useTwoColumnLayoutForTnNotesStore,
    useTwoColumnLayoutForTqNotesStore
  } from '$lib/stores/SettingsStore'
  import { taskStateStore } from '$lib/stores/TaskStore'
  import LogRocket from 'logrocket'
  import TaskStatus from './TaskStatus.svelte'
  import ErrorAlertIcon from '$lib/ErrorAlertIcon.svelte'
  import EyeIcon from '$lib/EyeIcon.svelte'

  let apiRootUrl = env.PUBLIC_BACKEND_API_URL
  let fileServerUrl: string = env.PUBLIC_FILE_SERVER_URL

  async function poll(taskId: string): Promise<string | [string, string]> {
    console.log(`taskId in poll: ${taskId}`)
    let res = await fetch(`${apiRootUrl}/task_status/${taskId}`, {
      method: 'GET'
    })
    let json = await res.json()
    let state = json?.state
    if (state === 'SUCCESS') {
      let result = json?.result
      return [state, result]
    }
    return state
  }

  $: generatingDocument = false

  async function generateDocument() {
    // Update some UI-related state
    generatingDocument = true
    $settingsUpdatedStore = false
    // One resource request for each resource the user chose, in language, book
    // and resource type order.
    let resourceRequests = buildResourceRequests(
      $selectedResourcesStore,
      $langCodesStore,
      $availabilityStore
    )
    // Create the JSON structure to POST.
    let documentRequest = {
      email_address: $emailStore,
      assembly_strategy_kind: $assemblyStrategyKindStore,
      layout_for_print: $layoutForPrintStore,
      generate_pdf: $generatePdfStore,
      generate_epub: $generateEpubStore,
      generate_docx: $generateDocxStore,
      resource_requests: resourceRequests,
      document_request_source: 'ui',
      limit_words: $limitTwStore,
      use_chapter_labels: $useChapterLabelsStore,
      use_section_visual_separator: $useSectionVisualSeparatorStore,
      use_prince: $usePrinceStore,
      use_two_column_layout_for_tn_notes: $useTwoColumnLayoutForTnNotesStore,
      use_two_column_layout_for_tq_notes: $useTwoColumnLayoutForTqNotesStore,
      show_tn_book_intro: $showTnBookIntroStore,
      show_tn_chapter_intro: $showTnChapterIntroStore,
      show_bc_book_intro: $showBcBookIntroStore,
      show_bc_chapter_commentary: $showBcChapterCommentaryStore,
      show_rg_chapter_commentary: $showRgChapterCommentaryStore
    }
    console.log('document request: ', JSON.stringify(documentRequest, null, 2))
    $errorStore = null
    $documentReadyStore = false
    $documentRequestKeyStore = ''
    let endpointUrl = `${apiRootUrl}/documents`
    if ($generateDocxStore) {
      endpointUrl = `${apiRootUrl}/documents_docx`
    }
    const response = await fetch(endpointUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(documentRequest)
    })
    const data = await response.json()
    if (!response.ok) {
      console.error(`data.detail: ${data.detail}`)
      $errorStore = data.detail
    } else {
      console.log(`data: ${JSON.stringify(data)}`)
      const timerIntervalId = setInterval(async function () {
        // Poll the server for the task state and result
        let results = await poll(data.task_id)
        console.log(`results: ${results}`)
        $taskStateStore = Array.isArray(results) ? results[0] : results
        console.log(`$taskStateStore: ${$taskStateStore}`)
        if ($taskStateStore === 'SUCCESS' && Array.isArray(results) && results[1]) {
          let finishedDocumentRequestKey = results[1]
          console.log(`finishedDocumentReuestKey: ${finishedDocumentRequestKey}`)
          // Update some UI-related state
          $documentReadyStore = true
          $documentRequestKeyStore = finishedDocumentRequestKey
          $errorStore = null
          $taskStateStore = ''
          generatingDocument = false
          clearInterval(timerIntervalId)
        } else if ($taskStateStore === 'FAILURE') {
          console.log("We're sorry, an internal error occurred which we'll investigate.")
          // Update some UI-related state
          $errorStore =
            "We're sorry. An error occurred. The document you requested may not yet be supported or we may have experienced an internal problem which we'll investigate. Please try another document request."
          $taskStateStore = ''
          generatingDocument = false
          clearInterval(timerIntervalId)
        }
      }, 5000)
    }
  }

  // Reactively set/store email (for this session) and identify session to
  // LogRocket
  $: {
    // Deal with empty string case
    if ($emailStore && $emailStore === '') {
      $emailStore = null
      LogRocket.identify($documentRequestKeyStore)
      // Deal with undefined case
    } else if ($emailStore === undefined) {
      $emailStore = null
      LogRocket.identify($documentRequestKeyStore)
      // Deal with non-empty string
    } else if ($emailStore && $emailStore !== '') {
      $emailStore = $emailStore.trim()
      // The LogRocket init call has been moved to App.svelte to be earlier in
      // the loading process so that hopefully more of the session
      // is recorded.
      // Send email to LogRocket to identify session.
      LogRocket.identify($emailStore)
    }
  }

  // Reactively set/store document output type flags
  $: {
    if ($docTypeStore === 'pdf') {
      $generatePdfStore = true
      $generateEpubStore = false
      $generateDocxStore = false
    } else if ($docTypeStore === 'epub') {
      $generatePdfStore = false
      $generateEpubStore = true
      $generateDocxStore = false
    } else if ($docTypeStore === 'docx') {
      $generatePdfStore = false
      $generateEpubStore = false
      $generateDocxStore = true
    }
  }

  // Reactively set download URLs of generated documents
  let pdfDownloadUrl: string
  $: pdfDownloadUrl = `${fileServerUrl}/${$documentRequestKeyStore}.pdf`
  let ePubDownloadUrl: string
  $: ePubDownloadUrl = `${fileServerUrl}/${$documentRequestKeyStore}.epub`
  let docxDownloadUrl: string
  $: docxDownloadUrl = `${fileServerUrl}/${$documentRequestKeyStore}.docx`
  let htmlDownloadUrl: string
  $: htmlDownloadUrl = `${fileServerUrl}/${$documentRequestKeyStore}.html`

  function viewFromUrl(url: string) {
    console.log(`url: ${url}`)
    window.open(url, '_blank')
  }

  // Warn user when they attempt to reload page or close tab
  window.addEventListener('beforeunload', (event) => {
    if (generatingDocument) {
      event.returnValue = `Are you sure you want to leave while your document is being generated?`
    }
  })
</script>

<div class="bg-white pb-4 pt-12">
  {#if $errorStore}
    <div class="bg-white">
      <ErrorAlertIcon />
      <div class="m-auto"><h3 class="text-center text-[#B85659]">Uh Oh...</h3></div>
      <div class="m-auto">
        <p class="text-xl text-[#B3B9C2]">
          Something went wrong. Please review your selections or contact tech support for
          assistance.
        </p>
      </div>
    </div>
  {:else if (!generatingDocument && !$documentReadyStore) || $settingsUpdatedStore}
    {#if ($langCountStore > 0 || $langCountStore <= 2) && $assemblyStrategyKindStore && $resourcesCountStore > 0}
      <div class="pb-4">
        <button
          class="blue-gradient w-1/2 rounded-md p-4 text-center"
          on:click={() => generateDocument()}
        >
          <span class="text-xl text-white">Generate File</span>
        </button>
      </div>
    {:else}
      <div class="pb-4">
        <button class="gray-gradiant btn-disabled w-1/2 rounded-md p-4 text-center">
          <span class="text-xl text-[#b3b9c2]" style="color: #140e0866">Generate File</span>
        </button>
      </div>
    {/if}
  {:else}
    {#if !$documentReadyStore}
      <TaskStatus />
    {/if}
    {#if $documentReadyStore}
      <div class="bg-white">
        <div class="h-1 w-1/2 bg-[#F2F3F5]">
          <div class="blue-gradient-bar h-1" style="width: 100%" />
        </div>
        <div class="m-auto"><h3 class="text-xl text-[#82A93F]">Complete!</h3></div>
        {#if $generatePdfStore}
          <div class="m-auto mt-4">
            <DownloadButton buttonText="View PDF" linkText="Download PDF" url={pdfDownloadUrl} />
          </div>
        {/if}
        {#if $generateEpubStore}
          <div class="m-auto mt-4">
            <DownloadButton buttonText="View ePub" linkText="Download ePub" url={ePubDownloadUrl} />
          </div>
        {/if}
        {#if $generateDocxStore}
          <div class="m-auto mt-4">
            <DownloadButton
              buttonText="Download Docx"
              linkText="Download Docx"
              url={docxDownloadUrl}
            />
          </div>
          <div class="mt-4 text-[#33445C]">
            <p>
              Any missing fonts on your computer may be downloaded here:
              <span style="text-decoration-line: underline;">
                <a
                  href="https://github.com/Bible-Translation-Tools/ScriptureAppBuilder-pipeline/tree/base/ContainerImage/home/fonts"
                  target="_blank">fonts</a
                >
              </span>
            </p>
          </div>
          <div class="mt-4 text-xl text-[#33445C]">
            <p>
              Once you have downloaded and installed any missing fonts, select the document text
              which looks like little empty boxes (indicating a missing font), and change the font
              for that highlighted text to the appropriate installed font in Word, then save the
              Word document.
            </p>
          </div>
        {/if}
        {#if !$generateDocxStore}
          <div class="mt-4 pb-4">
            <button
              class="gray-gradient hover:gray-gradient-hover w-1/2 rounded-md border-2 border-[#e5e8eb] p-4 text-center"
              on:click={() => viewFromUrl(htmlDownloadUrl)}
            >
              <EyeIcon />
              <span class="p-4 text-xl">View HTML Online</span>
            </button>
          </div>
        {/if}
      </div>
    {:else}
      <button
        class="mb-4 mt-2 w-1/2 rounded-md
                    border border-[#E5E8EB] bg-[#F2F3F5] p-4
                    text-center text-xl text-[#B3B9C2] hover:bg-[#efefef]"
        disabled
      >
        View
      </button>
      <a
        role="link"
        aria-disabled="true"
        tabindex="-1"
        class="pointer-events-none mt-2 block
           cursor-not-allowed text-sm text-[#B3B9C2]
           underline"
      >
        Download
        <span class="text-[#B3B9C2]"> (right-click → Save link as…) </span>
      </a>
      <p class="mt-4 text-xl italic text-[#B3B9C2]">
        We appreciate your patience as this can take several minutes for larger documents.
      </p>
    {/if}
  {/if}
</div>

<style lang="postcss">
  * :global(.gray-gradiant) {
    background:
      linear-gradient(0deg, rgba(20, 14, 8, 0.05), rgba(20, 14, 8, 0.05)),
      linear-gradient(0deg, rgba(20, 14, 8, 0), rgba(20, 14, 8, 0));
  }
  * :global(.gray-gradient:hover) {
    background:
      linear-gradient(0deg, rgba(20, 14, 8, 0.3), rgba(20, 14, 8, 0.3)),
      linear-gradient(0deg, rgba(20, 14, 8, 0.05), rgba(20, 14, 8, 0.05));
  }
  * :global(.blue-gradient) {
    background:
      linear-gradient(180deg, #1876fd 0%, #015ad9 100%), linear-gradient(0deg, #33445c, #33445c);
  }
  * :global(.blue-gradient-bar) {
    background: linear-gradient(180deg, #1876fd 0%, #015ad9 100%);
  }
</style>
