// End to end tests of the single "Choose resources" wizard step that replaced
// the separate books and resource types steps.
//
// These tests stub the backend's language list, /resource_availability/<lang>
// and POST /documents responses (via page.route) so that they are fast,
// deterministic and independent of what content happens to be in DCS. The
// frontend dev server (localhost:8001) must still be running, but the
// backend, PrinceXML, etc. need not be. Document generation itself is
// exercised by doc_test_1..7 against the real backend.
import { expect, test } from '@playwright/test'
import type { Page, Request } from '@playwright/test'

const FRONTEND = 'http://localhost:8001'
const CORS = { 'access-control-allow-origin': '*' }

const LANGUAGES = [
  ['en', 'English', true],
  ['es-419', 'Español', true],
  // The languages page only lists languages when it has some of both kinds
  ['hrt', 'Heartish', false]
]

// English: scripture (ulb), notes (tn), language wide words (tw).
const EN = {
  lang_code: 'en',
  resource_types: [
    { code: 'ulb', name: 'Unlocked Literal Bible', category: 'scripture', scope: 'book' },
    { code: 'tn', name: 'Translation Notes', category: 'notes', scope: 'book' },
    { code: 'tw', name: 'Translation Words', category: 'words', scope: 'language' }
  ],
  books: [
    { code: 'gen', name: 'Genesis', testament: 'OT', resources: ['ulb'] },
    { code: 'mat', name: 'Matthew', testament: 'NT', resources: ['ulb', 'tn'] },
    { code: 'mrk', name: 'Mark', testament: 'NT', resources: ['ulb'] }
  ],
  language_resources: ['tw']
}

// Spanish: scripture (reg) and questions (tq) but no notes and no words, so
// some of what English has is not available in Spanish and vice versa.
const ES = {
  lang_code: 'es-419',
  resource_types: [
    { code: 'reg', name: 'Regular', category: 'scripture', scope: 'book' },
    { code: 'tq', name: 'Translation Questions', category: 'questions', scope: 'book' }
  ],
  books: [
    { code: 'gen', name: 'Génesis', testament: 'OT', resources: ['reg'] },
    { code: 'mat', name: 'Mateo', testament: 'NT', resources: ['reg', 'tq'] },
    { code: 'mrk', name: 'Marcos', testament: 'NT', resources: ['reg'] }
  ],
  language_resources: []
}

const AVAILABILITY: Record<string, unknown> = { en: EN, 'es-419': ES }

async function stubBackend(
  page: Page,
  availability: Record<string, unknown> = AVAILABILITY,
  languages: Array<Array<unknown>> = LANGUAGES
) {
  await page.route('**/language_codes_and_names', (route) =>
    route.fulfill({ json: languages, headers: CORS })
  )
  await page.route('**/resource_availability/*', (route) => {
    const lang = decodeURIComponent(new URL(route.request().url()).pathname.split('/').pop() ?? '')
    const body = availability[lang]
    return body
      ? route.fulfill({ json: body, headers: CORS })
      : route.fulfill({ status: 404, json: { detail: 'not found' }, headers: CORS })
  })
}

// Record the document request the UI POSTs and answer with an error so that
// nothing is generated.
async function captureDocumentRequest(page: Page): Promise<() => Promise<Request>> {
  let resolve: (request: Request) => void
  const posted = new Promise<Request>((res) => (resolve = res))
  await page.route('**/documents*', async (route) => {
    if (route.request().method() === 'POST') {
      resolve(route.request())
      await route.fulfill({ status: 400, json: { detail: 'stubbed' }, headers: CORS })
    } else {
      await route.continue()
    }
  })
  return () => posted
}

async function chooseLanguagesAndGoToResources(page: Page, ...names: Array<string>) {
  await page.goto(FRONTEND)
  for (const name of names) {
    await page.getByText(name, { exact: false }).first().click()
  }
  // The desktop and mobile Next buttons are both in the page, only one is shown
  await page.locator('.next-button:visible').click()
  await expect(page.getByRole('heading', { name: 'Choose resources' })).toBeVisible()
}

test.describe('choose resources, one language, desktop', () => {
  test.beforeEach(async ({ page }) => {
    await stubBackend(page)
  })

  test('languages step leads straight to a single resources step', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    await expect(page).toHaveURL(/\/resources$/)
    // There is a resources step and no separate books/resource types steps
    const breadcrumb = page.locator('div.border-\\[\\#E5E8EB\\]').first()
    await expect(breadcrumb).toContainText('Languages')
    await expect(breadcrumb).toContainText('Resources')
    await expect(breadcrumb).toContainText('Review')
    await expect(breadcrumb).not.toContainText('Books')
    await expect(breadcrumb).not.toContainText('Resource Types')
    await expect(page.getByTestId('resource-matrix')).toBeVisible()
    // Nothing chosen yet, so there is nowhere to go
    await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled()
    await expect(page.locator('body')).toContainText(
      'Selections will appear here once a resource is selected'
    )
  })

  test('old routes for the removed steps no longer exist', async ({ page }) => {
    for (const path of ['/books', '/resource_types']) {
      const response = await page.goto(`${FRONTEND}${path}`)
      expect(response?.status()).toBe(404)
    }
  })

  test('only resources that exist for a book are offered', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    // New Testament is shown by default
    await expect(page.getByTestId('row-mat')).toBeVisible()
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeVisible()
    await expect(page.getByTestId('cell-en-tn-mat')).toBeVisible()
    await expect(page.getByTestId('cell-en-ulb-mrk')).toBeVisible()
    // Mark has no notes: a dash, not a checkbox
    await expect(page.getByTestId('cell-en-tn-mrk')).toHaveCount(0)
    await expect(page.getByTestId('row-mrk')).toContainText('–')
    // Old Testament books are not shown until asked for
    await expect(page.getByTestId('row-gen')).toHaveCount(0)
    await page.getByRole('button', { name: 'Old Testament', exact: true }).click()
    await expect(page.getByTestId('row-gen')).toBeVisible()
    await expect(page.getByTestId('row-mat')).toHaveCount(0)
    await expect(page.getByTestId('cell-en-ulb-gen')).toBeVisible()
    await expect(page.getByTestId('cell-en-tn-gen')).toHaveCount(0)
    // Legend explains the column codes
    await expect(page.getByTestId('resource-type-legend')).toContainText('tn = Translation Notes')
    // Translation Words is language wide so it is not a column
    await expect(page.getByTestId('column-en-tw')).toHaveCount(0)
  })

  test('selecting and deselecting resources updates the basket and Next', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
    await expect(page.getByTestId('basket-en-ulb')).toContainText('Matthew')
    await expect(page.getByRole('button', { name: 'Next' })).toBeEnabled()
    await page.getByTestId('cell-en-tn-mat').check()
    await page.getByTestId('cell-en-ulb-mrk').check()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('2 books')
    await expect(page.getByTestId('basket-en-tn')).toContainText('1 book')
    await page.getByTestId('cell-en-ulb-mrk').uncheck()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
    await page.getByTestId('cell-en-ulb-mat').uncheck()
    await expect(page.getByTestId('basket-en-ulb')).toHaveCount(0)
    await expect(page.getByTestId('basket-en-tn')).toBeVisible()
    await page.getByTestId('cell-en-tn-mat').uncheck()
    await expect(page.locator('body')).toContainText(
      'Selections will appear here once a resource is selected'
    )
    await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled()
  })

  test('row and column checkboxes choose groups and show partial state', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    // A row chooses every resource the book has
    await page.getByTestId('row-en-mat').check()
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
    await expect(page.getByTestId('cell-en-tn-mat')).toBeChecked()
    await expect(page.getByTestId('cell-en-ulb-mrk')).not.toBeChecked()
    // The ulb column is now partially chosen
    expect(
      await page.getByTestId('column-en-ulb').evaluate((el: HTMLInputElement) => el.indeterminate)
    ).toBe(true)
    await page.getByTestId('column-en-ulb').check()
    await expect(page.getByTestId('cell-en-ulb-mrk')).toBeChecked()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('2 books')
    // Unchecking the column removes the resource type for the books shown
    await page.getByTestId('column-en-ulb').uncheck()
    await expect(page.getByTestId('cell-en-ulb-mat')).not.toBeChecked()
    await expect(page.getByTestId('cell-en-tn-mat')).toBeChecked()
  })

  test('quick picks: select everything, testaments, clear all', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    const clearAll = page.getByRole('button', { name: 'Clear all' })
    await expect(clearAll).toBeDisabled()
    await page.getByRole('button', { name: 'Select all Old Testament' }).click()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('Genesis')
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
    await expect(clearAll).toBeEnabled()
    await page.getByRole('button', { name: 'Select all New Testament' }).click()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('3 books')
    await expect(page.getByTestId('basket-en-tn')).toContainText('1 book')
    await clearAll.click()
    await expect(page.getByTestId('basket-en-ulb')).toHaveCount(0)
    await page.getByRole('button', { name: 'Select everything' }).click()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('3 books')
    // Everything includes the language wide resource
    await expect(page.getByTestId('basket-en-tw')).toBeVisible()
  })

  test('search narrows the books shown', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    await page.getByPlaceholder('Search books').fill('mar')
    await expect(page.getByTestId('row-mrk')).toBeVisible()
    await expect(page.getByTestId('row-mat')).toHaveCount(0)
    await page.getByPlaceholder('Search books').fill('')
    await expect(page.getByTestId('row-mat')).toBeVisible()
  })

  test('language wide resource needs a book first', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    const tw = page.getByTestId('language-wide-en-tw')
    await expect(tw).toBeDisabled()
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(tw).toBeEnabled()
    await tw.check()
    await expect(page.getByTestId('basket-en-tw')).toBeVisible()
    await tw.uncheck()
    await expect(page.getByTestId('basket-en-tw')).toHaveCount(0)
  })

  test('removing a resource type from the basket unchooses its books', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    await page.getByTestId('cell-en-ulb-mat').check()
    await page.getByTestId('cell-en-ulb-mrk').check()
    await page.getByTestId('cell-en-tn-mat').check()
    await page.getByRole('button', { name: 'Remove Unlocked Literal Bible (en)' }).click()
    await expect(page.getByTestId('basket-en-ulb')).toHaveCount(0)
    await expect(page.getByTestId('cell-en-ulb-mat')).not.toBeChecked()
    await expect(page.getByTestId('cell-en-ulb-mrk')).not.toBeChecked()
    await expect(page.getByTestId('cell-en-tn-mat')).toBeChecked()
  })

  test('breadcrumb and back/next navigation retain what was chosen', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    await page.getByTestId('cell-en-ulb-mat').check()
    // Forward to review and back with the Back button
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(page).toHaveURL(/\/settings$/)
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
    await page.getByRole('button', { name: 'Back' }).click()
    await expect(page).toHaveURL(/\/resources$/)
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
    // Back to languages with the Back button and forward again
    await page.getByRole('button', { name: 'Back' }).click()
    await expect(page).toHaveURL(/\/languages$/)
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
    // The Languages breadcrumb link
    await page.getByRole('link', { name: 'Languages' }).click()
    await expect(page).toHaveURL(/\/languages$/)
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
  })

  test('review step: Edit returns to resources and the POSTed request matches the choices', async ({
    page
  }) => {
    const documentRequest = await captureDocumentRequest(page)
    await chooseLanguagesAndGoToResources(page, 'English')
    // Chosen out of canonical order on purpose
    await page.getByTestId('cell-en-ulb-mrk').check()
    await page.getByTestId('cell-en-tn-mat').check()
    await page.getByTestId('cell-en-ulb-mat').check()
    await page.getByTestId('language-wide-en-tw').check()
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(page).toHaveURL(/\/settings$/)
    // Edit goes back to choosing
    await page.getByRole('button', { name: 'Edit' }).last().click()
    await expect(page).toHaveURL(/\/resources$/)
    await expect(page.getByTestId('cell-en-ulb-mrk')).toBeChecked()
    await page.getByRole('button', { name: 'Next' }).click()
    await page.getByRole('radio', { name: 'PDF' }).click()
    await page.getByRole('button', { name: 'Generate File' }).click()
    const body = (await documentRequest()).postDataJSON()
    // Canonical book order, then the language's resource type order, and
    // the language wide resource is requested for each book chosen.
    expect(body.resource_requests).toEqual([
      { lang_code: 'en', resource_type: 'ulb', book_code: 'mat' },
      { lang_code: 'en', resource_type: 'tn', book_code: 'mat' },
      { lang_code: 'en', resource_type: 'tw', book_code: 'mat' },
      { lang_code: 'en', resource_type: 'ulb', book_code: 'mrk' },
      { lang_code: 'en', resource_type: 'tw', book_code: 'mrk' }
    ])
    expect(body.document_request_source).toBe('ui')
  })

  test('nothing available for the language', async ({ page }) => {
    await page.unroute('**/resource_availability/*')
    await page.route('**/resource_availability/*', (route) =>
      route.fulfill({
        json: { lang_code: 'en', resource_types: [], books: [], language_resources: [] },
        headers: CORS
      })
    )
    await chooseLanguagesAndGoToResources(page, 'English')
    await expect(page.getByTestId('nothing-available')).toContainText(
      'No resources are available for the language chosen'
    )
    await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled()
  })

  test('failure getting availability is reported', async ({ page }) => {
    await page.unroute('**/resource_availability/*')
    await page.route('**/resource_availability/*', (route) =>
      route.fulfill({ status: 500, json: { detail: 'boom' }, headers: CORS })
    )
    await chooseLanguagesAndGoToResources(page, 'English')
    await expect(page.locator('main, body').first()).toContainText(
      'Something went wrong getting the resources available for English'
    )
    await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled()
  })
})

test.describe('choose resources, two languages, desktop', () => {
  test.beforeEach(async ({ page }) => {
    await stubBackend(page)
  })

  test('matrix is grouped by language and choices are kept in sync by default', async ({
    page
  }) => {
    await chooseLanguagesAndGoToResources(page, 'English', 'Español')
    await expect(page.getByTestId('resource-matrix')).toContainText('English')
    await expect(page.getByTestId('resource-matrix')).toContainText('Español')
    await expect(page.getByTestId('resource-type-legend')).toContainText(
      'ulb = Unlocked Literal Bible'
    )
    await expect(page.getByTestId('resource-type-legend')).toContainText('reg = Regular')
    // Book names come from both languages' data, shown once per row
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeVisible()
    await expect(page.getByTestId('cell-es-419-reg-mat')).toBeVisible()
    await expect(page.getByRole('checkbox', { name: 'Keep languages in sync' })).toBeChecked()
    // ulb stands in for reg: they are both scripture
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(page.getByTestId('cell-es-419-reg-mat')).toBeChecked()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
    await expect(page.getByTestId('basket-es-419-reg')).toContainText('1 book')
    await expect(page.getByTestId('mismatch-banner')).toHaveCount(0)
    await page.getByTestId('cell-es-419-reg-mat').uncheck()
    await expect(page.getByTestId('cell-en-ulb-mat')).not.toBeChecked()
  })

  test('a resource the other language lacks is flagged and can be removed', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English', 'Español')
    await page.getByTestId('cell-en-ulb-mat').check()
    // Spanish has no notes for Matthew
    await page.getByTestId('cell-en-tn-mat').check()
    await expect(page.getByTestId('cell-es-419-tn-mat')).toHaveCount(0)
    await expect(page.getByTestId('mismatch-en-tn-mat')).toBeVisible()
    await expect(page.getByTestId('mismatch-banner')).toContainText('Not available in Español')
    await expect(page.getByTestId('mismatch-banner')).toContainText('Translation Notes')
    await page
      .getByRole('button', { name: 'Remove resources not available in the other language' })
      .click()
    await expect(page.getByTestId('cell-en-tn-mat')).not.toBeChecked()
    await expect(page.getByTestId('mismatch-banner')).toHaveCount(0)
    // What both have is untouched
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
    await expect(page.getByTestId('cell-es-419-reg-mat')).toBeChecked()
  })

  test('without sync, a choice for one language is flagged as not chosen for the other', async ({
    page
  }) => {
    await chooseLanguagesAndGoToResources(page, 'English', 'Español')
    await page.getByRole('checkbox', { name: 'Keep languages in sync' }).uncheck()
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(page.getByTestId('cell-es-419-reg-mat')).not.toBeChecked()
    await expect(page.getByTestId('mismatch-banner')).toContainText(
      'Chosen for English but not for Español'
    )
    // Next waits until each language has something chosen
    await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled()
    await page.getByTestId('cell-es-419-reg-mat').check()
    await expect(page.getByTestId('mismatch-banner')).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Next' })).toBeEnabled()
  })

  test('select only what both languages have', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English', 'Español')
    await page.getByRole('button', { name: 'Select only what both languages have' }).click()
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
    await expect(page.getByTestId('cell-es-419-reg-mat')).toBeChecked()
    await expect(page.getByTestId('cell-en-ulb-mrk')).toBeChecked()
    await expect(page.getByTestId('cell-es-419-reg-mrk')).toBeChecked()
    // Notes only exist in English and questions only in Spanish
    await expect(page.getByTestId('cell-en-tn-mat')).not.toBeChecked()
    await expect(page.getByTestId('cell-es-419-tq-mat')).not.toBeChecked()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('3 books')
    await expect(page.getByTestId('basket-es-419-reg')).toContainText('3 books')
    await expect(page.getByTestId('mismatch-banner')).toHaveCount(0)
  })

  test('both languages reach the document request, in the order chosen', async ({ page }) => {
    const documentRequest = await captureDocumentRequest(page)
    await chooseLanguagesAndGoToResources(page, 'Español', 'English')
    await page.getByTestId('cell-en-ulb-mat').check()
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(page).toHaveURL(/\/settings$/)
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
    await expect(page.getByTestId('basket-es-419-reg')).toContainText('1 book')
    await page.getByRole('radio', { name: 'PDF' }).click()
    await page.getByRole('button', { name: 'Generate File' }).click()
    const body = (await documentRequest()).postDataJSON()
    expect(body.resource_requests).toEqual([
      { lang_code: 'es-419', resource_type: 'reg', book_code: 'mat' },
      { lang_code: 'en', resource_type: 'ulb', book_code: 'mat' }
    ])
  })

  test('dropping a language on return drops its choices', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English', 'Español')
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(page.getByTestId('basket-es-419-reg')).toBeVisible()
    await page.getByRole('link', { name: 'Languages' }).click()
    await page.getByRole('checkbox', { name: /Español/ }).uncheck()
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeChecked()
    await expect(page.getByTestId('cell-es-419-reg-mat')).toHaveCount(0)
    await expect(page.getByTestId('basket-es-419-reg')).toHaveCount(0)
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
  })
})

test.describe('choose resources, mobile', () => {
  test.use({ viewport: { width: 390, height: 800 } })

  test.beforeEach(async ({ page }) => {
    await stubBackend(page)
  })

  test('accordion replaces the matrix and chooses books', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    // Mobile breadcrumb
    await expect(page.getByRole('heading', { name: 'Resources', exact: true })).toBeVisible()
    await expect(page.getByText('Step 2 of 3')).toBeVisible()
    await expect(page.getByTestId('resource-matrix')).toHaveCount(0)
    const ulb = page.getByTestId('type-en-ulb')
    const tn = page.getByTestId('type-en-tn')
    await expect(ulb).toContainText('Unlocked Literal Bible')
    // The accordion lists every book with the resource, both testaments
    await expect(ulb).toContainText('0 of 3')
    await expect(tn).toContainText('0 of 1')
    // Collapsed until opened
    await expect(page.getByTestId('cell-en-ulb-mat')).toBeHidden()
    await ulb.locator('summary').click()
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(ulb).toContainText('1 of 3')
    await expect(page.getByTestId('basket-en-ulb')).toBeHidden() // the basket is in a modal
    await expect(page.locator('.next-button:visible')).toBeEnabled()
    // Testament shortcuts within the card
    await ulb.getByRole('button', { name: 'Old Testament', exact: true }).click()
    await expect(ulb).toContainText('2 of 3')
    await ulb.getByRole('button', { name: 'Clear', exact: true }).click()
    await expect(ulb).toContainText('0 of 3')
    await ulb.getByRole('button', { name: 'New Testament', exact: true }).click()
    await expect(ulb).toContainText('2 of 3')
    await expect(page.getByTestId('cell-en-ulb-gen')).not.toBeChecked()
    await ulb.getByRole('button', { name: 'Select all', exact: true }).click()
    await expect(page.getByTestId('cell-en-ulb-gen')).toBeChecked()
    await expect(ulb).toContainText('3 of 3')
  })

  test('mobile Next and Back move through the steps', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English')
    await page.getByTestId('type-en-ulb').locator('summary').click()
    await page.getByTestId('cell-en-ulb-mat').check()
    await page.locator('.next-button:visible').click()
    await expect(page).toHaveURL(/\/settings$/)
    await expect(page.getByText('Step 3 of 3')).toBeVisible()
    await page.goBack()
    await expect(page).toHaveURL(/\/resources$/)
  })

  test('two languages are shown one at a time with tabs', async ({ page }) => {
    await chooseLanguagesAndGoToResources(page, 'English', 'Español')
    await expect(page.getByRole('tab', { name: 'English' })).toHaveAttribute(
      'aria-selected',
      'true'
    )
    await expect(page.getByTestId('type-en-ulb')).toBeVisible()
    await expect(page.getByTestId('type-es-419-reg')).toHaveCount(0)
    await page.getByTestId('type-en-ulb').locator('summary').click()
    await page.getByTestId('cell-en-ulb-mat').check()
    await page.getByRole('tab', { name: 'Español' }).click()
    await expect(page.getByTestId('type-en-ulb')).toHaveCount(0)
    // Kept in sync, so Spanish has already been chosen for Matthew
    await expect(page.getByTestId('type-es-419-reg')).toContainText('1 of 3')
    await expect(page.getByTestId('type-es-419-tq')).toContainText('0 of 1')
  })
})

test.describe('transfer from BIEL', () => {
  test.beforeEach(async ({ page }) => {
    await stubBackend(page)
  })

  test('all books of a resource go straight to review', async ({ page }) => {
    await page.goto(
      `${FRONTEND}/transfer/repo_url=https:%2F%2Fcontent.bibletranslationtools.org%2FWycliffeAssociates%2Fen_ulb`
    )
    await expect(page).toHaveURL(/\/settings$/)
    await expect(page.getByTestId('basket-en-ulb')).toContainText('3 books')
    await expect(page.getByTestId('basket-en-ulb')).toContainText('Genesis')
    await expect(page.getByTestId('basket-en-tn')).toHaveCount(0)
  })

  test('one book of a resource goes straight to review', async ({ page }) => {
    await page.goto(
      `${FRONTEND}/transfer/repo_url=https%3A%2F%2Fcontent.bibletranslationtools.org%2Fsomeone%2Fen_mat_text_tn&book_name=Matthew`
    )
    await expect(page).toHaveURL(/\/settings$/)
    await expect(page.getByTestId('basket-en-tn')).toContainText('1 book')
    await expect(page.getByTestId('basket-en-tn')).toContainText('Matthew')
    await expect(page.getByTestId('basket-en-ulb')).toHaveCount(0)
    // And what was transferred can be changed on the resources step
    await page.getByRole('button', { name: 'Back' }).click()
    await expect(page.getByTestId('cell-en-tn-mat')).toBeChecked()
    await page.getByTestId('cell-en-ulb-mat').check()
    await expect(page.getByTestId('basket-en-ulb')).toContainText('1 book')
  })

  test('a resource that is not available lands on the resources step', async ({ page }) => {
    await page.goto(
      `${FRONTEND}/transfer/repo_url=https:%2F%2Fcontent.bibletranslationtools.org%2Fsomeone%2Fen_tq`
    )
    await expect(page).toHaveURL(/\/resources$/)
    await expect(page.getByTestId('cell-en-ulb-mat')).not.toBeChecked()
    await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled()
  })

  test('an unknown language is reported', async ({ page }) => {
    await page.goto(
      `${FRONTEND}/transfer/repo_url=https:%2F%2Fcontent.bibletranslationtools.org%2Fsomeone%2Fzzz_ulb`
    )
    await expect(page.locator('body')).toContainText('which is not available')
  })
})
