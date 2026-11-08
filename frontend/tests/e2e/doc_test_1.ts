import { expect, test } from '@playwright/test'

test('ui part 1', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Tiếng Việt (Vietnamese)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-vi-ulb-gal').check({ timeout: 120_000 })
  await page.getByTestId('cell-vi-ulb-luk').check()
  await page.getByRole('button', { name: 'Edit' }).first().click()
  await page.getByRole('button', { name: 'Heart' }).click()
  await page.getByRole('button', { name: 'Gateway' }).click()
  await page.getByText('অসমীয়া (Assamese)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  // What was chosen for the first language is retained
  await expect(page.getByTestId('cell-vi-ulb-gal')).toBeChecked({ timeout: 120_000 })
  await page.getByRole('button', { name: 'Select everything' }).click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: 'Generate File' }).click()
})

test('ui part 2', async ({ page }) => {
  await page.goto('http://localhost:8001')
  await page.getByText('English').click()
  await page.getByText('Español Latin America (Latin').click()
  await page.getByRole('button', { name: 'Next' }).click()
  // Languages are kept in sync by default so choosing for English also chooses for Spanish
  await page.getByTestId('cell-en-ulb-gal').check({ timeout: 120_000 })
  await expect(page.getByTestId('cell-es-419-ulb-gal')).toBeChecked()
  await page.getByTestId('cell-en-tn-gal').check()
  await expect(page.getByTestId('cell-es-419-tn-gal')).toBeChecked()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByText('Interleave content by chapter').click()
  await page.getByRole('button', { name: 'Generate File' }).click()
})

test('resources retained in basket on back button to languages and then forward', async ({
  page
}) => {
  await page.goto('http://localhost:8001/')
  await page.getByPlaceholder('Search Gateway Languages').click()
  await page.getByPlaceholder('Search Gateway Languages').fill('Amh')
  await page.getByText('አማርኛ (Amharic)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('cell-am-ulb-2co').check({ timeout: 680000 })
  await page.getByRole('link', { name: 'Languages' }).click()
  await page.getByRole('button', { name: 'Heart' }).click()
  await page.getByPlaceholder('Search Heart Languages').click()
  await page.getByPlaceholder('Search Heart Languages').fill('adh')
  await page.getByText('Adhola adh').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByTestId('cell-am-ulb-2co')).toBeChecked({ timeout: 680000 })
  await expect(page.getByTestId('basket-am-ulb')).toContainText('1 book')
  await page.getByTestId('cell-adh-reg-2jn').check()
  await expect(page.locator('body')).toContainText('Adhola')
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('link', { name: 'Languages' }).click()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByTestId('cell-adh-reg-2jn')).toBeChecked()
})

test('transfer from biel', async ({ page }) => {
  await page.goto(
    'http://localhost:8001/transfer/repo_url=https%3A%2F%2Fcontent.bibletranslationtools.org%2Fchunga_moses%2Fleb-x-bisa_col_text_reg&book_name=Colossians'
  )
  await expect(page.getByText('Bisa').first()).toBeVisible()
  await expect(page.getByTestId('basket-leb-x-bisa-reg')).toContainText('1 book', {
    timeout: 1200000
  })
})

test('transfer from biel 2', async ({ page }) => {
  await page.goto(
    'http://localhost:8001/transfer/repo_url=https:%2F%2Fcontent.bibletranslationtools.org%2FWycliffeAssociates%2Fen_ulb'
  )
  await expect(page.getByText('English').first()).toBeVisible({ timeout: 120000 })
  await expect(page.getByTestId('basket-en-ulb')).toContainText('66 books')
  await expect(page.getByTestId('basket-en-ulb')).toContainText('Genesis')
})
