import { expect, test } from '@playwright/test'

test('merge of data API data and DOC only data', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await expect(page.getByRole('main')).toContainText('Bahasa Indonesia (Indonesian)')
  await page.getByLabel('Bahasa Indonesia (Indonesian').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.locator('body')).toContainText('Terjemahan Baru', { timeout: 120_000 })
  await expect(page.locator('body')).toContainText('Alkitab Yang Terbuka')
  await expect(page.locator('body')).toContainText('Translation Notes')
})

test.skip('space between end of chunk and beginning of another', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByRole('button', { name: 'Heart' }).click()
  await page.getByPlaceholder('Search Heart Languages').click()
  await page.getByPlaceholder('Search Heart Languages').fill('Aushi')
  await page.getByLabel('Aushi auh').check()
  await page.getByRole('button', { name: 'Next' }).click()
  const checkbox = page.getByTestId('cell-auh-reg-mat')
  await checkbox.check({ timeout: 120_000 })
  await expect(checkbox).toBeEnabled()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByText('Use PrinceXml to produce the').click()
  await page.getByRole('button', { name: 'Generate File' }).click()
  const viewHtmlButton = page.getByRole('button', { name: 'View HTML Online' })
  await viewHtmlButton.waitFor({ state: 'visible' })
  // Start waiting for the popup BEFORE clicking
  const page1Promise = page.waitForEvent('popup')
  // Click to trigger the popup
  await viewHtmlButton.click()
  // Get the new popup page
  const page1 = await page1Promise
  // Ensure there is a space between the end of a verse span chunk and
  // the verse number for the start of the next chunk, i.e., check
  // spacing at chunk boundaries
  await expect(page1.locator('body')).toContainText('bembu?" 12Ulo')
  await expect(page1.locator('body')).toContainText('lelo naishile ababembu." 14Nolu abasambi')
  await expect(page1.locator('body')).toContainText('bakafunga. 16Takuli')
})

// skipping because, for now, TS has requested by verse interleaving
// to be turned off
test.skip('lang then book by verse', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByLabel('English en').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-en-ulb-mat').check()
  await page.getByTestId('cell-en-tn-mat').check()
  await page.getByTestId('language-wide-en-tw').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByText('Use PrinceXml to produce the').click()
  await page.getByText('Interleave content by verse').click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.locator('body')).not.toContainText('Translation notes layout:')
  await expect(page.locator('body')).not.toContainText('Translation questions layout:')
  await page.getByRole('button', { name: 'Generate File' }).click()
  const viewHtmlButton = page.getByRole('button', { name: 'View HTML Online' })
  await viewHtmlButton.waitFor({ state: 'visible' })
  // Start waiting for the popup BEFORE clicking
  const page1Promise = page.waitForEvent('popup')
  // Click to trigger the popup
  await viewHtmlButton.click()
  // Get the new popup page
  const page1 = await page1Promise
  await expect(page1.locator('body')).toContainText(
    '1The book of the genealogy of Jesus Christ, son of David, son of Abraham.'
  )
  await page1.goto(
    'http://localhost:8089/en-tn-mat_en-tw-mat_en-ulb-mat_lvo_1c_chapter_clf_lwt_ssf_1ctn_1ctq.html#en-canaan'
  )
})

// skipping because, for now, TS has requested by verse interleaving
// to be turned off
test.skip('interleave by book shows tn and tq two column option', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Cebuano').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: 'Select all New Testament' }).click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByText('Use PrinceXml to produce the').click()
  await page.getByText('Interleave content by book').click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.locator('body')).toContainText('Translation notes layout:')
  await expect(page.locator('body')).toContainText('Translation questions layout:')
  await page.getByRole('button', { name: 'Generate File' }).click()
})

// skipping because, for now, TS has requested by verse interleaving
// to be turned off
test.skip('two languages settings available', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Cebuano').click()
  await page.getByText('English').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: 'Select all New Testament' }).click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await expect(page.getByRole('main')).toContainText(
    'Interleave content by verse one book at a time'
  )
  await expect(page.getByRole('main')).toContainText('Interleave content by chapter')
  await expect(page.getByRole('main')).toContainText(
    'Interleave content by verse one chapter at a time'
  )
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.getByRole('main')).toContainText('Translation notes layout:')
  await page.getByRole('button', { name: 'Generate File' }).click()
})

// skipping because, for now, TS has requested by verse interleaving
// to be turned off
test.skip('two languages, interleave by verse one chapter at a time', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('English').click()
  await page.getByText('Cebuano').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: 'Select all New Testament' }).click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByText('Docx').click()
  await page.getByText('Interleave content by verse one book at a time').click()
  await expect(page.getByRole('main')).toContainText('Interleave content by book')
  await expect(page.getByRole('main')).toContainText(
    'Interleave content by verse one book at a time'
  )
  await expect(page.getByRole('main')).toContainText('Interleave content by chapter')
  await expect(page.getByRole('main')).toContainText(
    'Interleave content by verse one chapter at a time'
  )
  await page.getByText('Interleave content by verse one chapter at a time').click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.getByRole('main')).toContainText(
    "Use chapter labels, e.g., 'Chapter 1' instead of '1'"
  )
  await expect(page.getByRole('main')).toContainText(
    'Show visual separator (horizontal line) between sections'
  )
  await expect(page.getByRole('main')).toContainText('Include TN book intro')
  await expect(page.getByRole('main')).toContainText('Include BC book intro')
  await expect(page.getByRole('main')).toContainText('Include TN chapter intro')
  await expect(page.getByRole('main')).toContainText('Include BC chapter commentary')
  await expect(page.getByRole('main')).toContainText('Include RG chapter commentary')
  await page.getByRole('button', { name: 'Generate File' }).click()
})

// skipping because, for now, TS has requested by verse interleaving
// to be turned off
test.skip('include tn book and chapter intros is not checked by default for this case', async ({
  page
}) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Français (French)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('row-fr-gal').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByText('Interleave content by verse').click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.locator('#use-two-column-layout-for-tn')).toHaveCount(0)
  await expect(page.locator('#show-tn-book-intro')).not.toBeChecked()
  await expect(page.locator('#show-tn-chapter-intro')).not.toBeChecked()
})
