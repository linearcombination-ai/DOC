import { expect, test } from '@playwright/test'

test('book name correction happened for pt-br, 1co', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Português Brasileiro (').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByTestId('row-1co')).toContainText('1 Coríntios', { timeout: 120000 })
})

// Limit words feature is now disabled because tw is handled better/differently
// and limiting the words provided means that cross references won't
// work and we have just done work to ensure cross reference links work
// nicely.
test.skip('limit tw words switch', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Cebuano').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-ceb-ulb-mat').check()
  await page.getByTestId('language-wide-ceb-tw').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByRole('main')).toContainText('Limit TW words')
  await page.getByRole('link', { name: 'Resources' }).click()
  await page.getByTestId('cell-ceb-ulb-mat').uncheck()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByRole('main')).not.toContainText('Limit TW words')
  await page.getByRole('link', { name: 'Resources' }).click()
  await page.getByTestId('cell-ceb-ulb-mat').check()
  await page.getByTestId('language-wide-ceb-tw').uncheck()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByRole('main')).not.toContainText('Limit TW words')
  await page.getByRole('link', { name: 'Resources' }).click()
  await page.getByTestId('language-wide-ceb-tw').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByRole('main')).toContainText('Limit TW words')
})

test('use section visual separator setting', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Español Latin America (Latin').click({ timeout: 120000 })
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('row-es-419-eph').check({ timeout: 120000 })
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await page.locator('div').filter({ hasText: 'Show visual separator (' }).nth(4).click()
  await page.getByRole('button', { name: 'Generate File' }).click()
})

test('ordering of books in document title(s) and body', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByPlaceholder('Search Gateway Languages').click()
  await page.getByPlaceholder('Search Gateway Languages').fill('tpi')
  await page.getByText('Tok Pisin').click()
  await page.getByRole('button', { name: 'Heart' }).click()
  await page.getByPlaceholder('Search Heart Languages').click()
  await page.getByPlaceholder('Search Heart Languages').fill('ont')
  await page.getByText('Ontenu').click()
  await page.getByRole('button', { name: 'Next' }).click()
  // Languages are kept in sync so choosing Tok Pisin's Unlocked Literal Bible
  // also chooses Ontenu's Regular
  await page.getByTestId('cell-tpi-ulb-eph').check({ timeout: 120_000 })
  await expect(page.getByTestId('cell-ont-reg-eph')).toBeChecked()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByText('Interleave content by chapter').click()
  await page.getByRole('button', { name: 'Generate File' }).click()
  // Wait for the "View HTML Online" button to appear and become visible
  const viewHtmlButton = page.getByRole('button', { name: 'View HTML Online' })
  await viewHtmlButton.waitFor({ state: 'visible' })
  // Start waiting for the popup BEFORE clicking
  const page1Promise = page.waitForEvent('popup')
  // Click to trigger the popup
  await viewHtmlButton.click()
  // Get the new popup page
  const page1 = await page1Promise
  // Perform text expectations on the popup page
  await expect(page1.locator('body')).toContainText(
    'Tok Pisin (Tok Pisin): Unlocked Literal Bible for Efesus'
  )
  await expect(page1.locator('body')).toContainText('Ontenu (Ontenu): Regular for Efeses')

  const headings = await page1.locator('h2').allTextContents()
  // Ensure expected headings are present
  expect(headings).toContain('Efesus')
  expect(headings).toContain('Efeses')

  // Order assertions
  // Find the positions of each
  const index1 = headings.indexOf('Efesus')
  const index2 = headings.indexOf('Efeses')

  // Ensure all were found
  expect(index1).not.toBe(-1)
  expect(index2).not.toBe(-1)

  // Check the order
  expect(index1).toBeLessThan(index2)
})
