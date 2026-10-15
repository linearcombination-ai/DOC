import { expect, test } from '@playwright/test'

test('es-419 resource types', async ({ page }) => {
  await page.goto('http://localhost:8001')
  await page.getByText(/.*Español.*/).click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-es-419-tn-mat').check({ timeout: 60000 })
  await page.getByTestId('cell-es-419-tq-mat').check()
  await expect(page.getByTestId('basket-es-419-tn')).toContainText('1 book')
  await expect(page.getByTestId('basket-es-419-tq')).toContainText('1 book')
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: 'Generate File' }).click()
})

test('resources that do not exist are not offered', async ({ page }) => {
  await page.goto('http://localhost:8001')
  await page.getByPlaceholder('Search Gateway Languages').fill('Rus')
  await page.getByText(/.*Русский.*/).click()
  await page.getByRole('button', { name: 'Next' }).click()
  // Russian has Translation Notes for Matthew but no Unlocked Literal Bible
  await expect(page.getByTestId('cell-ru-tn-mat')).toBeVisible({ timeout: 120000 })
  await expect(page.getByTestId('cell-ru-ulb-mat')).toHaveCount(0)
})

test.skip('can select gateway tab after first selecting heart language and hitting next', async ({
  page
}) => {
  await page.goto('http://localhost:8001/')
  await page.getByRole('button', { name: 'Heart' }).click()
  await page.getByText('Adhola').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('link', { name: 'Languages' }).click()
  await page.getByText('Cebuano').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-ceb-ulb-1th').check({ timeout: 120_000 })
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByLabel('Interleave content by chapter').check()
  await page.getByRole('button', { name: 'Generate File' }).click()
})

test('optional settings', async ({ page }) => {
  await page.goto('http://localhost:8001/languages')
  await page.getByText('Bichelamar (Bislama)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-bi-reg-mat').check({ timeout: 120000 })
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await expect(page.getByRole('main')).toContainText('▶ Show Optional Settings')
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.getByRole('main')).toContainText(
    "Use chapter labels, e.g., 'Chapter 1' instead of '1'"
  )
})
